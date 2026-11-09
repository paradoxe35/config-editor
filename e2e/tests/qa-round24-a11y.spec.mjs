import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 24 (accessibility and mobile): PageDown / PageUp in an editor
// that fits the window scroll the editor only, never the page; in a short
// window the text stays within its surface (every row has its line
// number) and the find widget within the window; under forced colors the
// selected suggestion is all HighlightText; the Edit-as notice's Dismiss
// and the hover card's Documentation link are 24px targets. Through the
// npm package's example pages (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name, options) =>
  `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(example('vanilla.html', options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

/** Waits two frames: what was put back before the next frame is drawn is back. */
const frames = (page) => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

// Sixty keys in each language, so a page key has somewhere to go.
const DOCUMENTS = {
  yaml: Array.from({ length: 60 }, (_, i) => `k${i}: ${i}\n`).join(''),
  json: `${JSON.stringify(Object.fromEntries(Array.from({ length: 60 }, (_, i) => [`k${i}`, i])), null, 2)}\n`,
  toml: Array.from({ length: 60 }, (_, i) => `k${i} = ${i}\n`).join(''),
};

test.describe('L24-A2: PageDown / PageUp in an editor that fits the window', () => {
  test.use({ viewport: { width: 1100, height: 700 } });

  for (const [language, value] of Object.entries(DOCUMENTS)) {
    for (const tallPage of [false, true]) {
      test(`${language}${tallPage ? ', in a tall page' : ''}: scroll the editor, never the page`, async ({ page }) => {
        const errors = await open(page, { value, language });
        if (tallPage) {
          await page.evaluate(() => {
            const filler = document.createElement('div');
            filler.style.height = '2000px';
            document.body.append(filler);
          });
        }
        const inView = await input(page).evaluate((element) => {
          const rect = element.getBoundingClientRect();
          return rect.top >= 0 && rect.bottom <= innerHeight;
        });
        expect(inView).toBe(true);
        await input(page).focus();
        const caretLine = () => input(page).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
        const scrolls = [];
        for (const key of ['PageDown', 'PageDown', 'PageUp', 'Shift+PageDown']) {
          await page.keyboard.press(key);
          await frames(page);
          scrolls.push(await page.evaluate(() => scrollY));
        }
        expect(scrolls).toEqual([0, 0, 0, 0]);
        // The keys still did their work: the caret moved a page down, and
        // the text scrolled.
        expect(await caretLine()).toBeGreaterThan(10);
        expect(await input(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
        expect(errors).toEqual([]);
      });
    }
  }
});

test.describe('L24-A3: a short window', () => {
  for (const [width, height] of [
    [320, 180],
    [320, 256],
    [640, 300],
    [740, 360],
  ]) {
    test(`${width}x${height}: the text stays in its surface and the find widget in the window`, async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width, height } });
      const page = await context.newPage();
      const errors = await open(page, { value: DOCUMENTS.yaml.split('\n').slice(0, 15).join('\n') });
      await input(page).focus();
      await page.keyboard.press('Control+End');
      await frames(page);
      const boxes = await page.evaluate(() => {
        const box = (selector) => document.querySelector(selector).getBoundingClientRect();
        const [surface, text, gutter] = ['.config-editor__surface', '.config-editor__input', '.config-editor__gutter'].map(box);
        return { surface: [surface.top, surface.bottom], text: [text.top, text.bottom], gutter: [gutter.top, gutter.bottom] };
      });
      // The textarea within its surface, the gutter beside all of it.
      expect(boxes.text[0]).toBeGreaterThanOrEqual(boxes.surface[0] - 1);
      expect(boxes.text[1]).toBeLessThanOrEqual(boxes.surface[1] + 1);
      expect(boxes.gutter[0]).toBeLessThanOrEqual(boxes.text[0] + 1);
      expect(boxes.gutter[1]).toBeGreaterThanOrEqual(boxes.text[1] - 1);

      await page.keyboard.press('Control+h');
      const find = page.locator('.config-editor__find');
      await expect(find).toBeVisible();
      const box = await find.boundingBox();
      expect(box.y).toBeGreaterThanOrEqual(0);
      expect(box.y + box.height).toBeLessThanOrEqual(height);
      expect(errors).toEqual([]);
      await context.close();
    });
  }
});

test.describe('L24-A6: forced colors', () => {
  test.use({ forcedColors: 'active', colorScheme: 'dark' });

  test('every part of the selected suggestion is HighlightText', async ({ page }) => {
    const errors = await open(page, {
      value: '',
      schema: JSON.stringify({
        type: 'object',
        properties: {
          port: { type: 'integer', description: 'Port to listen on.' },
          mode: { enum: ['fast', 'safe'], description: 'Routing mode.' },
          old: { type: 'string', deprecated: true, description: 'Old key.' },
        },
      }),
    });
    const highlightText = await page.evaluate(() => {
      const probe = document.createElement('span');
      probe.style.forcedColorAdjust = 'none';
      probe.style.color = 'HighlightText';
      document.body.append(probe);
      const color = getComputedStyle(probe).color;
      probe.remove();
      return color;
    });
    await input(page).focus();
    await page.keyboard.press('Control+Space');
    const options = page.locator('.config-editor__completion');
    await expect(options).toHaveCount(3);
    for (let row = 0; row < 3; row += 1) {
      const active = page.locator('.config-editor__completion-active');
      await expect(active).toHaveCount(1);
      const colors = await active.evaluate((option) =>
        [...option.querySelectorAll('*')]
          .filter((element) => [...element.childNodes].some((node) => node.nodeType === 3 && node.textContent.trim()))
          .map((element) => [element.className, getComputedStyle(element).color]),
      );
      expect(colors.length).toBeGreaterThan(1);
      for (const [name, color] of colors) expect(color, `row ${row}: ${name}`).toBe(highlightText);
      await page.keyboard.press('ArrowDown');
    }
    expect(errors).toEqual([]);
  });
});

test.describe('L24-A7: touch targets', () => {
  test.use({ viewport: { width: 390, height: 800 }, hasTouch: true, isMobile: true });

  const size = (locator) => locator.evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return [rect.width, rect.height];
  });

  test('the Edit-as notice’s Dismiss is at least 24 by 24', async ({ page }) => {
    const errors = await open(page, { value: '# a comment\nport: 1\n', editAs: 'json' });
    const dismiss = page.locator('.config-editor__edit-as-notice-dismiss');
    await expect(dismiss).toBeVisible();
    const [width, height] = await size(dismiss);
    expect(width).toBeGreaterThanOrEqual(24);
    expect(height).toBeGreaterThanOrEqual(24);
    expect(errors).toEqual([]);
  });

  test('the hover card’s Documentation link is at least 24 tall, and the card no taller for it', async ({ page }) => {
    const docs = { type: 'string', description: 'The server name.', 'x-docs-url': 'https://example.com/name' };
    const plain = { type: 'string', description: 'The server name.' };
    const card = page.locator('.config-editor__hover');
    const heights = [];
    for (const name of [docs, plain]) {
      await page.goto('about:blank');
      const errors = await open(page, { value: 'name: edge\n', schema: JSON.stringify({ type: 'object', properties: { name } }) });
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+i');
      await expect(card).toBeVisible();
      heights.push((await card.boundingBox()).height);
      if (name === docs) {
        const [width, height] = await size(page.locator('.config-editor__hover-link'));
        expect(width).toBeGreaterThanOrEqual(24);
        expect(height).toBeGreaterThanOrEqual(24);
      }
      expect(errors).toEqual([]);
    }
    // The link's line, no more: about one 11px line and the grid's gap.
    expect(heights[0] - heights[1]).toBeLessThan(26);
  });
});
