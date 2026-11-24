import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Why the editor did not do what the user asked shows with the
// `showDiagnostics` setting off too (it hides the text's own problems):
// a switch to TOML waiting on a JSON `null` is marked on the value, counted
// in the footer and reached with F8.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const markers = (page) => page.locator('.config-editor__diagnostic-marker');
const footer = (page) => page.locator('.config-editor__diagnostic-status');
const input = (page) => page.locator('.config-editor__input');
const tip = (page) => page.locator('.config-editor__problem-tip');

async function open(page, options) {
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
}

test.describe('problems the editor must show', () => {
  test.skip(!built, 'needs the npm package built (cd js && npm run build)');

  test('a switch to TOML waiting on a null says why with diagnostics off', async ({ page }) => {
    const value = '{\n  "port": null\n}\n';
    await open(page, { language: 'json', value, baseline: value, fileName: 'a.json', schema: null, showDiagnostics: false });
    // The text's own problems stay hidden: no footer count, no marker.
    await expect(footer(page)).toHaveCount(0);
    await page.evaluate(() => window.editor.update({ editAs: 'toml' }));
    await expect(markers(page)).toHaveCount(1);
    await expect(footer(page)).toContainText('1 error');
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(tip(page)).toContainText('Cannot switch to TOML yet');
    expect(await input(page).inputValue()).toBe(value);
  });
});
