import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 30, accessibility: results that were silent are said through the
// live region (a revert, F8 with no problems, Alt+F5 / Alt+F3 with no
// changes, the notice's Reload), and `aria-keyshortcuts` names F8 only while
// the problems are on. Through the npm package's vanilla page (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input').first();
const said = (page) => page.locator('.config-editor__announcement').first();

async function open(page, options) {
  await page.goto(pageUrl(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  await input(page).click();
}

test('F8 with no problems says "No problems"; Shift+F8 too', async ({ page }) => {
  await open(page);
  await page.keyboard.press('F8');
  await expect(said(page)).toHaveText('No problems');
  await page.keyboard.press('Shift+F8');
  await expect(said(page)).toHaveText(/^No problems ?$/);
});

test('Alt+F5 and Alt+F3 with no changes say "No changes"', async ({ page }) => {
  await open(page);
  for (const key of ['Alt+F5', 'Alt+F3', 'Shift+Alt+F5']) {
    await page.keyboard.press(key);
    await expect(said(page), key).toHaveText(/^No changes ?$/);
  }
});

test('reverting a change says "Reverted"', async ({ page }) => {
  await open(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.type('# note\n');
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Alt+F3');
  await expect(page.locator('.config-editor__diff-peek')).toHaveCount(1);
  await page.keyboard.press('r');
  await expect(page.locator('.config-editor__diff-peek')).toHaveCount(0);
  await expect(said(page)).toHaveText(/^Reverted ?$/);
});

test('Reload in the changed-baseline notice says "Reloaded"', async ({ page }) => {
  await open(page);
  await page.keyboard.press('Control+End');
  await page.keyboard.type('# mine\n');
  await page.locator('#external-change').click();
  await expect(page.locator('.config-editor__notice')).toBeVisible();
  await page.locator('.config-editor__notice-reload').click();
  await expect(said(page)).toHaveText(/^Reloaded ?$/);
});

test('aria-keyshortcuts names F8 only while the problems are on', async ({ page }) => {
  await open(page);
  await expect(input(page)).toHaveAttribute('aria-keyshortcuts', /\bF8\b.*\bShift\+F8\b/);
  await page.evaluate(() => window.editor.update({ showDiagnostics: false }));
  await expect(input(page)).not.toHaveAttribute('aria-keyshortcuts', /F8/);
  await page.evaluate(() => window.editor.update({ showDiagnostics: true }));
  await expect(input(page)).toHaveAttribute('aria-keyshortcuts', /\bShift\+F8\b/);
});
