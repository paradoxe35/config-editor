import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// `revealRange({ start, end })` selects any stretch of the text on screen
// (a comment, a value, the last line), scrolls it into view inside the
// editor and focuses it, the host page left where it is; past the end it
// is clamped, and an empty document has nothing to reveal.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;
test.skip(!built, 'js packages not built: run `npm run build` in js/');

const lines = Array.from({ length: 400 }, (_, index) => (index === 300 ? '# a note about é😀 limits' : `key${index}: välue ${index}`));
const text = lines.join('\n');

const input = (browserPage) => browserPage.locator('.config-editor__input');

/** The text area's selection and scroll, and the page's scroll. */
const where = (browserPage) =>
  input(browserPage).evaluate((element) => ({
    start: element.selectionStart,
    end: element.selectionEnd,
    scrollTop: element.scrollTop,
    focused: document.activeElement === element,
    pageY: window.scrollY,
  }));

test.describe('revealRange', () => {
  test.skip(!built, 'js packages not built');

  test.beforeEach(async ({ page: browserPage }) => {
    await browserPage.setViewportSize({ width: 1000, height: 700 });
    await browserPage.goto(page('vanilla.html'));
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await browserPage.addStyleTag({ content: 'body { padding-bottom: 4000px; }' });
    await browserPage.evaluate((value) => window.editor.update({ value }), text);
    await expect(input(browserPage)).toHaveValue(/key399/);
    await browserPage.evaluate(() => window.scrollTo(0, 60));
  });

  const cases = [
    ['a comment', (offset) => [offset(300), offset(300) + lines[300].length]],
    ['part of a value', (offset) => [offset(200) + 'key200: '.length, offset(200) + 'key200: välue'.length]],
    ['the last line', (offset) => [offset(399), offset(399) + lines[399].length]],
  ];
  for (const [what, range] of cases) {
    test(`selects ${what} exactly, scrolled inside the editor`, async ({ page: browserPage }) => {
      const offset = (line) => lines.slice(0, line).join('\n').length + (line ? 1 : 0);
      const [start, end] = range(offset);
      const pageY = await browserPage.evaluate(() => window.scrollY);
      expect(await browserPage.evaluate(([start, end]) => window.editor.revealRange({ start, end }), [start, end])).toBe(true);
      await browserPage.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50))));
      const now = await where(browserPage);
      expect(now).toMatchObject({ start, end, focused: true, pageY });
      expect(now.scrollTop).toBeGreaterThan(0);
      expect(await browserPage.evaluate(() => window.editor.text).then((shown) => shown.slice(start, end))).toBe(text.slice(start, end));
      // The selection's line is inside the editor's visible area.
      const inView = await input(browserPage).evaluate((element) => {
        const style = getComputedStyle(element);
        const lineHeight = parseFloat(style.lineHeight);
        const line = element.value.slice(0, element.selectionStart).split('\n').length - 1;
        const top = parseFloat(style.paddingTop) + line * lineHeight;
        return top >= element.scrollTop && top + lineHeight <= element.scrollTop + element.clientHeight;
      });
      expect(inView).toBe(true);
    });
  }

  test('a range past the end is clamped; an empty document has nothing to reveal', async ({ page: browserPage }) => {
    expect(await browserPage.evaluate(() => window.editor.revealRange({ start: 10, end: 1e9 }))).toBe(true);
    expect(await where(browserPage)).toMatchObject({ start: 10, end: text.length });
    expect(await browserPage.evaluate(() => window.editor.revealRange({ start: 1e9, end: 1e9 }))).toBe(true);
    expect(await where(browserPage)).toMatchObject({ start: text.length, end: text.length });
    await expect(browserPage.evaluate(() => window.editor.revealRange({ start: -1, end: 2 }))).rejects.toThrow(/start/);
    await browserPage.evaluate(() => window.editor.update({ value: '' }));
    await expect(input(browserPage)).toHaveValue('');
    expect(await browserPage.evaluate(() => window.editor.revealRange({ start: 0, end: 0 }))).toBe(false);
  });

  test('while editing as another language the range is in the text on screen', async ({ page: browserPage }) => {
    await browserPage.evaluate(() => window.editor.update({ value: 'a: 1\n# note\nb: 2\n', editAs: 'json' }));
    await expect(input(browserPage)).toHaveValue(/^\{/);
    const shown = await browserPage.evaluate(() => window.editor.text);
    const start = shown.indexOf('"b"');
    expect(await browserPage.evaluate((start) => window.editor.revealRange({ start, end: start + 3 }), start)).toBe(true);
    expect(await where(browserPage)).toMatchObject({ start, end: start + 3 });
  });
});
