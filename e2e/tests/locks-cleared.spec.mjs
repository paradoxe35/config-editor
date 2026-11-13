import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Locks the host clears hold back nothing any more: a key refused at a lock
// goes through at the same place once the lock is cleared (hard round 3's
// `D26.host-1`), and an Edit-as edit the typing pause could not convert
// converts then (hard round 4's `D27.host-1`).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

async function mount(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.changes = [];
    window.qa = new module.ConfigEditor(host, {
      language: 'yaml',
      value: 'a: 1\nb: 2\nc: 3\n',
      ...options,
      onChange: (text) => window.changes.push(text),
    });
  }, options);
  await input(page).waitFor();
}

async function caretAfter(page, marker) {
  await input(page).evaluate((element, marker) => {
    const at = element.value.indexOf(marker) + marker.length;
    element.focus();
    element.setSelectionRange(at, at);
  }, marker);
}

const LOCKS = {
  readOnlyPaths: { set: { readOnlyPaths: ['/a'] }, clear: { readOnlyPaths: null } },
  readOnlyRanges: {
    set: { readOnlyRanges: [{ startLine: 1, startColumn: 4, endLine: 1, endColumn: 5 }] },
    clear: { readOnlyRanges: null },
  },
};

test.describe('locks the host clears', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, { set, clear }] of Object.entries(LOCKS)) {
    test(`${name}: a key refused at the lock goes through once it is cleared`, async ({ page }) => {
      await mount(page, {});
      await page.evaluate((set) => window.qa.update(set), set);
      await caretAfter(page, 'a: 1');
      await page.keyboard.type('9');
      expect(await page.evaluate(() => window.qa.value)).toBe('a: 1\nb: 2\nc: 3\n');
      await page.evaluate((clear) => window.qa.update(clear), clear);
      await page.waitForTimeout(300);
      await page.keyboard.type('9');
      await expect.poll(() => page.evaluate(() => window.qa.value)).toBe('a: 19\nb: 2\nc: 3\n');
    });

    test(`${name}: an Edit-as edit refused in the pause converts once it is cleared`, async ({ page }) => {
      await mount(page, { editAs: 'json' });
      await caretAfter(page, '"a": 1');
      await page.keyboard.type('9');
      await page.evaluate((set) => window.qa.update(set), set);
      await page.waitForTimeout(600);
      expect(await page.evaluate(() => window.changes)).toEqual([]);
      await page.evaluate((clear) => window.qa.update(clear), clear);
      await expect.poll(() => page.evaluate(() => window.changes.at(-1))).toBe('a: 19\nb: 2\nc: 3\n');
    });
  }
});
