import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

// QA round 16: a syntax error anywhere keeps a read-only block's contents
// locked (and its repair free), and whitespace inside a locked field stays
// free outside YAML's indents.

const saved = (page) => page.locator('.demo__output-text');

/** Puts the caret at UTF-16 offset `at`. */
async function caretTo(page, at) {
  await editor(page).input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, at);
}

/** Puts the caret `offset` units into the first `marker`. */
async function caretAt(page, marker, offset = 0) {
  const text = await editor(page).value();
  const at = text.indexOf(marker);
  expect(at, marker).toBeGreaterThanOrEqual(0);
  await caretTo(page, at + offset);
}

// How to break each protected fixture outside its locks (caret at
// `marker` + `offset`, then the key), and the locked value inside the
// read-only `limits` block.
const BREAK = {
  yaml: { marker: 'name: edge', offset: 6, type: '"', max: 'max: 10' },
  json: { marker: '"edge"', offset: 5, press: 'Delete', max: '"max": 10' },
  toml: { marker: '"edge"', offset: 5, press: 'Delete', max: 'max = 10' },
  hcl: { marker: '"edge"', offset: 5, press: 'Delete', max: 'max = 10' },
  xml: { marker: '</name>', offset: 6, press: 'Delete', max: '<max>10' },
};

for (const [language, broken] of Object.entries(BREAK)) {
  test.describe(`${language}: a document that does not parse`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test('keeps the read-only block locked, and its repair free', async ({ page }) => {
      const original = await editor(page).value();
      await caretAt(page, broken.marker, broken.offset);
      if (broken.type) await page.keyboard.type(broken.type);
      else await page.keyboard.press(broken.press);
      await page.waitForTimeout(300);
      const unparseable = await editor(page).value();
      expect(unparseable).not.toBe(original);
      const reported = await saved(page).textContent();

      // Inside the block: a digit typed, a line deleted.
      await caretAt(page, broken.max, broken.max.length);
      await page.keyboard.type('9');
      await caretAt(page, broken.max, 0);
      await page.keyboard.press('Shift+End');
      await page.keyboard.press('Delete');
      await page.waitForTimeout(300);
      expect(await editor(page).value()).toBe(unparseable);
      await expect(saved(page)).toHaveText(reported);

      // Repairing the syntax goes through.
      await caretAt(page, broken.marker.slice(0, broken.offset), broken.offset);
      if (broken.type) await page.keyboard.press('Delete');
      else await page.keyboard.type(language === 'xml' ? '>' : '"');
      await page.waitForTimeout(300);
      expect(await editor(page).value()).toBe(original);
    });
  });
}

// Whitespace typed inside a locked field: [language, marker, offset, key].
const WHITESPACE = [
  ['json', '    "max"', 0, ' '],
  ['json', '"max": 10', 9, ' '],
  ['json', '"max": 10', 6, ' '],
  ['json', '"id": "srv', 5, ' '],
  ['json', '    "burst"', 2, 'Backspace'],
  ['toml', 'max = 10', 0, ' '],
  ['toml', 'max = 10', 4, ' '],
  ['toml', 'id = "srv', 3, ' '],
  ['hcl', '  max = 10', 0, ' '],
  ['hcl', 'max = 10', 4, ' '],
  ['xml', '  <max>', 0, ' '],
  ['xml', '</id>', 5, ' '],
  ['yaml', 'id: srv', 3, ' '],
  ['yaml', 'max: 10', 7, ' '],
  ['yaml', '  burst', 0, 'Enter'],
];

for (const [language, marker, offset, key] of WHITESPACE) {
  test.describe(`${language}: whitespace in a locked field`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test(`${JSON.stringify(key)} at ${JSON.stringify(marker)}+${offset} goes through`, async ({ page }) => {
      const before = await editor(page).value();
      await caretAt(page, marker, offset);
      if (key === ' ') await page.keyboard.type(' ');
      else await page.keyboard.press(key);
      await page.waitForTimeout(300);
      expect(await editor(page).value()).not.toBe(before);
    });
  });
}

test.describe('yaml: suggestions inside a read-only block', () => {
  setupEditor(test, '?schema=protected');

  test('show the reason, and accepting one changes nothing', async ({ page }) => {
    // On a locked key's text (the block holds every key it may).
    await caretAt(page, '  max: 10', 4);
    const before = await editor(page).value();
    await page.keyboard.press('Control+Space');
    const popup = page.locator('.config-editor__completions');
    await expect(popup).toBeVisible();
    await expect(popup).toContainText('Set by your plan');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toBe(before);
  });
});

test.describe('yaml: a locked line', () => {
  setupEditor(test, '?schema=protected');

  test('keeps its indent', async ({ page }) => {
    const before = await editor(page).value();
    await caretAt(page, '  max', 0);
    await page.keyboard.type(' ');
    await caretAt(page, '  burst', 1);
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toBe(before);
  });
});

test.describe('ConfigDocuments.update', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  test('moves activeId to the new list before it returns', async ({ page }) => {
    await page.goto(example('documents.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(() => {
      const view = window.documentsView;
      view.update({ documents: [{ id: 'a', fileName: 'a.yaml', value: 'x: 1\n' }] });
      const first = view.activeId;
      view.update({ documents: [{ id: 'z', fileName: 'z.yaml', value: 'y: 2\n' }] });
      return [first, view.activeId];
    });
    expect(seen).toEqual(['a', 'z']);
    await expect(page.locator('[role=tab][aria-selected=true]')).toHaveText(/z\.yaml/);
  });
});

test.describe('a string spelled like the reserved infinity form', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  test('stays a string through parse, serialize and convert', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(async () => {
      const tools = await import('../config-editor/pkg/config_editor.js');
      const forged = '\ufdd0inf';
      const parsed = tools.parse('json', JSON.stringify({ s: forged, n: 1 }));
      const yaml = tools.serialize('yaml', { s: forged, rate: Infinity });
      const converted = tools.convert('json', JSON.stringify({ s: forged }), 'toml');
      return {
        parsed: parsed.s === forged,
        yaml,
        back: tools.parse('yaml', yaml),
        converted: converted.losses,
        toml: tools.parse('toml', converted.text).s === forged,
      };
    });
    expect(seen.parsed).toBe(true);
    expect(seen.back.s).toBe('\ufdd0inf');
    expect(seen.back.rate).toBe(Infinity);
    expect(seen.converted).toEqual([]);
    expect(seen.toml).toBe(true);
  });
});
