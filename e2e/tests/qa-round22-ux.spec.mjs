import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 22 (UX): a key with docs and a problem shows one popup, the
// hover card listing the problems above the docs, by the pointer, the
// keyboard (`Ctrl+K Ctrl+I`) and a tap, while F8 and a problem without
// docs keep the problem tip. Through the npm package's vanilla page (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input');
const tip = (page) => page.locator('.config-editor__problem-tip');
const card = (page) => page.locator('.config-editor__hover');
const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

// The box of the character at one-based `line`:`column`, as drawn.
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

// Puts the caret before character `column` of `line` (one-based).
const caretAt = (page, line, column) =>
  input(page).evaluate(
    (element, [line, column]) => {
      const lines = element.value.split('\n');
      const at = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0) + column - 1;
      element.focus();
      element.setSelectionRange(at, at);
      element.dispatchEvent(new Event('select'));
    },
    [line, column],
  );

const schema = JSON.stringify({
  type: 'object',
  properties: {
    config: {
      type: 'object',
      properties: { old: { type: 'integer', deprecated: true, description: 'Old thing' }, port: { type: 'integer' } },
    },
  },
});
// Each language's document: the deprecated `old` (docs and a problem) on
// line 2 from `column`, `port` on line 3 from `column`.
const DOCS = {
  yaml: { value: 'config:\n  old: 1\n  port: 2\n', column: 3 },
  json: { value: '{\n  "config": {\n  "old": 1,\n  "port": 2\n  }\n}\n', column: 4 },
  toml: { value: '[config]\nold = 1\nport = 2\n', column: 1 },
  hcl: { value: 'config {\n  old = 1\n  port = 2\n}\n', column: 3 },
  xml: { value: '<config>\n  <old>1</old>\n  <port>2</port>\n</config>\n', column: 4 },
};
// JSON's keys sit a line lower (the `{` line first).
const lineOf = (language, line) => (language === 'json' ? line + 1 : line);

// One popup: the card, with the deprecation problem above the docs; no tip.
async function onePopup(page) {
  await expect(card(page)).toBeVisible();
  await expect(card(page).locator('.config-editor__problem-tip-entry')).toHaveCount(1);
  await expect(card(page).locator('.config-editor__problem-tip-entry')).toContainText(/deprecated/i);
  await expect(card(page)).toContainText('Old thing');
  await expect(tip(page)).toBeHidden();
}

test.describe('M22-U1: docs and a problem at one place show in one popup', () => {
  test.use({ viewport: { width: 900, height: 500 } });

  for (const [language, doc] of Object.entries(DOCS)) {
    const line = lineOf(language, 2);
    test(`${language}: hover and Ctrl+K Ctrl+I show the card with the problem; F8 and a problem without docs show the tip`, async ({ page }) => {
      const errors = await open(page, { value: doc.value, language, schema });
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
      // The pointer on the key: the card lists the problem, the tip yields.
      const key = await charBox(page, line, doc.column + 1);
      await page.mouse.move(key.x, key.y);
      await onePopup(page);
      // Away: both close.
      await page.mouse.move(890, 490);
      await expect(card(page)).toBeHidden();
      await expect(tip(page)).toBeHidden();
      // The keyboard's way: the same one popup.
      await caretAt(page, line, doc.column + 1);
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+i');
      await onePopup(page);
      await page.keyboard.press('Escape');
      await expect(card(page)).toBeHidden();
      // F8 goes to the problem: its tip, no card.
      await caretAt(page, 1, 1);
      await page.keyboard.press('F8');
      await expect(tip(page)).toBeVisible();
      await expect(tip(page)).toContainText(/deprecated/i);
      await expect(card(page)).toBeHidden();
      await page.keyboard.press('Escape');
      // A host's problem on a key without docs: the tip, as before.
      const portLine = lineOf(language, 3);
      await setHost(page, [{ range: { startLine: portLine, startColumn: doc.column, endColumn: doc.column + 4 }, severity: 'error', message: 'port is taken' }]);
      await expect(page.locator('.config-editor__diagnostic-marker[data-message="port is taken"]')).toHaveCount(1);
      const port = await charBox(page, portLine, doc.column + 1);
      await page.mouse.move(port.x, port.y);
      await expect(tip(page)).toBeVisible();
      await expect(tip(page)).toContainText('port is taken');
      expect(errors).toEqual([]);
    });
  }

  test.describe('by touch', () => {
    test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });

    for (const [language, doc] of Object.entries(DOCS)) {
      test(`${language}: a tap on the key shows the card with the problem`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema });
        await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
        const key = await charBox(page, lineOf(language, 2), doc.column + 1);
        await page.touchscreen.tap(key.x, key.y);
        await onePopup(page);
        expect(errors).toEqual([]);
      });
    }
  });
});

test.describe('L22-U2, L22-U3: the toolbar and the footer fit', () => {
  const portSchema = JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } });

  for (const width of [260, 390, 480]) {
    test(`at ${width} px the schema state keeps its check, beside the language`, async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width, height: 400 } });
      const page = await context.newPage();
      const errors = await open(page, { value: 'port: 1\n', schema: portSchema });
      const check = await page.locator('.config-editor__schema-check').boundingBox();
      expect(check?.width ?? 0).toBeGreaterThan(4);
      expect(errors).toEqual([]);
      await context.close();
    });
  }

  test('a language without a schema state has no separator before it', async ({ page }) => {
    await open(page, { value: 'port: 1\n', schema: null });
    await expect(page.locator('.config-editor__schema-state')).toHaveCount(0);
    await expect(page.locator('.config-editor__language')).toHaveCSS('border-left-width', '0px');
  });

  // Whether the hint is inside the footer's one row, and whether any of
  // its pieces broke over lines.
  const hintLayout = (page) =>
    page.locator('.config-editor__status').evaluate((status) => {
      const box = status.getBoundingClientRect();
      const hint = status.querySelector('.config-editor__hint');
      const hintBox = hint.getBoundingClientRect();
      return {
        height: Math.round(box.height),
        shown: hintBox.width > 0 && hintBox.top >= box.top - 0.5 && hintBox.bottom <= box.bottom + 0.5,
        // A text broken over lines has a box per line.
        broken: [...hint.querySelectorAll('span, kbd')].some((piece) =>
          [...piece.childNodes].some((node) => {
            if (node.nodeType !== Node.TEXT_NODE) return false;
            const range = document.createRange();
            range.selectNodeContents(node);
            return range.getClientRects().length > 1;
          }),
        ),
      };
    });

  for (const width of [700, 1100]) {
    test(`at ${width} px the suggestions hint never wraps; it gives way whole to the problem summary`, async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width, height: 400 } });
      const page = await context.newPage();
      const errors = await open(page, { value: 'port: 1\n', schema: portSchema });
      await expect.poll(async () => (await hintLayout(page)).shown).toBe(true);
      const long = 'The upstream refused the configuration because the port is reserved for the metrics endpoint';
      for (const problems of [
        // Counts and the hint to hover them (QA's case), then an unplaced
        // problem's long message.
        [{ range: { startLine: 1, startColumn: 1, endColumn: 5 }, severity: 'warning', message: 'w' }],
        [{ severity: 'error', message: long }],
      ]) {
        await setHost(page, problems);
        await expect(page.locator('.config-editor__diagnostic-status')).toContainText(/1 (error|warning)/);
        await input(page).click();
        await page.keyboard.press('Control+End');
        await page.keyboard.type('x');
        const layout = await hintLayout(page);
        expect(layout.height).toBe(28);
        expect(layout.broken).toBe(false);
      }
      expect(errors).toEqual([]);
      await context.close();
    });
  }
});

test.describe('L22-U5: the change preview stays inside the editor on a phone', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });

  test('Alt+F3 on a changed line', async ({ page }) => {
    const errors = await open(page, { value: 'listen: 7835\nhost: example\n' });
    // The editor inset from the window's edges, as on a page with margins.
    await page.addStyleTag({ content: '#editor { margin: 0 20px; }' });
    await caretAt(page, 1, 13);
    await page.keyboard.type('1');
    await page.keyboard.press('Alt+F3');
    const preview = page.locator('.config-editor__diff-peek');
    await expect(preview).toBeVisible();
    const box = await preview.boundingBox();
    const frame = await page.locator('.config-editor__frame').first().boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(frame.x - 0.5);
    expect(box.x + box.width).toBeLessThanOrEqual(frame.x + frame.width + 0.5);
    expect(errors).toEqual([]);
  });
});

test.describe('L22-U7: forced colors keep the severities apart', () => {
  test('errors, warnings and infos have their own underline style', async ({ browser }) => {
    const context = await browser.newContext({ forcedColors: 'active', viewport: { width: 900, height: 400 } });
    const page = await context.newPage();
    const errors = await open(page, { value: 'a: 1\nb: 2\nc: 3\n', schema: '{}' });
    const at = (line, severity) => ({ range: { startLine: line, startColumn: 1, endColumn: 2 }, severity, message: severity });
    await setHost(page, [at(1, 'error'), at(2, 'warning'), at(3, 'info')]);
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(3);
    const styles = await page.evaluate(() =>
      ['error', 'warning', 'info'].map(
        (message) => getComputedStyle(document.querySelector(`.config-editor__diagnostic-marker[data-message="${message}"]`)).textDecorationStyle,
      ),
    );
    expect(new Set(styles).size).toBe(3);
    expect(errors).toEqual([]);
    await context.close();
  });
});

test.describe('L22-U6: the demo tab is as wide as its name', () => {
  test('at 2560 px', async ({ browser, baseURL }) => {
    const context = await browser.newContext({ viewport: { width: 2560, height: 800 } });
    const page = await context.newPage();
    await page.goto(`${baseURL}?language=yaml`);
    await page.locator('.config-editor__input').waitFor();
    const spare = await page.locator('.demo__toolbar .config-editor__file').evaluate((file) => {
      const used = [...file.children].reduce((sum, child) => sum + child.getBoundingClientRect().width, 0);
      const gaps = parseFloat(getComputedStyle(file).columnGap) || 0;
      return file.getBoundingClientRect().width - used - gaps * (file.children.length - 1);
    });
    // A few pixels of font rounding (CI's fonts differ); the bug left 1480.
    expect(spare).toBeLessThan(24);
    await context.close();
  });
});
