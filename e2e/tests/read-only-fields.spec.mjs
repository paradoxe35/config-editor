import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, FRAMEWORK_PAGES, pointAt, setupEditor } from './helpers.mjs';

// Read-only fields against the protected-fields fixture (`?schema=protected`):
// `id` (readOnly, "Assigned by the server") and the `limits` block
// (readOnly, "Set by your plan"); `&lock=/name` locks `name` as the host.

const saved = (page) => page.locator('.demo__output-text');
const tip = (page) => page.locator('.config-editor__protect-tip');

// Clicks into `text` on the line holding it (`offset` characters in).
async function clickInto(page, text, offset = 1) {
  const lines = (await editor(page).value()).split('\n');
  const line = lines.findIndex((candidate) => candidate.includes(text));
  expect(line, `"${text}" is in the document`).toBeGreaterThanOrEqual(0);
  const at = await pointAt(page, line, lines[line].indexOf(text) + offset);
  await page.mouse.click(at.x, at.y);
  return line;
}

async function expectUnchanged(page, before, reason) {
  await page.waitForTimeout(150);
  expect(await editor(page).value()).toBe(before);
  if (reason) await expect(tip(page)).toContainText(reason);
}

test.describe('read-only fields', () => {
  setupEditor(test, '?schema=protected');

  test('a lock marks read-only lines and their values are muted', async ({ page }) => {
    const locks = page.locator('.config-editor__lock-marker');
    // `id`, and the three lines of `limits`.
    await expect(locks).toHaveCount(4);
    await expect(page.locator('[data-line="1"] .config-editor__lock-marker')).toHaveAttribute(
      'title',
      'Read-only: Assigned by the server',
    );
    await expect(page.locator('[data-line="5"] .config-editor__lock-marker')).toHaveAttribute('title', /Set by your plan/);
    await expect(page.locator('[data-line="2"] .config-editor__lock-marker')).toHaveCount(0);
    const muted = page.locator('.config-editor__code-line[data-line="1"] .config-editor__locked');
    const mutedText = (await muted.allTextContents()).join('');
    expect(mutedText).toContain('srv-42');
    expect(mutedText).not.toContain('id');
    const colors = await muted.last().evaluate((element) => ({
      shown: getComputedStyle(element).color,
      locked: getComputedStyle(element.closest('.config-editor')).getPropertyValue('--config-editor-locked'),
    }));
    expect(colors.locked.trim()).not.toBe('');
    // The key keeps its own color.
    const key = page.locator('.config-editor__code-line[data-line="1"] .config-editor__key');
    expect(await key.evaluate((element) => element.closest('.config-editor__locked') === null)).toBe(true);
  });

  test('typing, Backspace, Delete, paste and cut in a read-only value are refused with the reason', async ({ page }) => {
    const ed = editor(page);
    const before = await ed.value();
    await clickInto(page, 'srv-42', 3);
    await page.keyboard.type('x');
    await expectUnchanged(page, before, 'Assigned by the server');
    await page.keyboard.press('Backspace');
    await expectUnchanged(page, before, 'Assigned by the server');
    await page.keyboard.press('Delete');
    await expectUnchanged(page, before);
    // A selection inside the value: paste over it, cut it.
    await page.keyboard.press('Shift+ArrowRight');
    await page.keyboard.press('Shift+ArrowRight');
    await ed.input.evaluate((element) => {
      const data = new DataTransfer();
      data.setData('text/plain', 'ZZ');
      element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
      if (!element.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertFromPaste', data: 'ZZ', bubbles: true, cancelable: true }))) return;
      element.setRangeText('ZZ', element.selectionStart, element.selectionEnd, 'end');
      element.dispatchEvent(new InputEvent('input', { inputType: 'insertFromPaste', data: 'ZZ', bubbles: true }));
    });
    await expectUnchanged(page, before);
    await page.keyboard.press('Control+X');
    await expectUnchanged(page, before);
    // Nothing reached the host.
    await expect(saved(page)).toHaveText(before);
    // The free fields still edit.
    await clickInto(page, 'edge', 4);
    await page.keyboard.type('-1');
    await expect(saved(page)).toContainText('name: edge-1');
  });

  test('deleting the key, a selection over it, or its parent block is refused', async ({ page }) => {
    const ed = editor(page);
    const before = await ed.value();
    // Select from `name` back over the `id` line and delete.
    await clickInto(page, 'name', 0);
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Backspace');
    await expectUnchanged(page, before, 'Assigned by the server');
    // Everything.
    await page.keyboard.press('Control+A');
    await page.keyboard.press('Delete');
    await expectUnchanged(page, before);
    // A line inside the block.
    await clickInto(page, 'burst', 0);
    await page.keyboard.press('Home');
    await page.keyboard.press('Shift+End');
    await page.keyboard.press('Delete');
    await expectUnchanged(page, before, 'Set by your plan');
    // The block's key.
    await clickInto(page, 'limits', 2);
    await page.keyboard.type('x');
    await expectUnchanged(page, before, 'Set by your plan');
  });

  test('commenting a read-only line is refused; commenting another line works', async ({ page }) => {
    const ed = editor(page);
    const before = await ed.value();
    await clickInto(page, 'srv-42', 1);
    await page.keyboard.press('Control+/');
    await expectUnchanged(page, before, 'Assigned by the server');
    await clickInto(page, 'listen', 1);
    await page.keyboard.press('Control+/');
    await expect.poll(ed.value).toContain('# listen: 7835');
  });

  test('a line added inside a read-only block is refused, one after it is not', async ({ page }) => {
    const ed = editor(page);
    const before = await ed.value();
    await clickInto(page, 'burst: 20', 9);
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('x: 1');
    await page.waitForTimeout(150);
    expect(await ed.value()).not.toContain('  x: 1');
    await expect(tip(page)).toContainText('Set by your plan');
    // Undo leaves the read-only text alone too.
    await page.keyboard.press('Control+Z');
    await page.keyboard.press('Control+Z');
    expect((await ed.value()).includes('id: srv-42')).toBe(true);
    await page.keyboard.press('Escape');
    await ed.input.evaluate((element) => element.setSelectionRange(element.value.length, element.value.length));
    await page.keyboard.type('extra: 1');
    await expect.poll(ed.value).toContain('extra: 1');
    expect((await ed.value()).startsWith(before.split('\n')[0])).toBe(true);
  });

  test('suggestions leave read-only keys out', async ({ page }) => {
    const ed = editor(page);
    await ed.input.click();
    await ed.input.evaluate((element) => element.setSelectionRange(element.value.length, element.value.length));
    await page.keyboard.press('Control+Space');
    await expect.poll(ed.keys).toContain('clients');
    const keys = await ed.keys();
    expect(keys).not.toContain('created');
    expect(keys).not.toContain('id');
    await page.keyboard.press('Escape');
    await page.keyboard.type('cre');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await ed.keys()).not.toContain('created');
  });

  test('replace all skips matches in read-only values and says so', async ({ page }) => {
    const ed = editor(page);
    await ed.input.click();
    await page.keyboard.press('Control+H');
    await page.getByRole('textbox', { name: 'Find' }).fill('s');
    await page.getByRole('textbox', { name: 'Replace' }).fill('S');
    await page.getByRole('button', { name: 'Replace all' }).click();
    await expect(page.locator('.config-editor__find')).toContainText(/skipped \d+ in read-only/);
    const text = await ed.value();
    expect(text).toContain('id: srv-42');
    expect(text).toContain('burst: 20');
    expect(text).toContain('tlS:');
    expect(text).toContain('liSten');
  });

  test('hovering a read-only value shows why', async ({ page }) => {
    const lines = (await editor(page).value()).split('\n');
    const at = await pointAt(page, 0, lines[0].indexOf('srv-42') + 2);
    await page.mouse.move(at.x, at.y, { steps: 4 });
    await expect(tip(page)).toHaveText('Assigned by the server');
    await expect(tip(page)).toHaveAttribute('role', 'tooltip');
    const free = await pointAt(page, 1, lines[1].indexOf('edge') + 1);
    await page.mouse.move(free.x, free.y, { steps: 4 });
    await expect(tip(page)).toHaveCount(0);
  });
});

test.describe('read-only fields the host names', () => {
  setupEditor(test, '?schema=protected&lock=/name');

  test('a host path locks its field with the default reason', async ({ page }) => {
    const ed = editor(page);
    const before = await ed.value();
    await clickInto(page, 'edge', 2);
    await page.keyboard.type('x');
    await expectUnchanged(page, before, 'This field is read-only');
    await expect(page.locator('[data-line="2"] .config-editor__lock-marker')).toHaveCount(1);
  });
});

for (const language of ['json', 'toml', 'hcl', 'xml']) {
  test.describe(`read-only fields in ${language}`, () => {
    setupEditor(test, `?schema=protected&language=${language}`);

    test('typing in a read-only value is refused', async ({ page }) => {
      const ed = editor(page);
      const before = await ed.value();
      await expect(page.locator('.config-editor__lock-marker').first()).toBeVisible();
      await clickInto(page, 'srv-42', 3);
      await page.keyboard.type('x');
      await expectUnchanged(page, before, 'Assigned by the server');
      await clickInto(page, 'burst', 1);
      await page.keyboard.type('x');
      await expectUnchanged(page, before, 'Set by your plan');
    });
  });
}

test.describe('read-only fields while editing as another language', () => {
  setupEditor(test, '?schema=protected&edit=json');

  test('the locks hold in the Edit-as view and free edits save back', async ({ page }) => {
    const ed = editor(page);
    await expect.poll(ed.value).toContain('"id": "srv-42"');
    const before = await ed.value();
    await expect(page.locator('.config-editor__lock-marker').first()).toBeVisible();
    await clickInto(page, 'srv-42', 3);
    await page.keyboard.type('x');
    await expectUnchanged(page, before, 'Assigned by the server');
    await clickInto(page, 'edge', 4);
    await page.keyboard.type('2');
    await expect(saved(page)).toContainText('name: edge2');
    await expect(saved(page)).toContainText('id: srv-42');
  });
});

// The npm packages' example pages: read-only paths and ranges set at
// runtime (`setReadOnlyPaths` / `setReadOnlyRanges`, React props).
const examples = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(examples, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

for (const name of ['vanilla.html', ...FRAMEWORK_PAGES]) {
  test.describe(`read-only fields in ${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('paths and ranges lock at runtime and unlock again', async ({ page }) => {
      await page.goto(pathToFileURL(join(examples, 'examples', name)).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const input = page.locator('.config-editor__input');
      await input.waitFor();
      const before = await input.inputValue();
      await page.locator('#lock-address').check();
      await expect(page.locator('.config-editor__lock-marker')).toHaveCount(1);
      await clickInto(page, 'tunnel', 2);
      await page.keyboard.type('x');
      await expectUnchanged(page, before, 'This field is read-only');
      await page.locator('#lock-first-line').check();
      await expect(page.locator('.config-editor__lock-marker')).toHaveCount(2);
      await clickInto(page, 'Burrow', 2);
      await page.keyboard.type('x');
      await expectUnchanged(page, before, 'This text is read-only');
      // Unlocked, both edit again.
      await page.locator('#lock-address').uncheck();
      await page.locator('#lock-first-line').uncheck();
      await expect(page.locator('.config-editor__lock-marker')).toHaveCount(0);
      await clickInto(page, 'tunnel', 2);
      await page.keyboard.type('x');
      await expect(input).toHaveValue(/tuxnnel/);
    });
  });
}

// Folding: the textarea holds the text on screen, folded lines cut out.
const foldRow = (page, line) => page.locator(`.config-editor__gutter-content > div[data-line="${line}"]`);
const foldToggle = (page, line) => page.locator(`.config-editor__fold-toggle[data-line="${line}"]`);
async function fold(page, line) {
  await foldRow(page, line).hover();
  await foldToggle(page, line).click();
  await expect(foldToggle(page, line)).toHaveClass(/config-editor__fold-toggle-closed/);
}
const documentText = (page) => page.locator('.demo__output-text').textContent();

for (const [query, limitsLine] of [['?schema=protected', 4], ['?schema=protected&edit=json', 5]]) {
  test.describe(`read-only fields in a fold (${query})`, () => {
    setupEditor(test, query);

    test('an edit taking a fold with a locked value is refused and the fold stays', async ({ page }) => {
      const ed = editor(page);
      await fold(page, limitsLine);
      const screen = await ed.value();
      expect(screen).not.toContain('burst');
      const whole = await documentText(page);
      // Delete the folded line (and with it the hidden ones).
      await clickInto(page, 'limits', 1);
      await page.keyboard.press('Home');
      await page.keyboard.press('Shift+ArrowDown');
      await page.keyboard.press('Delete');
      await page.waitForTimeout(200);
      expect(await ed.value()).toBe(screen);
      await expect(tip(page)).toContainText('Set by your plan');
      await expect(foldToggle(page, limitsLine)).toHaveClass(/config-editor__fold-toggle-closed/);
      expect(await documentText(page)).toBe(whole);
      // Unfolded, the values inside are still locked.
      await page.locator(`.config-editor__fold-chip[data-line="${limitsLine}"]`).click();
      const before = await ed.value();
      await clickInto(page, 'burst', 1);
      await page.keyboard.type('x');
      await expectUnchanged(page, before, 'Set by your plan');
      // A free edit still works.
      await clickInto(page, 'edge', 4);
      await page.keyboard.type('5');
      await expect(page.locator('.demo__output-text')).toContainText('edge5');
    });
  });
}

test.describe('read-only fields and the host', () => {
  setupEditor(test, '?schema=protected');

  test('a refused edit leaves the document clean; a reloaded baseline may change a read-only value', async ({ page }) => {
    const ed = editor(page);
    const status = page.locator('.demo__host-status');
    await clickInto(page, 'srv-42', 3);
    await page.keyboard.type('x');
    await page.waitForTimeout(150);
    await expect(status).toHaveAttribute('data-dirty', 'false');
    // An edit of mine, then the server's version changes `id` (srv-43).
    await clickInto(page, 'edge', 4);
    await page.keyboard.type('9');
    await expect(status).toHaveAttribute('data-dirty', 'true');
    await page.locator('.demo__output-toggle').click();
    await page.locator('.demo__server-change').click();
    await expect(page.locator('.config-editor__notice')).toBeVisible();
    await page.locator('.config-editor__notice').getByRole('button', { name: /reload/i }).click();
    // The host's own text: the lock does not hold it back.
    await expect.poll(ed.value).toContain('id: srv-43');
    expect(await ed.value()).not.toContain('edge9');
    await expect(status).toHaveAttribute('data-dirty', 'false');
  });
});

test.describe('saving with a read-only value changed in Edit-as (vanilla.html)', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('save refuses an edit to a field the host locked meanwhile', async ({ page }) => {
    await page.goto(pathToFileURL(join(examples, 'examples', 'vanilla.html')).href);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('#edit-as').selectOption('json');
    const input = page.locator('.config-editor__input');
    await expect(input).toHaveValue(/"public_address"/);
    // An edit to public_address still waiting out the typing pause, then
    // the host locks the field and saves: the save must not carry it.
    await clickInto(page, 'tunnel', 2);
    const sent = await page.evaluate(() => {
      const element = document.querySelector('.config-editor__input');
      element.dispatchEvent(new InputEvent('beforeinput', { inputType: 'insertText', data: 'Q', bubbles: true, cancelable: true }));
      element.setRangeText('Q', element.selectionStart, element.selectionStart, 'end');
      element.dispatchEvent(new InputEvent('input', { inputType: 'insertText', data: 'Q', bubbles: true }));
      window.editor.setReadOnlyPaths(['/public_address']);
      const before = window.saves.length;
      window.editor.save();
      return window.saves.length - before;
    });
    expect(sent).toBe(0);
    await expect(page.locator('#saved')).not.toContainText('tuQnnel');
    await expect(page.locator('.config-editor__diagnostic-marker[data-message*="read-only"]')).toHaveCount(1);
  });
});
