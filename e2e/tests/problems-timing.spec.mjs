import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Thousands of host problems stay cheap (QA 19): F8 with 3000 of them in a
// 6000-line document (each press froze 350-1050 ms), and moving through a
// document with 50 ranges of 200 lines (10,000 underlines froze 1.6-2.9 s
// per Ctrl+End). Timed: runs alone after the other specs (see the config).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (page) => page.locator('.config-editor__input');
const footer = (page) => page.locator('.config-editor__diagnostic-status');

async function open(page, options) {
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  await page.evaluate(() => {
    window.longest = 0;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.longest = Math.max(window.longest, entry.duration);
    }).observe({ type: 'longtask' });
  });
}

/** The longest task while `work` runs and the page settles, in ms. */
async function longestTask(page, work) {
  await page.waitForTimeout(300);
  await page.evaluate(() => (window.longest = 0));
  await work();
  await page.waitForTimeout(400);
  return page.evaluate(() => window.longest);
}

const lines = (count) => Array.from({ length: count }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

test.describe('problem timing', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('F8 through 3000 host problems in 6000 lines stays responsive', async ({ page }) => {
    await open(page, { value: lines(6000), schema: '{}' });
    const problems = Array.from({ length: 3000 }, (_, index) => ({
      range: { startLine: 2 + ((index * 2) % 5990), startColumn: 1, endLine: 4 + ((index * 2) % 5990), endColumn: 1 },
      severity: ['error', 'warning', 'info', 'hint'][index % 4],
      message: `host ${index}`,
    }));
    await page.evaluate((list) => window.editor.update({ hostDiagnostics: list }), problems);
    await expect(footer(page)).toHaveText(/^750 errors/);
    await input(page).click();
    await page.keyboard.press('F8');
    const presses = [];
    for (let press = 0; press < 5; press += 1) presses.push(await longestTask(page, () => page.keyboard.press('F8')));
    // Was 350-1050 ms a press.
    expect(Math.min(...presses), `longest task per press: ${presses.join(', ')}`).toBeLessThan(150);
  });

  test('moving through 50 ranges of 200 lines stays responsive', async ({ page }) => {
    await open(page, { value: lines(3000), schema: '{}' });
    const problems = Array.from({ length: 50 }, (_, index) => ({
      range: { startLine: 1 + index * 7, startColumn: 1, endLine: 201 + index * 7, endColumn: 2 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await page.evaluate((list) => window.editor.update({ hostDiagnostics: list }), problems);
    await expect(footer(page)).toHaveText(/^50 warnings/);
    expect(await page.locator('.config-editor__diagnostic-marker').count()).toBeLessThanOrEqual(200);
    await input(page).click();
    const moves = [];
    for (let move = 0; move < 3; move += 1) {
      moves.push(await longestTask(page, () => page.keyboard.press('Control+End')));
      moves.push(await longestTask(page, () => page.keyboard.press('Control+Home')));
    }
    // Was 1.6-2.9 s.
    expect(Math.min(...moves), `longest task per move: ${moves.join(', ')}`).toBeLessThan(200);
  });
});
