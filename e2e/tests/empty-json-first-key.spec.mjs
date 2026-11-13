import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The first key completed in an empty JSON document opens the root object
// around it, so the document parses at once: asked for on nothing
// (Ctrl+Space) or typed first, in a JSON editor and in an Edit-as JSON view.

const PAGES = [
  ['a JSON editor', '?language=json', 'listen', 'lis'],
  ['an Edit-as JSON view', '?schema=help&edit=json', 'level', 'lev'],
];

for (const [name, query, key, typed] of PAGES) {
  test.describe(`${name}: the first key of an empty document`, () => {
    setupEditor(test, query);

    for (const [how, start] of [
      ['asked for on nothing', async (page) => page.keyboard.press('Control+Space')],
      ['typed', async (page) => page.keyboard.type(typed)],
      ['typed after a quote', async (page) => page.keyboard.type(`"${typed}`)],
    ]) {
      test(`${how} opens the root object`, async ({ page }) => {
        const e = editor(page);
        await e.typeDoc('\n');
        await start(page);
        await expect.poll(() => e.isOpen()).toBe(true);
        await e.clickKey(key);
        const text = await e.value();
        expect(text.startsWith(`{\n  "${key}": `), JSON.stringify(text)).toBe(true);
        expect(text.trimEnd().endsWith('}'), JSON.stringify(text)).toBe(true);
        // The caret is inside the object, on the key's line.
        expect((await e.caret()).line).toBe(2);
        await page.keyboard.type('1');
        await page.waitForTimeout(700);
        expect(Object.keys(JSON.parse(await e.value()))).toEqual([key]);
      });
    }
  });
}
