import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 20 (UX): the problem tip, the hover card and the suggestion list
// sit next to what they point at and inside the window when the host puts
// the editor in a transformed, scaled, zoomed or filtered container (an
// animated dialog): `position: fixed` counts from such a container, and
// they landed 80–150 px off, the suggestion list off-screen.
// Through the npm package's vanilla page (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const schema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port.' }, host: { type: 'string' } },
});
const value = 'port: "eighty"\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

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
    test(`U20-N1: the tip, the hover card and the suggestion list sit at what they point at (${name})`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      await page.setViewportSize({ width: 1100, height: 700 });
      await page.goto(vanilla({ value, schema }));
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
      nextTo('tip', await box(tip), underline, viewport);

      // The hover card of the key under the pointer.
      await page.mouse.move(5, 5);
      const key = page.locator('.config-editor__highlight .config-editor__code-line').first().locator('.config-editor__key');
      const keyBox = await box(key);
      await page.mouse.move(keyBox.left + 6, (keyBox.top + keyBox.bottom) / 2);
      const card = page.locator('.config-editor__hover');
      await expect(card).toBeVisible();
      await expect(card).toContainText('The port.');
      nextTo('card', await box(card), keyBox, viewport);

      // The suggestion list at the caret on the last line.
      await page.mouse.move(5, 5);
      await input.click();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Space');
      const list = page.locator('.config-editor__completions:not(.hidden)');
      await expect(list).toBeVisible();
      const lastLine = await box(page.locator('.config-editor__highlight .config-editor__code-line').last());
      nextTo('list', await box(list), lastLine, viewport);
      expect(errors).toEqual([]);
    });
  }
});

// QA round 20 (U20-3): text of several lines arriving whole in one command
// (an IME commit, `Input.insertText`) is applied by the editor as a paste,
// not line by line by the textarea: over the selection, line breaks as LF,
// the caret after it, one undo step; read-only refuses it.
test.describe('lines arriving whole in one command', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('U20-3: replace the selection as a paste does, in one undo step, and stay out of read-only', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(vanilla({ value: 'a: 1\nb: 2\n', schema: '{}' }));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const input = page.locator('.config-editor__input');
    await input.click();
    // `1` of line 1 selected.
    await input.evaluate((element) => element.setSelectionRange(3, 4));
    await page.keyboard.insertText('9\r\nc: 3\nd: 4');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4\nb: 2\n');
    await page.keyboard.type('x');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4x\nb: 2\n');
    await page.keyboard.press('Control+z');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4\nb: 2\n');
    await page.keyboard.press('Control+z');
    await expect(input).toHaveValue('a: 1\nb: 2\n');
    await page.locator('#read-only').check();
    await input.focus();
    await page.keyboard.insertText('e: 5\nf: 6\n');
    await expect(input).toHaveValue('a: 1\nb: 2\n');
    expect(errors).toEqual([]);
  });
});
