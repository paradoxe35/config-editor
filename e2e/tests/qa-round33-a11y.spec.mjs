import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// QA round 33, accessibility: an empty find query shows and announces no
// count (A33-1); in a right-to-left page the find widget sits at the start
// side, though the editor's text stays left to right (A33-2); the demo's
// unsaved-changes dot is a graphic (3:1 needed, 4.5:1 held, as it is also a glyph) against its background in every
// theme (A33-3).

const input = (page) => page.locator('.config-editor__input');
const find = (page) => page.locator('.config-editor__find');
const count = (page) => page.locator('.config-editor__find-count');
const said = (page) => page.locator('.config-editor__announcement').first();

test.describe('the find count (A33-1)', () => {
  setupEditor(test, '');

  test('is empty, and says nothing, for an empty query', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+f');
    await expect(find(page)).toBeVisible();
    await expect(count(page)).toHaveText('');
    await expect(said(page)).not.toHaveText(/No results/);
    await page.keyboard.type('zzzqqq');
    await expect(count(page)).toHaveText('No results');
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Backspace');
    await expect(count(page)).toHaveText('');
  });
});

test.describe('the find widget in a right-to-left page (A33-2)', () => {
  setupEditor(test, '');

  async function sides(page) {
    await input(page).click();
    await page.keyboard.press('Control+f');
    await expect(find(page)).toBeVisible();
    return page.evaluate(() => {
      const wrap = document.querySelector('.config-editor__input-wrap').getBoundingClientRect();
      const box = document.querySelector('.config-editor__find').getBoundingClientRect();
      return { start: box.left - wrap.left, end: wrap.right - box.right };
    });
  }

  test('sits at the left, and at the right in a left-to-right page', async ({ page }) => {
    const ltr = await sides(page);
    expect(ltr.end).toBeLessThan(ltr.start);
    await page.keyboard.press('Escape');
    await page.evaluate(() => (document.documentElement.dir = 'rtl'));
    const rtl = await sides(page);
    expect(rtl.start).toBeLessThan(rtl.end);
    expect(rtl.start).toBeLessThan(30);
    expect(await input(page).evaluate((element) => getComputedStyle(element).direction)).toBe('ltr');
  });
});

const ratio = (a, b) => {
  const lum = ([r, g, bl]) => {
    const f = (x) => ((x /= 255) <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(bl);
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

test.describe('the unsaved-changes dot (A33-3)', () => {
  setupEditor(test, '');

  test('is 4.5:1 against its background in every theme', async ({ page }) => {
    const themes = await page.locator('.demo__theme-select option').evaluateAll((options) => options.map((option) => option.value));
    expect(themes.length).toBe(11);
    const failures = [];
    for (const theme of themes) {
      await page.locator('.demo__theme-select').selectOption(theme);
      const [fg, bg] = await page.locator('.demo__host-status').evaluate((dot) => {
        // Colors resolve through a canvas: any CSS color becomes rgba.
        const rgb = (color) => {
          const canvas = document.createElement('canvas').getContext('2d');
          canvas.fillStyle = '#000';
          canvas.fillStyle = color;
          canvas.clearRect(0, 0, 1, 1);
          canvas.fillRect(0, 0, 1, 1);
          return [...canvas.getImageData(0, 0, 1, 1).data];
        };
        const layers = [];
        for (let element = dot; element; element = element.parentElement) {
          const color = rgb(getComputedStyle(element).backgroundColor);
          if (color[3] > 0) layers.push(color);
          if (color[3] === 255) break;
        }
        let under = [255, 255, 255];
        for (const [r, g, b, a] of layers.reverse()) under = [r, g, b].map((v, i) => (v * a + under[i] * (255 - a)) / 255);
        return [rgb(getComputedStyle(dot).color).slice(0, 3), under];
      });
      const measured = ratio(fg, bg);
      if (!(measured >= 4.5)) failures.push(`${theme}: ${measured.toFixed(2)} < 4.5`);
    }
    expect(failures).toEqual([]);
  });
});
