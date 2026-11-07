import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Edit-as saves as a user makes them: a value edited in another language's
// view keeps the host's form, a refused save says why, a switch is an undo
// step, and the demo labels the language really on screen.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);
const status = (page) => page.locator('.demo__host-status');

// Puts the caret just after `needle` (its `offset`-th character with one).
async function caretAfter(e, needle, offset = needle.length) {
  await e.input.evaluate(
    (element, [find, at]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + at, start + at);
    },
    [needle, offset],
  );
}

test.describe('a YAML block scalar edited in another view', () => {
  setupEditor(test, '?language=yaml');

  for (const view of ['json', 'toml', 'hcl', 'xml']) {
    test(`stays a block through the ${view} view, its last line break kept`, async ({ page }) => {
      const e = editor(page);
      await e.paste('# c\nscript: |\n  line one\n    indented\n  line three\nfolded: >-\n  folded text\n  continues\n');
      await expect.poll(() => saved(page)).toContain('line three');
      await pick(page, view);
      await expect.poll(() => e.value()).toContain('three');
      await caretAfter(e, 'three');
      await page.keyboard.type('X');
      await expect
        .poll(() => saved(page))
        .toBe('# c\nscript: |\n  line one\n    indented\n  line threeX\nfolded: >-\n  folded text\n  continues\n');
      await caretAfter(e, 'folded text', 6);
      await page.keyboard.type('Y');
      await expect.poll(() => saved(page)).toContain('folded: >-\n  foldedY text\n  continues\n');
    });
  }
});

test.describe('XML text with whitespace and CDATA edited in another view', () => {
  setupEditor(test, '?language=xml');

  test('keeps its spaces, its last line break and its CDATA', async ({ page }) => {
    const e = editor(page);
    const xml = '<!-- c -->\n<spaced>  keep  </spaced>\n<lines>a\nb\n</lines>\n<script><![CDATA[if a < b]]></script>\n';
    await e.paste(xml);
    await expect.poll(() => saved(page)).toContain('CDATA');
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('spaced: "  keep  "');
    await caretAfter(e, 'keep');
    await page.keyboard.type('s');
    await caretAfter(e, 'a < b');
    await page.keyboard.type('c');
    await expect
      .poll(() => saved(page))
      .toBe('<!-- c -->\n<spaced>  keeps  </spaced>\n<lines>a\nb\n</lines>\n<script><![CDATA[if a < bc]]></script>\n');
  });
});

test.describe('a key added to an empty HCL block over lines', () => {
  setupEditor(test, '?language=hcl');

  test('goes inside it, the comments and blank lines kept', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nblock {\n}\n\n# mid\nlisten = 1\n');
    await expect.poll(() => saved(page)).toContain('# mid');
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('block: {}');
    await caretAfter(e, 'block: {');
    await page.keyboard.type('k: 1');
    await expect.poll(() => saved(page)).toBe('# top\nblock {\n  k = 1\n}\n\n# mid\nlisten = 1\n');
  });
});

test.describe('saving text that does not parse while editing as another language', () => {
  setupEditor(test, '?language=yaml&edit=toml');

  test('is refused: the caret goes to the syntax error and its message shows', async ({ page }) => {
    const e = editor(page);
    await caretAfter(e, 'listen = 7835');
    await page.keyboard.type(' =');
    // The caret far from the problem.
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    const line = (await e.value()).split('\n').findIndex((text) => text.includes('listen = 7835')) + 1;
    expect(line).toBeGreaterThan(0);
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', '0');
    await expect(page.locator('.config-editor__diagnostic-marker-shown')).toHaveCount(1);
    expect(await e.caret()).toMatchObject({ line });
  });
});

test.describe('switching the editing language', () => {
  setupEditor(test, '?language=yaml');

  test('is an undo step, and the caret stays on its field', async ({ page }) => {
    const e = editor(page);
    const original = await e.value();
    await caretAfter(e, 'listen', 2);
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"listen": 7835');
    // The caret is on the same field in the new view.
    const json = await e.value();
    const caret = await e.input.evaluate((element) => element.selectionStart);
    expect(json.slice(caret, caret + 8)).toBe('"listen"');
    await e.input.focus();
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(original);
    await expect(page.locator('.config-editor__file')).not.toContainText('editing as');
    await expect(page.locator('.demo__edit-select')).toHaveValue('');
    await page.keyboard.press('Control+y');
    await expect.poll(() => e.value()).toBe(json);
    await expect(page.locator('.config-editor__file')).toContainText('editing as JSON');
    await expect(page.locator('.demo__edit-select')).toHaveValue('json');
    // An edit after the switch undoes first, then the switch.
    await caretAfter(e, '7835');
    await page.keyboard.type('1');
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(json);
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(original);
    await expect.poll(() => saved(page)).toBe(original);
  });
});

test.describe('a switch that waits', () => {
  setupEditor(test, '?language=yaml');

  test('keeps the host language on screen and labelled until it completes', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\npublic_address: null\n');
    await pick(page, 'toml');
    const marker = page.locator('.config-editor__diagnostic-marker', { hasText: 'null' });
    await expect(marker).toHaveAttribute('data-message', /Cannot switch to TOML yet/);
    // The YAML text is what is on screen and what the host gets.
    await expect(page.locator('.config-editor__file')).not.toContainText('editing as');
    await expect(page.locator('.config-editor__file-icon')).toHaveText('Y');
    await caretAfter(e, '7835');
    await page.keyboard.type('1');
    await expect.poll(() => saved(page)).toBe('listen: 78351\npublic_address: null\n');
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('null');
      element.focus();
      element.setSelectionRange(start, start + 4);
    });
    await page.keyboard.type('a');
    await expect.poll(() => e.value()).toContain('public_address = "a"');
    await expect(page.locator('.config-editor__file')).toContainText('editing as TOML');
  });
});

test.describe('Ctrl+/ on a folded block', () => {
  setupEditor(test, '?language=yaml');

  test('comments the block and selects its whole lines', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: 1\ntls:\n  cert: x\n  key: y\nb: 2\n');
    await caretAfter(e, 'tls', 1);
    await page.keyboard.press('Control+Shift+BracketLeft');
    await page.keyboard.press('Control+/');
    await expect.poll(() => e.value()).toBe('a: 1\n# tls:\n#   cert: x\n#   key: y\nb: 2\n');
    const selected = await e.input.evaluate((element) =>
      element.value.slice(element.selectionStart, element.selectionEnd),
    );
    expect(selected.startsWith('# tls:')).toBe(true);
  });
});
