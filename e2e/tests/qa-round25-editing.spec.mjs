import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 25 (editing), through the npm package's example page with real
// keys: folds stay closed through an Edit-as switch (carried by their
// field); an XML child removed and added back in one Edit-as session is a
// child again, its attribute kept; a YAML merge key stays when a merged
// value is edited from another view. Run `npm run build` in js/ first.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input').first();

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  await page.goto(example(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await input(page).waitFor();
  return errors;
}

/** Selects `needle` (its `nth` occurrence) in the textarea. */
async function select(page, needle, nth = 0) {
  await input(page).evaluate(
    (area, [needle, nth]) => {
      let at = -1;
      for (let index = 0; index <= nth; index++) at = area.value.indexOf(needle, at + 1);
      if (at < 0) throw new Error(`no ${needle}`);
      area.focus();
      area.setSelectionRange(at, at + needle.length);
      area.dispatchEvent(new Event('select', { bubbles: true }));
    },
    [needle, nth],
  );
}

const host = (page) => page.evaluate(() => window.editor.value);

test('folds stay closed through an Edit-as switch and back', async ({ page }) => {
  const value = 'a: 1\nweb:\n  http: 80\n  hosts:\n    - x\n    - y\nz: 2\n';
  const errors = await open(page, { language: 'yaml', value });
  await select(page, 'web');
  await page.keyboard.press('Control+Shift+BracketLeft');
  await expect(input(page)).not.toHaveValue(/http/);
  await page.evaluate(() => window.editor.update({ editAs: 'json' }));
  await expect(input(page)).toHaveValue(/"web": \{/);
  await expect(input(page)).toHaveValue(/"z": 2/);
  await expect(input(page)).not.toHaveValue(/http/);
  await page.evaluate(() => window.editor.update({ editAs: 'toml' }));
  await expect(input(page)).toHaveValue(/\[web\]/);
  await expect(input(page)).not.toHaveValue(/http/);
  await page.evaluate(() => window.editor.update({ editAs: 'yaml' }));
  await expect(input(page)).toHaveValue(/web:/);
  await expect(input(page)).not.toHaveValue(/http/);
  expect(errors).toEqual([]);
});

test('an XML child removed and added back is a child, its attribute kept', async ({ page }) => {
  const value = '<r a="1">\n  <a>2</a> <!-- c -->\n  <z>0</z>\n</r>\n';
  const errors = await open(page, { language: 'xml', value, editAs: 'json' });
  await expect(input(page)).toHaveValue(/"@a": 1/);
  await select(page, ',\n    "a": 2');
  await page.keyboard.press('Delete');
  await expect.poll(() => host(page)).toBe('<r a="1">\n  <z>0</z>\n</r>\n');
  await select(page, '"z": 0');
  await page.keyboard.insertText('"z": 0,\n    "a": 5');
  await expect.poll(() => host(page)).toBe('<r a="1">\n  <z>0</z>\n  <a>5</a>\n</r>\n');
  expect(errors).toEqual([]);
});

test('a YAML merge stays when a merged value is edited from another view', async ({ page }) => {
  const value = 'defaults: &D\n  a: 1 # da\n  b: 2\nsvc:\n  # merged\n  <<: *D\n  b: 20 # own\n';
  const errors = await open(page, { language: 'yaml', value, editAs: 'json' });
  await select(page, '"a": 1', 1);
  await page.keyboard.insertText('"a": 9');
  await expect
    .poll(() => host(page))
    .toBe('defaults: &D\n  a: 1 # da\n  b: 2\nsvc:\n  # merged\n  a: 9\n  <<: *D\n  b: 20 # own\n');
  expect(errors).toEqual([]);
});
