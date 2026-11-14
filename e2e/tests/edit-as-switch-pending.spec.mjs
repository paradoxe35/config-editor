import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// An item deleted in an Edit-as view of XML, then `update({ editAs })`
// switched inside the typing pause (at once, 50 ms or 250 ms after): the
// save that follows keeps each remaining item's own `xsi:type`, whichever
// view it was deleted in and switched to (QA E33-1: the edits that tell
// the items apart were lost at the switch, and the save paired the items
// by place, `Circle` landing on the second item).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const XSI = 'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"';
const shapes = (items) =>
  `<shapes ${XSI}>\n${items
    .map(([type, size]) => `  <shape xsi:type="${type}">\n    <size>${size}</size>\n  </shape>\n`)
    .join('')}</shapes>\n`;
const ITEMS = [
  ['Circle', 1],
  ['Square', 2],
  ['Triangle', 3],
];

// The view's lines of item `index`: its first line's text, and how many.
const ITEM_LINES = {
  yaml: (index) => [`- size: ${ITEMS[index][1]}`, 1],
  json: (index) => [`"size": ${ITEMS[index][1]}`, 3],
};

async function open(page, editAs) {
  const host = shapes(ITEMS);
  const options = { language: 'xml', value: host, baseline: host, fileName: 'shapes.xml', editAs };
  const url = `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
  await page.goto(url);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
}

/** Deletes item `index`'s lines with Ctrl+Shift+K, from its first line. */
async function deleteItem(page, view, index) {
  const [needle, lines] = ITEM_LINES[view](index);
  await page.locator('textarea').first().evaluate(
    (element, [find, json]) => {
      let at = element.value.indexOf(find);
      if (at < 0) throw new Error(`no ${find}`);
      // A JSON item starts on the `{` line above its key.
      at = element.value.lastIndexOf('\n', at - 1);
      if (json) at = element.value.lastIndexOf('\n', at - 1);
      element.focus();
      element.setSelectionRange(at + 1, at + 1);
    },
    [needle, view === 'json'],
  );
  for (let line = 0; line < lines; line += 1) await page.keyboard.press('Control+Shift+K');
}

test.describe('an item deleted, then a view switch inside the typing pause', () => {
  test.skip(!built, 'needs the npm package built (cd js && npm run build)');

  for (const view of ['yaml', 'json']) {
    for (const index of [0, 1]) {
      for (const wait of [0, 50, 250]) {
        for (const target of ['yaml', 'json', 'toml', null].filter((target) => target !== view)) {
          test(`${view}, item ${index} deleted, switched to ${target ?? 'xml'} after ${wait} ms`, async ({ page }) => {
            await open(page, view);
            await deleteItem(page, view, index);
            if (wait) await page.waitForTimeout(wait);
            await page.evaluate((editAs) => window.editor.update({ editAs }), target);
            const kept = ITEMS.filter((_, at) => at !== index);
            await expect.poll(() => page.locator('#saved').textContent()).toBe(shapes(kept));
            // Saved again after the switch, the items keep it.
            await page.keyboard.press('Control+s');
            await expect.poll(() => page.evaluate(() => window.saves.at(-1)?.text)).toBe(shapes(kept));
          });
        }
      }
    }
  }
});
