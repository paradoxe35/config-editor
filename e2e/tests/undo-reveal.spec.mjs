import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Undo and redo bring the change back into view as a jump does: centered
// when it is out of view or near an edge (never the top row under the
// sticky lines), and the view left alone when the change is well in view.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

// Nested blocks, so sticky lines show at the top once scrolled.
const VALUE = [
  'server:',
  '  http:',
  ...Array.from({ length: 200 }, (_, index) => `    key${index}: value${index}`),
  '',
].join('\n');
// Zero-based: `key10` is on line 12.
const EDITED_LINE = 12;

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();

async function openEditor(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla({ value: VALUE }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await field(page).waitFor();
  await field(page).click();
  return errors;
}

/** Puts the caret at the end of zero-based `line`, scrolled so `top` is the first row. */
const placeCaret = (page, line, top) =>
  field(page).evaluate(
    (element, [line, top]) => {
      const lines = element.value.split('\n');
      const at = lines.slice(0, line + 1).join('\n').length;
      element.setSelectionRange(at, at);
      const style = getComputedStyle(element);
      element.scrollTop = parseFloat(style.paddingTop) + top * parseFloat(style.lineHeight);
      element.dispatchEvent(new Event('scroll'));
    },
    [line, top],
  );

/** Scrolls so zero-based `top` is the first row, the caret left where it is. */
const scrollTo = (page, top) =>
  field(page).evaluate((element, top) => {
    const style = getComputedStyle(element);
    element.scrollTop = parseFloat(style.paddingTop) + top * parseFloat(style.lineHeight);
    element.dispatchEvent(new Event('scroll'));
  }, top);

/** The caret's zero-based line, its middle as a share of the view's height, and whether the sticky lines cover it. */
const caretPlace = (page) =>
  field(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    const line = element.value.slice(0, element.selectionStart).split('\n').length - 1;
    const top = parseFloat(style.paddingTop) + line * lineHeight - element.scrollTop;
    const layer = element.closest('.config-editor').querySelector('.config-editor__sticky');
    const covered = layer ? layer.getBoundingClientRect().bottom - element.getBoundingClientRect().top : 0;
    return { line, share: (top + lineHeight / 2) / element.clientHeight, underSticky: top < covered - 0.5 };
  });
const scrollTop = (page) => field(page).evaluate((element) => element.scrollTop);

/** The caret's row is in the middle band of the view, clear of the sticky lines. */
async function expectCentered(page, line) {
  await expect.poll(() => caretPlace(page).then((place) => place.line)).toBe(line);
  // Settled: the frame after the edit may still move the view.
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const { share, underSticky } = await caretPlace(page);
  expect(share, `the line sits at ${Math.round(share * 100)}% of the view`).toBeGreaterThan(0.35);
  expect(share, `the line sits at ${Math.round(share * 100)}% of the view`).toBeLessThan(0.65);
  expect(underSticky).toBe(false);
}

test.describe('undo and redo reveal the change, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const redo of ['Control+y', 'Control+Shift+z']) {
    test(`undo, then redo with ${redo}, from far below centers the edited line`, async ({ page }) => {
      const errors = await openEditor(page);
      await placeCaret(page, EDITED_LINE, 0);
      await page.keyboard.type('X');
      await scrollTo(page, 150);
      await page.keyboard.press('Control+z');
      await expectCentered(page, EDITED_LINE);
      expect(await field(page).inputValue()).toBe(VALUE);
      await scrollTo(page, 150);
      await page.keyboard.press(redo);
      await expectCentered(page, EDITED_LINE);
      expect(await field(page).inputValue()).toContain('key10: value10X');
      expect(errors).toEqual([]);
    });
  }

  test('undo from far above centers the edited line', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, 150, 140);
    await page.keyboard.type('X');
    await scrollTo(page, 0);
    await page.keyboard.press('Control+z');
    await expectCentered(page, 150);
    expect(errors).toEqual([]);
  });

  test('undo of an edit just below the sticky lines centers it: they count as the top edge', async ({ page }) => {
    const errors = await openEditor(page);
    // Two sticky lines (`server:`, `http:`) cover rows 0 and 1; the edit is on row 4.
    await placeCaret(page, 100, 96);
    await page.keyboard.type('X');
    await page.keyboard.press('Control+z');
    await expectCentered(page, 100);
    expect(errors).toEqual([]);
  });

  test('undo of an edit well in view does not scroll', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, 100, 90);
    await page.keyboard.type('X');
    const before = await scrollTop(page);
    await page.keyboard.press('Control+z');
    await expect(field(page)).toHaveValue(VALUE);
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    expect(await scrollTop(page)).toBe(before);
    expect(errors).toEqual([]);
  });
});
