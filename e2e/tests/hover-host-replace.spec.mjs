import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A docs card about the text goes when the host replaces the text, and one
// that had the focus gives it back to the text (round 14's `D14.ux-2`: a
// focused card kept a removed key's docs).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');
const card = (page) => page.locator('#qa .config-editor__hover');

test.describe('a docs card when the host replaces the text', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const focused of [false, true]) {
    test(`goes${focused ? ', the focus back in the text' : ''}`, async ({ page }) => {
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(async () => {
        const module = await import('../config-editor/index.js');
        window.editor?.destroy();
        const host = document.createElement('div');
        host.id = 'qa';
        document.body.prepend(host);
        window.qa = new module.ConfigEditor(host, {
          language: 'yaml',
          value: 'public_address: x\n',
          schema: JSON.stringify({ type: 'object', properties: { public_address: { type: 'string', description: 'Where clients connect.' } } }),
        });
      });
      await input(page).evaluate((element) => {
        element.focus();
        element.setSelectionRange(3, 3);
      });
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+i');
      await expect(card(page)).toContainText('Where clients connect');
      if (focused) {
        await page.keyboard.press('Control+k');
        await page.keyboard.press('Control+i');
        await expect.poll(() => card(page).evaluate((element) => element.contains(document.activeElement))).toBe(true);
      }
      await page.evaluate(() => window.qa.update({ value: 'port: 1\n' }));
      await expect(card(page)).toBeHidden();
      if (focused) await expect(input(page)).toBeFocused();
    });
  }
});
