import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A hover card whose content arrives after it showed (problems checked
// after it opened, a host's diagnostics) grows in place: it keeps the side
// of its line it showed on, scrolling in its room there, so a pointer on
// its way into it still gets there (the card flipped to the other side of
// its line and closed under a pointer heading for where it was). Through
// the npm package's vanilla page (run `npm run build` in js/ first).

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const card = (page) => page.locator('.config-editor__hover');

const SCHEMA = JSON.stringify({ type: 'object', properties: { port: { type: 'integer', description: 'Port docs' } } });
const VALUE = `name: x\nport: 3\n${Array.from({ length: 30 }, (_, i) => `key_${i}: ${i}\n`).join('')}`;
const LINE = { startLine: 2, startColumn: 1, endLine: 2, endColumn: 8 };
const LONG =
  'This problem says a great deal about the port, more than the room above its line can hold at once, so the card holding it grows';

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 900, height: 500 } });

test('a card that grows past its room after it showed keeps its side and the pointer can enter it', async ({ page }) => {
  await page.goto(pageUrl({ value: VALUE, language: 'yaml', schema: SCHEMA }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  const port = await page.evaluate(() => {
    const row = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')].find((line) =>
      line.textContent.startsWith('port'),
    );
    const box = row.getBoundingClientRect();
    return { x: box.left + 12, y: box.top + box.height / 2, top: box.top, bottom: box.bottom };
  });
  // A short problem on the line first: the card shows the problems there.
  await page.evaluate((range) => window.editor.update({ hostDiagnostics: [{ range, message: 'Short' }] }), LINE);
  await page.mouse.move(port.x, port.y);
  await expect(card(page)).toContainText('Short');
  await page.waitForTimeout(200);
  const before = await card(page).boundingBox();
  // Shown above its line, where it fits small.
  expect(before.y + before.height).toBeLessThanOrEqual(port.top);
  // Its problems arrive: far taller than the room above.
  await page.evaluate(
    ([range, message]) => window.editor.update({ hostDiagnostics: Array.from({ length: 6 }, () => ({ range, message })) }),
    [LINE, LONG],
  );
  await expect(card(page)).toContainText('a great deal');
  await page.waitForTimeout(200);
  const after = await card(page).boundingBox();
  expect(after.y + after.height).toBeLessThanOrEqual(port.top + 1);
  // Straight up into where the card showed: it stays.
  await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2, { steps: 6 });
  await page.waitForTimeout(500);
  await expect(card(page)).toBeVisible();
});
