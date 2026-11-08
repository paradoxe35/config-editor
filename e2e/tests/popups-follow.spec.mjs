import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Popups stay with what they point at: when the text scrolls under a
// lock's tooltip, and when a host's scrolling container (not the page)
// moves the editor (frames that come late: popups-late-frames.spec.mjs). Through the npm package's vanilla page (run `npm run build` in
// js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const schema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port.' }, host: { type: 'string' } },
});
const value = 'port: "eighty"\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

/** The boxes of `locators` at the next frame, as the user sees them. */
async function atFrame(page, ...locators) {
  const handles = await Promise.all(locators.map((locator) => locator.elementHandle({ timeout: 5000 })));
  return page.evaluate(
    (elements) =>
      new Promise((resolve) =>
        requestAnimationFrame(() =>
          resolve(
            elements.map((element) => {
              const { left, top, right, bottom } = element.getBoundingClientRect();
              return { left, top, right, bottom };
            }),
          ),
        ),
      ),
    handles,
  );
}

/** Fails unless `popup` is right below or above `anchor` (at most a small gap away). */
function nextTo(name, popup, anchor) {
  const below = popup.top >= anchor.bottom - 2 && popup.top <= anchor.bottom + 6;
  const above = popup.bottom <= anchor.top + 2 && popup.bottom >= anchor.top - 6;
  expect(below || above, `${name}: ${JSON.stringify(popup)} for ${JSON.stringify(anchor)}`).toBe(true);
}

async function open(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  await page.setViewportSize({ width: 1100, height: 700 });
  await page.goto(vanilla({ value, schema }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  return errors;
}

const lastLine = (page) => page.locator('.config-editor__highlight .config-editor__code-line').last();

test.describe('popups follow what they point at', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test("a lock's tooltip moves with its text as the text scrolls, and goes with it out of view", async ({ page }) => {
    const errors = await open(page);
    await page.locator('#lock-first-line').check();
    const input = page.locator('.config-editor__input');
    await input.click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    await page.keyboard.type('x');
    const tip = page.locator('.config-editor__protect-tip');
    await expect(tip).toBeVisible();
    const line = page.locator('.config-editor__highlight .config-editor__code-line[data-line="1"]');
    const [before, lineBefore] = await atFrame(page, tip, line);
    // At the caret's line, though the caret was at its end.
    nextTo('tip', before, lineBefore);
    await input.evaluate((element) => {
      element.scrollTop = 10;
    });
    const [after, lineAfter] = await atFrame(page, tip, line);
    expect(lineAfter.top).toBeLessThan(lineBefore.top - 5);
    expect(after.top - lineAfter.top, JSON.stringify({ before, lineBefore, after, lineAfter })).toBeCloseTo(before.top - lineBefore.top, 0);
    await input.evaluate((element) => {
      element.scrollTop = 300;
    });
    await expect(tip).toHaveCount(0);
    await input.evaluate((element) => {
      element.scrollTop = 0;
    });
    await expect(tip).toBeVisible();
    expect(errors).toEqual([]);
  });

  test("a host's scrolling container moves the suggestion list and the problem tip, and closes the hover card", async ({ page }) => {
    const errors = await open(page);
    // The page itself does not scroll: its body does, as a dialog's or a
    // panel's container would.
    await page.addStyleTag({ content: 'html { overflow: hidden; } body { height: 100vh; overflow: auto; } #saved { min-height: 900px; }' });
    const body = page.locator('body');
    const input = page.locator('.config-editor__input');

    // The problem tip.
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toBeVisible();
    const underline = await marker.boundingBox();
    await page.mouse.move(underline.x + 8, underline.y + underline.height / 2);
    const tip = page.locator('.config-editor__problem-tip:not([hidden])');
    await expect(tip).toBeVisible();
    await body.evaluate((element) => {
      element.scrollTop = 30;
    });
    const [tipBox, markerBox] = await atFrame(page, tip, marker);
    nextTo('tip', tipBox, markerBox);
    await body.evaluate((element) => {
      element.scrollTop = 0;
    });

    // The hover card goes, as it does when the text scrolls.
    await page.mouse.move(5, 5);
    const key = page.locator('.config-editor__highlight .config-editor__code-line').first().locator('.config-editor__key');
    const keyBox = await key.boundingBox();
    await page.mouse.move(keyBox.x + 6, keyBox.y + keyBox.height / 2);
    const card = page.locator('.config-editor__hover');
    await expect(card).toBeVisible();
    await body.evaluate((element) => {
      element.scrollTop = 30;
    });
    await expect(card).toHaveCount(0);
    await body.evaluate((element) => {
      element.scrollTop = 0;
    });

    // The suggestion list.
    await page.mouse.move(5, 5);
    await input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    const list = page.locator('.config-editor__completions:not(.hidden)');
    await expect(list).toBeVisible();
    const [listBefore, lineBefore] = await atFrame(page, list, lastLine(page));
    nextTo('list', listBefore, lineBefore);
    await body.evaluate((element) => {
      element.scrollTop = 200;
    });
    const [listAfter, lineAfter] = await atFrame(page, list, lastLine(page));
    expect(lineAfter.top).toBeLessThan(lineBefore.top - 150);
    nextTo('list after the container scrolled', listAfter, lineAfter);
    expect(errors).toEqual([]);
  });
});
