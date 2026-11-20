import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Sticky scroll through the npm package: the header lines of the blocks
// holding the top line stay at the top, outermost first, in every language,
// in an Edit-as view, with folds closed and in a documents view; the
// innermost is pushed up as its block ends; a click goes back to its line;
// the settings switch it live; a caret moved under them is scrolled back
// below them; they draw secret placeholders as the text does, take no focus
// and are hidden from screen readers.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const entries = (count, line) => Array.from({ length: count }, (_, index) => line(index + 1)).join('\n');
const prefix = (count, line) => (count ? `${entries(count, line)}\n` : '');

/** Each language's spelling of `a` holding `b` holding `c`, `c` holding 30 entries, after `before` top-level entries. */
const nested = {
  yaml: (before = 0) =>
    `${prefix(before, (i) => `p${i}: ${i}`)}a:\n  b:\n    c:\n${entries(30, (i) => `      e${i}: ${i}`)}\n    h: 5\n  i: 6\nj: 7\n`,
  json: (before = 0) =>
    `{\n${prefix(before, (i) => `  "p${i}": ${i},`)}  "a": {\n    "b": {\n      "c": {\n${entries(30, (i) => `        "e${i}": ${i}${i < 30 ? ',' : ''}`)}\n      },\n      "h": 5\n    },\n    "i": 6\n  },\n  "j": 7\n}\n`,
  toml: (before = 0) => `${prefix(before, (i) => `p${i} = ${i}`)}\n[a.b.c]\n${entries(30, (i) => `e${i} = ${i}`)}\n\n[a.b]\nh = 5\n`,
  hcl: (before = 0) =>
    `${prefix(before, (i) => `p${i} = ${i}`)}a {\n  b {\n    c {\n${entries(30, (i) => `      e${i} = ${i}`)}\n    }\n    h = 5\n  }\n  i = 6\n}\nj = 7\n`,
  xml: (before = 0) =>
    `<r>\n${prefix(before, (i) => `  <p${i}>${i}</p${i}>`)}  <a>\n    <b>\n      <c>\n${entries(30, (i) => `        <e${i}>${i}</e${i}>`)}\n      </c>\n      <h>5</h>\n    </b>\n    <i>6</i>\n  </a>\n</r>\n`,
};

/** The sticky lines each language shows with `e5` at the top. */
const headers = {
  yaml: ['a:', 'b:', 'c:'],
  json: ['{', '"a": {', '"b": {', '"c": {'],
  toml: ['[a.b.c]'],
  hcl: ['a {', 'b {', 'c {'],
  xml: ['<r>', '<a>', '<b>', '<c>'],
};

const input = (page) => page.locator('#sticky .config-editor__input');

/** Mounts a `ConfigEditor` (or, with `documents`, a `ConfigDocuments`) with `options` in a 440px tall box. */
async function mount(page, options, documents = false) {
  await page.goto(vanilla);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(
    async ({ options, documents }) => {
      const { ConfigEditor, ConfigDocuments } = await import(new URL('../config-editor/index.js', location.href).href);
      const element = document.createElement('div');
      element.id = 'sticky';
      element.style.setProperty('--config-editor-height', '440px');
      element.style.setProperty('--config-editor-max-height', '440px');
      document.body.prepend(element);
      window.sticky = documents ? new ConfigDocuments(element, options) : new ConfigEditor(element, options);
    },
    { options, documents },
  );
  await input(page).first().waitFor();
}

/** The textarea's metrics. */
const metrics = (page) =>
  input(page).evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      lineHeight: parseFloat(style.lineHeight),
      paddingTop: parseFloat(style.paddingTop),
      scrollTop: element.scrollTop,
      top: element.getBoundingClientRect().top,
    };
  });

/** Scrolls so textarea row `row` (0-based) is at the top, `extra` pixels further. */
async function scrollToRow(page, row, extra = 0) {
  await input(page).evaluate(
    (element, { row, extra }) => {
      const style = getComputedStyle(element);
      element.scrollTop = parseFloat(style.paddingTop) + row * parseFloat(style.lineHeight) + extra;
      element.dispatchEvent(new Event('scroll'));
    },
    { row, extra },
  );
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
}

/** The row (0-based) of the first textarea line that starts, trimmed, with `marker`. */
const rowOf = (page, marker) =>
  input(page).evaluate((element, marker) => element.value.split('\n').findIndex((line) => line.trim().startsWith(marker)), marker);

/** The sticky lines, top to bottom: text, line number, top relative to the textarea. */
const sticky = (page) =>
  page.locator('#sticky .config-editor__sticky-line').evaluateAll((lines) => {
    const area = document.querySelector('#sticky .config-editor__input').getBoundingClientRect();
    return lines
      .map((line) => ({ text: line.textContent.trim(), line: Number(line.dataset.line), top: line.getBoundingClientRect().top - area.top }))
      .sort((a, b) => a.line - b.line);
  });
const texts = async (page) => (await sticky(page)).map((line) => line.text);
const numbers = (page) =>
  page
    .locator('#sticky .config-editor__sticky-number')
    .evaluateAll((lines) => lines.map((line) => Number(line.textContent)).sort((a, b) => a - b));

/** Where the sticky area ends, relative to the textarea's top. */
const stickyBottom = (page) =>
  page.locator('#sticky .config-editor__sticky').evaluate((layer) => {
    const area = document.querySelector('#sticky .config-editor__input').getBoundingClientRect();
    return layer.getBoundingClientRect().bottom - area.top;
  });

/** The caret's line (1-based) and its row's top relative to the textarea (no folds). */
const caretRow = (page) =>
  input(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const line = element.value.slice(0, element.selectionStart).split('\n').length;
    return { line, top: parseFloat(style.paddingTop) + (line - 1) * parseFloat(style.lineHeight) - element.scrollTop };
  });

test.describe('sticky scroll', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const language of Object.keys(nested)) {
    test(`${language}: the headers of the blocks above the top stick, outermost first, with their line numbers`, async ({ page }) => {
      await mount(page, { language, value: nested[language]() });
      await expect(page.locator('#sticky .config-editor__sticky-line')).toHaveCount(0);
      await scrollToRow(page, await rowOf(page, language === 'xml' ? '<e5>' : language === 'json' ? '"e5"' : 'e5'));
      await expect.poll(() => texts(page)).toEqual(headers[language]);
      const lines = (await sticky(page)).map((line) => line.line);
      expect(await numbers(page)).toEqual(lines);
      const shown = await input(page).inputValue();
      for (const line of await sticky(page)) expect(shown.split('\n')[line.line - 1].trim()).toBe(line.text);
    });
  }

  test('the innermost header is pushed up as its block ends, then goes', async ({ page }) => {
    // Lines after the blocks, so the text scrolls past their ends.
    await mount(page, { language: 'yaml', value: `${nested.yaml()}${entries(40, (i) => `z${i}: ${i}`)}\n` });
    const { lineHeight } = await metrics(page);
    const last = await rowOf(page, 'e30:');
    // `c:` sits third while its last row is below the third place.
    await scrollToRow(page, last - 2);
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
    expect((await sticky(page))[2].top).toBeCloseTo(2 * lineHeight, 0);
    await scrollToRow(page, last - 2, 6);
    await expect.poll(async () => (await sticky(page))[2]?.top).toBeCloseTo(2 * lineHeight - 6, 0);
    expect((await sticky(page))[1].top).toBeCloseTo(lineHeight, 0);
    await scrollToRow(page, last - 1);
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:']);
  });

  test('a click on a sticky line scrolls back to it with the caret at its start, keeping the focus in the text', async ({ page }) => {
    await mount(page, { language: 'yaml', value: `${nested.yaml(30)}${entries(40, (i) => `z${i}: ${i}`)}\n` });
    await scrollToRow(page, await rowOf(page, 'e10:'));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
    await expect(page.locator('#sticky .config-editor__sticky')).toHaveAttribute('aria-hidden', 'true');
    await expect(page.locator('#sticky .config-editor__sticky-numbers')).toHaveAttribute('aria-hidden', 'true');
    await page.locator('#sticky .config-editor__sticky-line', { hasText: 'c:' }).click();
    const c = (await rowOf(page, 'c:')) + 1;
    await expect.poll(async () => (await caretRow(page)).line).toBe(c);
    expect(await input(page).evaluate((element) => document.activeElement === element)).toBe(true);
    expect(await input(page).evaluate((element) => element.selectionStart === element.selectionEnd)).toBe(true);
    const { top } = await caretRow(page);
    const { lineHeight } = await metrics(page);
    expect(top).toBeGreaterThanOrEqual((await stickyBottom(page)) - 1);
    expect(top).toBeLessThan(4 * lineHeight);
    // A number clicks the same way.
    await scrollToRow(page, await rowOf(page, 'e20:'));
    await page.locator('#sticky .config-editor__sticky-number', { hasText: String(c - 1) }).click();
    await expect.poll(async () => (await caretRow(page)).line).toBe(c - 1);
  });

  test('the settings switch it live: off, back on, and at most the lines asked for', async ({ page }) => {
    await mount(page, { language: 'yaml', value: nested.yaml(), folding: false });
    await scrollToRow(page, await rowOf(page, 'e5:'));
    // The blocks come from folding's ranges, also with folding off.
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
    await page.evaluate(() => window.sticky.update({ stickyScroll: false }));
    await expect(page.locator('#sticky .config-editor__sticky-line')).toHaveCount(0);
    await expect(page.locator('#sticky .config-editor__sticky-number')).toHaveCount(0);
    await page.evaluate(() => window.sticky.update({ stickyScroll: true, stickyScrollMaxLines: 2 }));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:']);
    await page.evaluate(() => window.sticky.update({ stickyScrollMaxLines: null }));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
  });

  test('a caret moved under the sticky lines is scrolled back below them', async ({ page }) => {
    await mount(page, { language: 'yaml', value: nested.yaml(30) });
    const e10 = await rowOf(page, 'e10:');
    await scrollToRow(page, e10);
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
    // A click on the row right below the sticky lines, then up a line at a time.
    const { lineHeight, top } = await metrics(page);
    const box = await input(page).boundingBox();
    await page.mouse.click(box.x + 80, top + (await stickyBottom(page)) + lineHeight / 2);
    const start = (await caretRow(page)).line;
    for (let step = 1; step <= 6; step += 1) {
      await page.keyboard.press('ArrowUp');
      await expect.poll(async () => (await caretRow(page)).line).toBe(start - step);
      await expect
        .poll(async () => (await caretRow(page)).top - (await stickyBottom(page)), { message: `step ${step}` })
        .toBeGreaterThanOrEqual(-1);
    }
  });

  test('an Edit-as view sticks its own headers', async ({ page }) => {
    await mount(page, { language: 'yaml', value: nested.yaml() });
    await page.evaluate(() => window.sticky.update({ editAs: 'json' }));
    await expect.poll(() => input(page).inputValue()).toContain('"e5": 5');
    await scrollToRow(page, await rowOf(page, '"e5"'));
    await expect.poll(() => texts(page)).toEqual(headers.json);
  });

  test('with a fold closed above, the sticky lines keep their own line numbers', async ({ page }) => {
    const value = `a:\n  x:\n${entries(10, (i) => `    x${i}: ${i}`)}\n  b:\n${entries(30, (i) => `    e${i}: ${i}`)}\nj: 7\n`;
    await mount(page, { language: 'yaml', value });
    expect(await page.evaluate(() => window.sticky.fold('/a/x'))).toBe(true);
    await expect.poll(() => input(page).inputValue()).not.toContain('x1:');
    await scrollToRow(page, await rowOf(page, 'e5:'));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:']);
    expect(await numbers(page)).toEqual([1, 13]);
  });

  test('a secret placeholder on a header line shows as the text shows it', async ({ page }) => {
    const value = `{\n  "tls": { "pin": "«secret 0b1c»",\n${entries(30, (i) => `    "e${i}": ${i},`)}\n    "end": 0\n  }\n}\n`;
    const schema = { type: 'object', properties: { tls: { type: 'object', properties: { pin: { type: 'string', 'x-secret': true } } } } };
    await mount(page, { language: 'json', value, schema: JSON.stringify(schema) });
    await scrollToRow(page, await rowOf(page, '"e5"'));
    await expect.poll(() => texts(page)).toEqual(['{', '"tls": { "pin": "«secret 0b1c»",']);
    const placeholder = page.locator('#sticky .config-editor__sticky-line .config-editor__secret');
    await expect(placeholder).toHaveText('«secret 0b1c»');
  });

  test('in forced colors a line in the system text color divides the sticky lines from the text', async ({ browser }) => {
    const context = await browser.newContext({ forcedColors: 'active', viewport: { width: 1100, height: 620 } });
    const page = await context.newPage();
    await mount(page, { language: 'yaml', value: nested.yaml() });
    await scrollToRow(page, await rowOf(page, 'e5:'));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:', 'c:']);
    // Pixel rows across the right of the text, where no header reaches:
    // the last one of the sticky lines, and one a few pixels above it.
    const area = await input(page).boundingBox();
    const bottom = area.y + (await stickyBottom(page));
    const strip = (y) => page.screenshot({ clip: { x: area.x + area.width - 160, y, width: 120, height: 1 }, animations: 'disabled' });
    expect((await strip(bottom - 1)).equals(await strip(bottom - 4)), 'the dividing line shows').toBe(false);
    await context.close();
  });

  test('a documents view sticks the headers of the document shown', async ({ page }) => {
    await mount(page, { documents: [{ id: 'main', fileName: 'main.yaml', value: nested.yaml() }], stickyScrollMaxLines: 2 }, true);
    await scrollToRow(page, await rowOf(page, 'e5:'));
    await expect.poll(() => texts(page)).toEqual(['a:', 'b:']);
  });
});
