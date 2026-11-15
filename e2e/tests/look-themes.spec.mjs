import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setupEditor } from './helpers.mjs';

// Themes: page rules beat bundled themes, badges and dots keep 4.5:1 contrast.

const ratio = (a, b) => {
  const lum = ([r, g, bl]) => {
    const f = (x) =>
      (x /= 255) <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(bl);
  };
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

test.describe('a page stylesheet overrides theme colors', () => {
  setupEditor(test, '?language=yaml');

  test('a plain .config-editor rule beats every bundled theme', async ({ page }) => {
    await page.addStyleTag({ content: '.config-editor { --config-editor-bg: rgb(200, 0, 0); }' });
    const background = () =>
      page.locator('.config-editor').first().evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
    expect(await background()).toBe('rgb(200, 0, 0)');
    await page.locator('.demo__theme-select').selectOption('nord');
    await expect(page.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
    expect(await background()).toBe('rgb(200, 0, 0)');
  });

  test('themes still differ without a page rule', async ({ page }) => {
    const background = () =>
      page.locator('.config-editor').first().evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
    const dark = await background();
    await page.locator('.demo__theme-select').selectOption('nord');
    await expect(page.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
    expect(await background()).not.toBe(dark);
    expect(await background()).toBe('#2e3440');
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const card = (page) => page.locator('.config-editor__hover');

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  // The middle of the character at one-based `line`:`column`, as drawn.
  const charBox = (page, line, column) =>
    page.evaluate(
      ([line, column]) => {
        const row = document.querySelectorAll('.config-editor__highlight .config-editor__code-line')[line - 1];
        const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
        let left = column - 1;
        let node;
        while ((node = walker.nextNode())) {
          if (left < node.length) break;
          left -= node.length;
        }
        const range = document.createRange();
        range.setStart(node, left);
        range.setEnd(node, left + 1);
        const box = range.getBoundingClientRect();
        return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
      },
      [line, column],
    );

  // Each language's document: `old` (deprecated, with docs and a docs link)
  // on `line`, from `column`; `port` (documented too) on the next line.
  const schema = JSON.stringify({
    type: 'object',
    properties: {
      old: { type: 'integer', deprecated: true, description: 'Old thing', 'x-docs-url': 'https://example.com/old' },
      port: { type: 'integer', description: 'The port', 'x-docs-url': 'https://example.com/port' },
    },
  });

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('L23-U8: the deprecated badge reads in every theme', () => {
    const luminance = (rgb) =>
      rgb
        .map((part) => part / 255)
        .map((part) => (part <= 0.03928 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4))
        .reduce((sum, part, i) => sum + part * [0.2126, 0.7152, 0.0722][i], 0);
    const ratio = (a, b) => {
      const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (light + 0.05) / (dark + 0.05);
    };

    test('at least 4.5:1 on the card', async ({ page }) => {
      await open(page, { value: 'old: 1\n', schema });
      const themes = await page.locator('#theme option').evaluateAll((options) => options.map((option) => option.value));
      expect(themes.length).toBeGreaterThan(10);
      const failures = [];
      for (const theme of themes) {
        await page.evaluate((theme) => window.editor.update({ theme }), theme);
        // Away until the last theme's card has gone, then onto the key again.
        await page.mouse.move(790, 590);
        await expect(card(page)).toHaveCount(0);
        const key = await charBox(page, 1, 2);
        await page.mouse.move(key.x - 2, key.y);
        await page.mouse.move(key.x, key.y);
        const badge = card(page).locator('.config-editor__hover-deprecated');
        await expect(badge).toBeVisible();
        const [fore, back] = await badge.evaluate((element) => {
          const rgb = (color) => color.match(/[\d.]+/g).slice(0, 3).map(Number);
          return [rgb(getComputedStyle(element).color), rgb(getComputedStyle(element.closest('.config-editor__hover')).backgroundColor)];
        });
        const measured = ratio(fore, back);
        if (measured < 4.5) failures.push(`${theme}: ${measured.toFixed(2)}`);
      }
      expect(failures).toEqual([]);
    });
  });
});

test.describe("the unsaved-changes dot (A33-3)", () => {
  setupEditor(test, "");

  test("is 4.5:1 against its background in every theme", async ({ page }) => {
    const themes = await page
      .locator(".demo__theme-select option")
      .evaluateAll((options) => options.map((option) => option.value));
    expect(themes.length).toBe(11);
    const failures = [];
    for (const theme of themes) {
      await page.locator(".demo__theme-select").selectOption(theme);
      const [fg, bg] = await page
        .locator(".demo__host-status")
        .evaluate((dot) => {
          // Colors resolve through a canvas: any CSS color becomes rgba.
          const rgb = (color) => {
            const canvas = document.createElement("canvas").getContext("2d");
            canvas.fillStyle = "#000";
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
          for (const [r, g, b, a] of layers.reverse())
            under = [r, g, b].map(
              (v, i) => (v * a + under[i] * (255 - a)) / 255,
            );
          return [rgb(getComputedStyle(dot).color).slice(0, 3), under];
        });
      const measured = ratio(fg, bg);
      if (!(measured >= 4.5))
        failures.push(`${theme}: ${measured.toFixed(2)} < 4.5`);
    }
    expect(failures).toEqual([]);
  });
});
