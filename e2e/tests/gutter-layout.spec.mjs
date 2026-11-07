import { test, expect } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The gutter's markers (a read-only lock, a secret's key, the problem dot)
// sit in their own column: a gap from the gutter's edge, and clear of the
// line number however many digits it has.

/** The left edge of a line number's digits, from its text node. */
function digitsLeft(page, line) {
  return page.evaluate((line) => {
    const row = document.querySelector(`.config-editor__gutter div[data-line="${line}"] span`);
    const range = document.createRange();
    range.selectNodeContents(row);
    return range.getBoundingClientRect().left;
  }, line);
}

const gutterLeft = (page) => page.evaluate(() => document.querySelector('.config-editor__gutter').getBoundingClientRect().left);

/** Each marker's box beside its line's digits: a gap on both sides. */
async function expectClear(page, selector, minimumGap = 3) {
  const markers = page.locator(selector);
  expect(await markers.count()).toBeGreaterThan(0);
  const edge = await gutterLeft(page);
  for (const marker of await markers.all()) {
    const box = await marker.boundingBox();
    const line = await marker.evaluate((element) => element.closest('[data-line]').dataset.line);
    expect(box.x - edge, `${selector} on line ${line} keeps a gap from the edge`).toBeGreaterThanOrEqual(minimumGap);
    expect(await digitsLeft(page, line), `${selector} on line ${line} is clear of its number`).toBeGreaterThanOrEqual(box.x + box.width + 1);
  }
}

test.describe('the gutter markers, read-only and secrets', () => {
  setupEditor(test, '?schema=protected');

  test('locks and keys keep a gap from the edge and from the line number', async ({ page }) => {
    await expectClear(page, '.config-editor__lock-marker');
    await expectClear(page, '.config-editor__secret-marker');
  });
});

test.describe('the gutter on a document with four-digit line numbers', () => {
  setupEditor(test, '?sample=large');

  test('the problem dot stays clear of a four-digit line number', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('nonsense_key: 1');
    await page.keyboard.press('Escape');
    await expect(page.locator('.config-editor__line-number-error')).toHaveCount(1, { timeout: 5000 });
    const line = await page.locator('.config-editor__line-number-error').getAttribute('data-line');
    expect(Number(line)).toBeGreaterThanOrEqual(1000);
    const dot = await page.locator('.config-editor__line-number-error').evaluate((row) => {
      const style = getComputedStyle(row, '::before');
      const box = row.getBoundingClientRect();
      return { left: box.left + parseFloat(style.left), width: parseFloat(style.width) };
    });
    expect(dot.left - (await gutterLeft(page))).toBeGreaterThanOrEqual(3);
    expect(await digitsLeft(page, line)).toBeGreaterThanOrEqual(dot.left + dot.width + 1);
  });
});

test.describe('the gutter markers at phone width', () => {
  test.use({ viewport: { width: 390, height: 700 } });
  setupEditor(test, '?schema=protected');

  test('locks and keys keep a gap on a phone too', async ({ page }) => {
    await expectClear(page, '.config-editor__lock-marker');
    await expectClear(page, '.config-editor__secret-marker');
  });
});
