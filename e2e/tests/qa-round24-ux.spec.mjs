import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 24 (UX): a long hover card scrolls with the wheel and the
// keyboard whether or not it has a link; the pointer resting in the gap
// between the line and the card keeps it. Through the npm package's pages
// (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (name, options = {}) =>
  `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 800, height: 400 } });

const input = (page) => page.locator('.config-editor__input');
const card = (page) => page.locator('.config-editor__hover');
const announcement = (page) => page.locator('.config-editor__announcement');
const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl('vanilla.html', options));
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
      return { x: box.left + box.width / 2, y: box.top + box.height / 2, bottom: box.bottom };
    },
    [line, column],
  );

// Which popups show now: 'hover', 'problem-tip', both or none.
const shown = (page) =>
  page.evaluate(() =>
    ['hover', 'problem-tip']
      .filter((name) => document.querySelector(`.config-editor__${name}`)?.checkVisibility({ visibilityProperty: true }))
      .join('+'),
  );

// Each language's document with `port` on line 3.
const DOCS = {
  yaml: 'a: 1\nb: 2\nport: 3\nc: 4\n',
  json: '{\n  "b": 2,\n  "port": 3,\n  "c": 4\n}\n',
  toml: 'a = 1\nb = 2\nport = 3\nc = 4\n',
  hcl: 'a = 1\nb = 2\nport = 3\nc = 4\n',
  xml: '<a>1</a>\n<b>2</b>\n<port>3</port>\n<c>4</c>\n',
};
const portColumn = (value) => value.split('\n')[2].indexOf('port') + 1;
const portSchema = (description, extra = {}) =>
  JSON.stringify({ type: 'object', properties: { port: { type: 'integer', description, ...extra } } });
const LONG = 'The port is documented at length. '.repeat(40);
// Many host warnings on `port`, none with a link.
const manyProblems = (value) => {
  const column = portColumn(value);
  return Array.from({ length: 25 }, (_, i) => ({
    range: { startLine: 3, startColumn: column, endColumn: column + 4 },
    severity: 'warning',
    message: `host problem ${i} with words enough to wrap onto a second line of the card`,
  }));
};

// Points at `port`, waits for its card, then moves into the card in small
// steps (slower than the hover pause at the end): what showed at each step.
async function intoCard(page, value) {
  const key = await charBox(page, 3, portColumn(value) + 1);
  await page.mouse.move(key.x, key.y);
  await expect(card(page)).toBeVisible();
  await expect(card(page)).toHaveClass(/config-editor__hover--scrolls/);
  const box = await card(page).boundingBox();
  const target = { x: Math.max(box.x + 30, key.x), y: box.y + box.height / 2 };
  const trace = [];
  for (let step = 1; step <= 10; step += 1) {
    await page.mouse.move(key.x + ((target.x - key.x) * step) / 10, key.y + ((target.y - key.y) * step) / 10);
    trace.push(await shown(page));
  }
  await page.waitForTimeout(500);
  trace.push(await shown(page));
  return trace;
}

// The wheel over the card scrolls the card, not the editor.
async function wheelScrollsCard(page) {
  const editorTop = await input(page).evaluate((element) => element.scrollTop);
  await page.mouse.wheel(0, 150);
  await expect.poll(() => card(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
  expect(await input(page).evaluate((element) => element.scrollTop)).toBe(editorTop);
}

test.describe('M24-U1: a long card without a link stays under the pointer and scrolls', () => {
  for (const [language, value] of Object.entries(DOCS)) {
    test(`${language}: long docs, the wheel`, async ({ page }) => {
      const errors = await open(page, { value, language, schema: portSchema(LONG) });
      expect(await intoCard(page, value)).toEqual(Array(11).fill('hover'));
      await expect(card(page)).toContainText('documented at length');
      await wheelScrollsCard(page);
      expect(errors).toEqual([]);
    });

    test(`${language}: many problems, no link: the card stays, not the problem tip`, async ({ page }) => {
      const errors = await open(page, { value, language, schema: portSchema('The port.') });
      await setHost(page, manyProblems(value));
      await expect(page.locator('.config-editor__diagnostic-status')).toContainText('25 warnings');
      expect(await intoCard(page, value)).toEqual(Array(11).fill('hover'));
      await expect(card(page)).toContainText('The port.');
      await expect(card(page)).toContainText('host problem 0');
      await wheelScrollsCard(page);
      await expect(card(page)).toContainText('The port.');
      expect(errors).toEqual([]);
    });

    test(`${language}: a second Ctrl+K Ctrl+I focuses the card; keys scroll it, Escape returns`, async ({ page }) => {
      const errors = await open(page, { value, language, schema: portSchema(LONG) });
      await setHost(page, manyProblems(value).slice(0, 6));
      const key = await charBox(page, 3, portColumn(value) + 1);
      await page.mouse.click(key.x, key.y);
      await page.mouse.move(790, 390);
      await expect(card(page)).toBeHidden();
      const caret = await input(page).evaluate((element) => element.selectionStart);
      const chord = async () => {
        await page.keyboard.press('Control+k');
        await page.keyboard.press('Control+i');
      };
      await chord();
      await expect(card(page)).toBeVisible();
      await expect(card(page)).toHaveClass(/config-editor__hover--scrolls/);
      await expect(input(page)).toBeFocused();
      await chord();
      await expect(card(page)).toBeFocused();
      await expect(announcement(page)).toContainText(/Escape/);
      const top = () => card(page).evaluate((element) => element.scrollTop);
      const most = await card(page).evaluate((element) => element.scrollHeight - element.clientHeight);
      expect(most).toBeGreaterThan(100);
      await page.keyboard.press('ArrowDown');
      await expect.poll(top).toBeGreaterThan(0);
      const afterArrow = await top();
      await page.keyboard.press('PageDown');
      await expect.poll(top).toBeGreaterThan(afterArrow + 40);
      await page.keyboard.press('End');
      await expect.poll(top).toBeGreaterThanOrEqual(most - 1);
      await page.keyboard.press('ArrowUp');
      await expect.poll(top).toBeLessThan(most - 1);
      await page.keyboard.press('PageUp');
      await page.keyboard.press('Home');
      await expect.poll(top).toBe(0);
      // The text never moved, and gets the focus back with its caret.
      expect(await input(page).evaluate((element) => element.selectionStart)).toBe(caret);
      await page.keyboard.press('Escape');
      await expect(input(page)).toBeFocused();
      await expect(card(page)).toBeHidden();
      expect(await input(page).evaluate((element) => [element.selectionStart, element.value])).toEqual([caret, value]);
      expect(errors).toEqual([]);
    });
  }

  test('Tab still reaches a link, in Ctrl+M mode, and Shift+Tab returns', async ({ page }) => {
    const value = DOCS.yaml;
    await open(page, { value, schema: portSchema(LONG, { 'x-docs-url': 'https://example.com/port' }) });
    const key = await charBox(page, 3, 2);
    await page.mouse.click(key.x, key.y);
    await page.mouse.move(790, 390);
    await page.keyboard.press('Control+m');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(card(page)).toBeVisible();
    await page.keyboard.press('Tab');
    await expect(card(page).locator('.config-editor__hover-link')).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(input(page)).toBeFocused();
    await expect(card(page)).toBeVisible();
  });
});

test.describe('L24-U6: the footer hint says tap on a touch screen', () => {
  for (const touch of [true, false]) {
    test(touch ? 'a phone' : 'a mouse', async ({ browser }) => {
      const context = await browser.newContext(
        touch ? { hasTouch: true, isMobile: true, viewport: { width: 390, height: 600 } } : { viewport: { width: 800, height: 400 } },
      );
      const page = await context.newPage();
      await open(page, { value: 'port: x\n', schema: portSchema('The port.') });
      await expect(page.locator('.config-editor__diagnostic-status')).toContainText(
        touch ? '1 error · tap underlined text' : '1 error · hover or focus underlined text',
      );
      await context.close();
    });
  }
});

test.describe('L24-U1: the pointer resting between the line and the card keeps it', () => {
  for (const [language, value] of Object.entries(DOCS)) {
    for (const link of [true, false]) {
      test(`${language}: ${link ? 'with a link' : 'long, no link'}`, async ({ page }) => {
        const extra = link ? { 'x-docs-url': 'https://example.com/port' } : {};
        const errors = await open(page, { value, language, schema: portSchema(link ? 'The port.' : LONG, extra) });
        const key = await charBox(page, 3, portColumn(value) + 1);
        await page.mouse.move(key.x, key.y);
        await expect(card(page)).toBeVisible();
        const box = await card(page).boundingBox();
        expect(box.y).toBeGreaterThan(key.bottom);
        // In the gap: below the line, above the card.
        await page.mouse.move(key.x, (key.bottom + box.y) / 2 + 0.5, { steps: 3 });
        await page.waitForTimeout(1200);
        expect(await shown(page)).toBe('hover');
        // Then on into it, and it stays.
        await page.mouse.move(key.x, box.y + 10, { steps: 2 });
        await page.waitForTimeout(600);
        expect(await shown(page)).toBe('hover');
        expect(errors).toEqual([]);
      });
    }
  }
});
