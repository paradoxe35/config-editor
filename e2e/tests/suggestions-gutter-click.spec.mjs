import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A click in the editor's own gutter (a line number, a fold control, the
// space between) closes the suggestion list, as a click in the text does
// and as in VS Code; what it is on still does its own: a line number
// selects its line, a fold control folds. On the vanilla page and the
// React one (run `npm run build` in js/ first).

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VALUE = 'listen: 7835\ntls:\n  cert: a.pem\n  key: b.pem\npublic_address: tunnel.example.com\n';
const PAGES = {
  vanilla: `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify({ value: VALUE }))}`,
  react: pathToFileURL(join(js, 'examples', 'react.html')).href,
};

const list = (page) => page.locator('.config-editor__completions');
const input = (page) => page.locator('.config-editor__input').first();

// Opens the suggestion list on a new last line.
async function suggest(page) {
  await input(page).focus();
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Control+Space');
  await expect(list(page)).toBeVisible();
}

test.skip(!built, 'run `npm run build` in js/ first');

for (const [name, url] of Object.entries(PAGES)) {
  test(`${name}: a click on a line number closes the suggestions and selects the line`, async ({ page }) => {
    await page.goto(url);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await suggest(page);
    await page.locator('.config-editor__gutter [data-line="2"]').first().click();
    await expect(list(page)).toBeHidden();
    const selected = await input(page).evaluate((element) => ({
      text: element.value.slice(element.selectionStart, element.selectionEnd),
      focused: document.activeElement === element,
    }));
    expect(selected.focused).toBe(true);
    expect(selected.text.length).toBeGreaterThan(0);
    expect(selected.text.split('\n')[0]).toBe((await input(page).inputValue()).split('\n')[1]);
  });

  test(`${name}: a click on the gutter below the last line closes the suggestions`, async ({ page }) => {
    await page.goto(url);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await suggest(page);
    const gutter = await page.locator('.config-editor__gutter').first().boundingBox();
    await page.mouse.click(gutter.x + gutter.width / 2, gutter.y + gutter.height - 6);
    await expect(list(page)).toBeHidden();
    expect(await input(page).evaluate((element) => document.activeElement === element)).toBe(true);
  });
}

test('vanilla: a click on a fold control closes the suggestions and folds', async ({ page }) => {
  await page.goto(PAGES.vanilla);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await suggest(page);
  const toggle = page.locator('.config-editor__fold-toggle[data-line="2"]');
  await toggle.click({ force: true });
  await expect(list(page)).toBeHidden();
  await expect(toggle).toHaveClass(/config-editor__fold-toggle-closed/);
});
