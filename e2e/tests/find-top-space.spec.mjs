import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// While the find widget is open, the text has room above its first line as
// tall as the widget (as VS Code's `editor.find.addExtraSpaceOnTop`): a
// match on the first line, at the right where the widget is, scrolls clear
// of it. The text does not move as the widget opens or closes; the caret,
// the gutter and the marks follow the room; and a match that fits in the
// view leaves the text unscrolled sideways.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();
const widget = (page) => page.locator('.config-editor__find');
const current = (page) => page.locator('.config-editor__search-match-current');

const filler = Array.from({ length: 80 }, (_, index) => `key${index}: ${index}`);

async function open(page, firstLine) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl({ schema: undefined, value: [firstLine, ...filler, ''].join('\n') }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await field(page).waitFor();
  return errors;
}

/** How many columns the text shows without scrolling sideways. */
async function columns(page) {
  await open(page, 'x'.repeat(400));
  return field(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const right = parseFloat(style.paddingRight);
    const column = (element.scrollWidth - right - parseFloat(style.paddingLeft)) / 400;
    return Math.floor((element.clientWidth - right) / column);
  });
}

/** Where zero-based `line` is drawn by the textarea, from its top padding and scroll. */
const lineTop = (page, line) =>
  field(page).evaluate((element, line) => {
    const style = getComputedStyle(element);
    return (
      element.getBoundingClientRect().top +
      parseFloat(style.paddingTop) +
      line * parseFloat(style.lineHeight) -
      element.scrollTop
    );
  }, line);

/** The top of the gutter's row numbered `number`. */
const gutterTop = (page, number) =>
  page.evaluate((number) => {
    const row = document.querySelector(`.config-editor__gutter-content > div[data-line="${number}"]`);
    return row ? row.getBoundingClientRect().top : null;
  }, number);

const overlaps = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

test.skip(!built, 'run `npm run build` in js/ first');

for (const [width, height] of [
  [390, 844],
  [1280, 800],
]) {
  test.describe(`${width}x${height}`, () => {
    test.use({ viewport: { width, height } });

    test('a match on the first line at the right shows clear of the find widget, unscrolled sideways', async ({ page }) => {
      const fit = await columns(page);
      // `secret` ends seven columns before the view's right edge.
      const firstLine = `# ${'x'.repeat(fit - 16)} secret`;
      const errors = await open(page, firstLine);
      await field(page).click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Control+f');
      await expect(widget(page)).toBeVisible();
      await page.keyboard.type('secret');
      await expect(current(page)).toHaveCount(1);
      const match = await current(page).boundingBox();
      const box = await widget(page).boundingBox();
      expect(overlaps(match, box), JSON.stringify({ match, box })).toBe(false);
      expect(await field(page).evaluate((element) => element.scrollLeft)).toBe(0);
      // The match's mark is drawn where the textarea draws the first line, and the gutter's row with it.
      expect(Math.abs(match.y - (await lineTop(page, 0)))).toBeLessThanOrEqual(1);
      expect(Math.abs((await gutterTop(page, 1)) - (await lineTop(page, 0)))).toBeLessThanOrEqual(1);

      // A click in the middle of the match puts the caret inside it.
      await page.mouse.click(match.x + match.width / 2, match.y + match.height / 2);
      const start = firstLine.indexOf('secret');
      const caret = await field(page).evaluate((element) => element.selectionStart);
      expect(caret).toBeGreaterThan(start);
      expect(caret).toBeLessThan(start + 'secret'.length);
      expect(errors).toEqual([]);
    });

    test('opening and closing find does not move the text', async ({ page }) => {
      const errors = await open(page, 'first: 1');
      await field(page).click();
      await page.keyboard.press('Control+Home');
      const before = { top: await lineTop(page, 0), gutter: await gutterTop(page, 1) };
      // At the top: the room comes and goes without moving the text.
      await page.keyboard.press('Control+f');
      await expect(widget(page)).toBeVisible();
      expect(Math.abs((await lineTop(page, 0)) - before.top)).toBeLessThanOrEqual(1);
      await page.keyboard.press('Escape');
      await expect(widget(page)).toBeHidden();
      expect(Math.abs((await lineTop(page, 0)) - before.top)).toBeLessThanOrEqual(1);
      expect(Math.abs((await gutterTop(page, 1)) - before.gutter)).toBeLessThanOrEqual(1);

      // Scrolled down: the line at the top stays there through open and close.
      await field(page).evaluate((element) => {
        element.scrollTop = 20 * parseFloat(getComputedStyle(element).lineHeight);
        element.dispatchEvent(new Event('scroll'));
      });
      const scrolled = { top: await lineTop(page, 30), gutter: await gutterTop(page, 31) };
      await page.keyboard.press('Control+f');
      await expect(widget(page)).toBeVisible();
      expect(Math.abs((await lineTop(page, 30)) - scrolled.top)).toBeLessThanOrEqual(1);
      expect(Math.abs((await gutterTop(page, 31)) - scrolled.gutter)).toBeLessThanOrEqual(1);
      await page.keyboard.press('Escape');
      await expect(widget(page)).toBeHidden();
      expect(Math.abs((await lineTop(page, 30)) - scrolled.top)).toBeLessThanOrEqual(1);
      expect(Math.abs((await gutterTop(page, 31)) - scrolled.gutter)).toBeLessThanOrEqual(1);
      expect(errors).toEqual([]);
    });
  });
}
