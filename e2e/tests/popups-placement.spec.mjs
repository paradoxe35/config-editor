import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Popups in a transformed container sit at what they point at.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const schema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port.' }, host: { type: 'string' } },
});

const value = 'port: 80\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

// The problem under the pointer is on a key the schema says nothing of:
// its tip shows (on a documented one, the card would list it).
const hostDiagnostics = [{ range: { startLine: 2, startColumn: 1, endColumn: 3 }, severity: 'error', message: 'k0 is taken' }];

// The host's container around the editor, as in xform.mjs.
const CONTAINERS = {
  'translate + margin': '#editor { transform: translate(0, 0); margin: 60px 0 0 80px; }',
  'scale 0.9': '#editor { transform: scale(0.9); transform-origin: 0 0; margin: 40px; }',
  'zoom 1.5': '#editor { zoom: 1.5; }',
  'filter + margin': '#editor { filter: drop-shadow(0 0 2px #000); margin-left: 100px; }',
};

const box = (locator) => locator.evaluate((element) => {
  const { left, top, right, bottom } = element.getBoundingClientRect();
  return { left, top, right, bottom };
});

/**
 * The boxes of `locators` together at the next frame, as the user sees them:
 * between frames the text may have scrolled before its scroll event (and so
 * its highlighted copy) has come.
 */
async function atFrame(page, ...locators) {
  const handles = await Promise.all(locators.map((locator) => locator.elementHandle()));
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

/** Fails unless `popup` is inside the window and right below or above `anchor`, from its left. */
function nextTo(name, popup, anchor, viewport) {
  const below = popup.top >= anchor.bottom - 2 && popup.top <= anchor.bottom + 24;
  const above = popup.bottom <= anchor.top + 2 && popup.bottom >= anchor.top - 24;
  const where = `${name}: ${JSON.stringify(popup)} for ${JSON.stringify(anchor)}`;
  expect(below || above, where).toBe(true);
  expect(Math.abs(popup.left - anchor.left), where).toBeLessThan(40);
  expect(popup.left >= 0 && popup.top >= 0, where).toBe(true);
  expect(popup.right <= viewport.width && popup.bottom <= viewport.height, where).toBe(true);
}

test.describe('popups in a transformed container', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const [name, css] of Object.entries(CONTAINERS)) {
    test(`the tip, the hover card and the suggestion list sit at what they point at (${name})`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      await page.setViewportSize({ width: 1100, height: 700 });
      await page.goto(vanilla({ value, schema, hostDiagnostics }));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.addStyleTag({ content: css });
      const viewport = page.viewportSize();
      const input = page.locator('.config-editor__input');

      // The tip of the problem under the pointer.
      const marker = page.locator('.config-editor__diagnostic-marker').first();
      await expect(marker).toBeVisible();
      const underline = await box(marker);
      await page.mouse.move(underline.left + 8, (underline.top + underline.bottom) / 2);
      const tip = page.locator('.config-editor__problem-tip:not([hidden])');
      await expect(tip).toBeVisible();
      nextTo('tip', ...(await atFrame(page, tip, marker)), viewport);

      // The hover card of the key under the pointer.
      await page.mouse.move(5, 5);
      const key = page.locator('.config-editor__highlight .config-editor__code-line').first().locator('.config-editor__key');
      const keyBox = await box(key);
      await page.mouse.move(keyBox.left + 6, (keyBox.top + keyBox.bottom) / 2);
      const card = page.locator('.config-editor__hover');
      await expect(card).toBeVisible();
      await expect(card).toContainText('The port.');
      nextTo('card', ...(await atFrame(page, card, key)), viewport);

      // The suggestion list at the caret on the last line.
      await page.mouse.move(5, 5);
      await input.click();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Space');
      const list = page.locator('.config-editor__completions:not(.hidden)');
      await expect(list).toBeVisible();
      const lastLine = page.locator('.config-editor__highlight .config-editor__code-line').last();
      nextTo('list', ...(await atFrame(page, list, lastLine)), viewport);
      expect(errors).toEqual([]);
    });
  }
});
