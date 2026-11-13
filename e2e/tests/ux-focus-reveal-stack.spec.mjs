import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// Pointer, key and layout rules of the shell around the text, in the shipped demo:
// a press on the gutter keeps the focus in the text (a drag that starts on a line number or a diff marker
// ends over the text, and the keys after it are the editor's); the documents' switch keys page no document;
// a renamed tab is shown whole again; the change preview stays above the find widget.

const input = (page) => page.locator('.config-editor__input');
const focusedClass = (page) => page.evaluate(() => document.activeElement?.className ?? '');

// Types `text` at the start of the first line, leaving the text focused.
async function edit(page, text) {
  await input(page).click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type(text);
}

test.describe('a drag from the gutter', () => {
  setupEditor(test);

  for (const [name, selector] of [
    ['a line number', '.config-editor__gutter-content > div[data-line="3"]'],
    ['a diff marker', '.config-editor__diff-marker-modified'],
  ]) {
    test(`from ${name} leaves the focus in the text, so the keys after it type`, async ({ page }) => {
      await edit(page, 'X');
      await page.evaluate(() => document.activeElement.blur());
      expect(await focusedClass(page)).not.toContain('config-editor__input');
      const from = await page.locator(selector).first().boundingBox();
      const to = await input(page).boundingBox();
      await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
      await page.mouse.down();
      await page.mouse.move(to.x + 120, from.y + from.height / 2, { steps: 6 });
      await page.mouse.up();
      expect(await focusedClass(page)).toContain('config-editor__input');
      await page.keyboard.type('Q');
      await expect(input(page)).toHaveValue(/Q/);
    });
  }
});

