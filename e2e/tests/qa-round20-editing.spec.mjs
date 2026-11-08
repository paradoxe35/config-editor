import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

// QA round 20 editing: items moved in another view take their comments, an
// emptied YAML list keeps the gap before its comment, a YAML string over
// lines keeps its lines, the HCL view keeps whole floats, the XML view reads
// `0x50` as a number, Enter in a plain YAML scalar over lines, and the
// headless tools read a YAML comment line ending in a lone CR.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Selects `needle` (or puts the caret after it with `collapse`).
async function select(e, needle, collapse = false) {
  await e.input.evaluate(
    (element, [find, end]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      const stop = start + find.length;
      element.setSelectionRange(end ? stop : start, stop);
    },
    [needle, collapse],
  );
}

test.describe('YAML edited in other views', () => {
  setupEditor(test, '?language=yaml');

  test('a list in a list item reversed as JSON takes its comments', async ({ page }) => {
    const e = editor(page);
    const host = 'l:\n  - name: a\n    sub:\n      # about 1\n      - 1  # one\n      - 2  # two\nn: 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('# two');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"sub"');
    const shown = await e.value();
    const list = shown.match(/"sub": (\[[^\]]*\])/)[1];
    await select(e, list);
    await page.keyboard.type('[2, 1]');
    await expect
      .poll(() => saved(page))
      .toBe('l:\n  - name: a\n    sub:\n      - 2  # two\n      # about 1\n      - 1  # one\nn: 1\n');
  });

  test('a list emptied as TOML keeps the gap before its comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('ports:    # pc\n  - 80\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('- 80');
    await pick(page, 'toml');
    await expect.poll(() => e.value()).toContain('ports = [80]');
    await select(e, '80');
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).toBe('ports: []    # pc\nn: 1\n');
  });

  test('a string over lines edited as JSON keeps its lines', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\nml: "l1\n  l2"  # mc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('# mc');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"l1 l2"');
    await select(e, 'l1 l2', true);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe('# c\nml: "l1\n  l2Q"  # mc\nn: 1\n');
  });

  test('a whole float edited as HCL stays a float', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nf: 1.0\nn: x\n');
    await expect.poll(() => saved(page)).toContain('f: 1.0');
    await pick(page, 'hcl');
    await expect.poll(() => e.value()).toContain('f = 1');
    await select(e, 'f = 1', true);
    await page.keyboard.press('Backspace');
    await page.keyboard.type('2');
    await expect.poll(() => saved(page)).toBe('# top\nf: 2.0\nn: x\n');
  });

  test('0x51 typed in the XML view is a number', async ({ page }) => {
    const e = editor(page);
    await e.paste('# top\nport: 80  # pc\nn: x\n');
    await expect.poll(() => saved(page)).toContain('# pc');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<port>80</port>');
    await select(e, '>80<');
    await page.keyboard.type('>0x51<');
    await expect.poll(() => saved(page)).toBe('# top\nport: 81  # pc\nn: x\n');
  });

  test('Enter at the end of a plain scalar line continues it, before a comment keeps it one', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\nml: l1\n  l2  # mc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('# mc');
    await select(e, 'ml: l1', true);
    await page.keyboard.press('Enter');
    await page.keyboard.type('X');
    await expect.poll(() => e.value()).toBe('# c\nml: l1\n  X\n  l2  # mc\nn: 1\n');
    await select(e, '  l2', true);
    await page.keyboard.press('Enter');
    await page.keyboard.type('Y');
    await expect.poll(() => e.value()).toBe('# c\nml: l1\n  X\n  l2\n  Y  # mc\nn: 1\n');
  });
});

test.describe('the vanilla example', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  test('the headless tools read a YAML comment line ending in a lone CR', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(async () => {
      const tools = await import('../config-editor/pkg/config_editor.js');
      const text = '# top\rname: edge\rn: 1\n';
      return {
        parsed: tools.parse('yaml', text),
        converted: tools.parse('json', tools.convert('yaml', text, 'json').text),
        problems: tools.validate('yaml', text, '{"properties":{"n":{"type":"string"}}}').length,
      };
    });
    expect(seen.parsed).toEqual({ name: 'edge', n: 1 });
    expect(seen.converted).toEqual({ name: 'edge', n: 1 });
    expect(seen.problems).toBe(1);
  });

  test('the Edit as select follows undo back across a switch', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('#edit-as').selectOption('json');
    await expect(page.locator('body')).toHaveAttribute('data-editing', 'json');
    await page.locator('.config-editor__input').focus();
    await page.keyboard.press('Control+z');
    await expect(page.locator('body')).toHaveAttribute('data-editing', 'yaml');
    await expect(page.locator('#edit-as')).toHaveValue('');
  });
});
