import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Popups sit at what they point at in the first frame the browser draws,
// however late it comes. Through the npm package's vanilla page (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const schema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port.' }, host: { type: 'string' } },
});
const value = 'port: "eighty"\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

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

// The page draws only when asked (see `framesOnDemand`).
test.use({ launchOptions: { args: ['--no-sandbox', '--disable-gpu', '--allow-file-access-from-files', '--deterministic-mode'] } });

/**
 * Frames on demand: with `--deterministic-mode` the page draws (and runs
 * scroll events and animation frames) only when asked, as a loaded machine
 * that skips frames while the page still takes keys and scripts. `run`
 * draws a frame every 16 ms; `hold` stops, and `frame` draws one and
 * resolves with the boxes of `locators` at it.
 */
async function framesOnDemand(page) {
  const cdp = await page.context().newCDPSession(page);
  const begin = () => cdp.send('HeadlessExperimental.beginFrame', {}).catch(() => {});
  let timer = null;
  const pump = {
    run() {
      if (!timer) timer = setInterval(begin, 16);
    },
    async hold() {
      clearInterval(timer);
      timer = null;
      // A frame already asked for draws first.
      await begin();
    },
    async frame(...locators) {
      const handles = await Promise.all(locators.map((locator) => locator.elementHandle({ timeout: 5000 })));
      const boxes = page.evaluate(
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
      // The callback is registered before the frame is asked for.
      await page.evaluate(() => 0);
      await begin();
      return boxes;
    },
  };
  pump.run();
  // Frames keep coming until the page closes (it waits for one to close).
  page.on('close', () => clearInterval(timer));
  return pump;
}

// The first frame drawn after the text scrolls shows each popup at its text,
// though the text scrolled (and the popup opened) frames before the browser
// got to draw: the scroll event and so the highlighted copy come only with
// that frame. In the full e2e under load, U20-N1 measured between the two
// and saw the list 155 px above its line.
test.describe('popups when frames come late', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test('the suggestion list, the problem tip, the change preview and the lock tooltip sit at their text in the first frame drawn', async ({ page }) => {
    const pump = await framesOnDemand(page);
    const errors = await open(page);
    await page.addStyleTag({ content: '#editor { filter: drop-shadow(0 0 2px #000); margin-left: 100px; }' });
    const input = page.locator('.config-editor__input');
    const scrollText = (top) =>
      input.evaluate((element, top) => {
        element.scrollTop = top;
      }, top);

    // The list, opened at the end with no frame since Ctrl+End scrolled.
    await input.click();
    await pump.hold();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    const list = page.locator('.config-editor__completions:not(.hidden)');
    await expect(list).toBeAttached();
    nextTo('list', ...(await pump.frame(list, lastLine(page))));
    pump.run();
    await page.keyboard.press('Escape');
    await expect(list).toHaveCount(0);

    // The problem tip, as the text scrolls under it.
    await scrollText(0);
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toBeVisible();
    // Measured at a frame: the text scrolled back since the last one.
    const [underline] = await pump.frame(marker);
    await page.mouse.move(underline.left + 8, (underline.top + underline.bottom) / 2);
    const tip = page.locator('.config-editor__problem-tip:not([hidden])');
    await expect(tip).toBeVisible();
    await pump.hold();
    await scrollText(12);
    nextTo('tip', ...(await pump.frame(tip, marker)));
    pump.run();
    await page.mouse.move(5, 5);
    await expect(tip).toHaveCount(0);

    // The change preview of an edited line.
    await scrollText(0);
    const third = page.locator('.config-editor__highlight .config-editor__code-line[data-line="3"]');
    await input.focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.type('0');
    await page.keyboard.press('Alt+F3');
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toBeVisible();
    const [peekBefore, lineBefore] = await pump.frame(preview, third);
    await pump.hold();
    await scrollText(12);
    const [peekAfter, lineAfter] = await pump.frame(preview, third);
    expect(lineAfter.top).toBeLessThan(lineBefore.top - 5);
    expect(peekAfter.top - lineAfter.top, JSON.stringify({ peekBefore, lineBefore, peekAfter, lineAfter })).toBeCloseTo(peekBefore.top - lineBefore.top, 0);
    pump.run();
    await page.keyboard.press('Escape');
    await expect(preview).toHaveCount(0);

    // The lock tooltip.
    await scrollText(0);
    await page.locator('#lock-first-line').check();
    await input.focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    await page.keyboard.type('x');
    const lockTip = page.locator('.config-editor__protect-tip');
    await expect(lockTip).toBeVisible();
    const first = page.locator('.config-editor__highlight .config-editor__code-line[data-line="1"]');
    const [lockBefore, firstBefore] = await pump.frame(lockTip, first);
    await pump.hold();
    await scrollText(12);
    const [lockAfter, firstAfter] = await pump.frame(lockTip, first);
    expect(firstAfter.top).toBeLessThan(firstBefore.top - 5);
    expect(lockAfter.top - firstAfter.top, JSON.stringify({ lockBefore, firstBefore, lockAfter, firstAfter })).toBeCloseTo(lockBefore.top - firstBefore.top, 0);
    pump.run();
    expect(errors).toEqual([]);
  });
});
