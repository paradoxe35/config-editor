import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Layout: the footer and toolbar fit at every width, phones, right-to-left pages, line heights, notices.

async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

async function open(page) {
  page.errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.module = await import('../config-editor/index.js');
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.mountPoint = () => {
      const element = document.createElement('div');
      document.body.prepend(element);
      return element;
    };
  });
}

test.describe("a user line-height keeps the painted rows on the textarea lines", () => {
  for (const lineHeight of ["1.5", "2"]) {
    test.describe(`line-height ${lineHeight} !important`, () => {
      setupEditor(test, "?sample=large");

      test("rows, gutter and find highlights follow the textarea", async ({
        page,
      }) => {
        await page.addStyleTag({
          content: `* { line-height: ${lineHeight} !important; }`,
        });
        const e = editor(page);
        await e.input.click();
        await page.keyboard.press("Control+f");
        await page.keyboard.type("listen");
        await page.waitForTimeout(500);
        const drift = await page.evaluate(() => {
          const area = document.querySelector(".config-editor__input");
          const top = area.getBoundingClientRect().top;
          const style = getComputedStyle(area);
          const pad = parseFloat(style.paddingTop);
          const lineHeight = parseFloat(style.lineHeight);
          const expected = (line) =>
            top + pad + (line - 1) * lineHeight - area.scrollTop;
          const rows = (selector, attribute) =>
            [...document.querySelectorAll(selector)].map((row) => ({
              line: Number(row.getAttribute(attribute)),
              drift:
                row.getBoundingClientRect().top -
                expected(Number(row.getAttribute(attribute))),
              height: row.getBoundingClientRect().height,
            }));
          return {
            lineHeight,
            code: rows(".config-editor__code-line", "data-line"),
            gutter: rows(".config-editor__gutter-content > div", "data-line"),
            matches: [
              ...document.querySelectorAll(".config-editor__search-match"),
            ].map((match) => {
              const rect = match.getBoundingClientRect();
              return { top: rect.top, height: rect.height };
            }),
            keys: [
              ...document.querySelectorAll(".config-editor__code-line"),
            ].map((row) => ({
              line: Number(row.getAttribute("data-line")),
              text: row.textContent,
              top: row.getBoundingClientRect().top,
            })),
          };
        });
        expect(drift.code.length).toBeGreaterThan(10);
        for (const row of [...drift.code, ...drift.gutter]) {
          expect(Math.abs(row.drift), `line ${row.line}`).toBeLessThanOrEqual(
            1,
          );
          expect(
            Math.abs(row.height - drift.lineHeight),
            `line ${row.line} height`,
          ).toBeLessThanOrEqual(1);
        }
        // A find highlight sits on its code row.
        expect(drift.matches.length).toBeGreaterThan(0);
        const row = drift.keys.find((row) => row.text.includes("listen"));
        const match = drift.matches[0];
        expect(Math.abs(match.top - row.top)).toBeLessThanOrEqual(1);
        expect(Math.abs(match.height - drift.lineHeight)).toBeLessThanOrEqual(
          1,
        );
      });

      test("a click on a row lands on its line", async ({ page }) => {
        await page.addStyleTag({
          content: `* { line-height: ${lineHeight} !important; }`,
        });
        const e = editor(page);
        await e.input.click();
        const target = await page.evaluate(() => {
          const row = document.querySelector(
            '.config-editor__code-line[data-line="12"]',
          );
          const rect = row.getBoundingClientRect();
          return { x: rect.left + 40, y: rect.top + rect.height / 2 };
        });
        await page.mouse.click(target.x, target.y);
        const line = await e.input.evaluate(
          (area) => area.value.slice(0, area.selectionStart).split("\n").length,
        );
        expect(line).toBe(12);
      });
    });
  }
});

test.describe('a phone-width footer', () => {
  test.use({ viewport: { width: 390, height: 800 } });
  setupEditor(test, '?language=yaml');

  test('keeps the line count visible with a long error', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: true\nbogus_key_with_a_very_long_name: 1\nanother_unknown_key: 2\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
    const count = page.locator('.config-editor__line-count');
    const box = await count.boundingBox();
    expect(box.x + box.width).toBeLessThanOrEqual(390);
  });
});

test.describe('yaml: editing', () => {
  setupEditor(test, '?language=yaml');
  test('the gutter stays above code scrolled sideways', async ({ page }) => {
    const e = await at(page, `key: ${'x'.repeat(400)}|`);
    await page.keyboard.press('End');
    const covered = await page.evaluate(() => {
      const gutter = document.querySelector('.config-editor__gutter').getBoundingClientRect();
      const top = document.elementFromPoint(gutter.left + gutter.width / 2, gutter.top + 12);
      return top.closest('.config-editor__gutter') !== null;
    });
    expect(covered).toBe(true);
  });
});

test.describe('the toolbar while editing as another language', () => {
  test.use({ viewport: { width: 1200, height: 800 } });
  setupEditor(test, '?edit=json');

  test('stays on one row at 1200px', async ({ page }) => {
    const toolbar = page.locator('.demo__toolbar');
    await expect(toolbar).toContainText('editing as');
    // One row: every control starts above where the shortest one ends.
    const boxes = await toolbar.evaluate((element) =>
      [...element.children].map((child) => {
        const box = child.getBoundingClientRect();
        return [Math.round(box.top), Math.round(box.bottom)];
      }),
    );
    const lowestTop = Math.max(...boxes.map(([top]) => top));
    const firstBottom = Math.min(...boxes.map(([, bottom]) => bottom));
    expect(lowestTop, `toolbar controls (top, bottom): ${JSON.stringify(boxes)}`).toBeLessThan(firstBottom);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(vanilla(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the textarea, Ctrl+K Ctrl+I, docs links and the footer', () => {
    test.use({ viewport: { width: 1000, height: 600 } });
    const listenSchema = JSON.stringify({
      type: 'object',
      properties: { config: { type: 'object', properties: { listen: { type: 'integer', description: 'Port to listen on.' } } } },
    });
    const values = {
      yaml: ['config:\n  listen: 80\n', '80'],
      json: ['{\n  "config": {\n    "listen": 80\n  }\n}\n', '80'],
      toml: ['[config]\nlisten = 80\n', '80'],
      hcl: ['config {\n  listen = 80\n}\n', '80'],
      xml: ['<config>\n  <listen>80</listen>\n</config>\n', '80'],
    };
    for (const [width, height] of [[260, 300], [320, 568], [390, 260]]) {
      test(`the editor and its footer fit a ${width}×${height} window`, async ({ browser }) => {
        const context = await browser.newContext({ viewport: { width, height }, hasTouch: true, isMobile: true });
        const page = await context.newPage();
        const value = 'port: "eighty"\nhost: example\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';
        const errors = await open(page, { value, schema: JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } }) });
        await page.addStyleTag({ content: 'header { display: none; }' });
        await expect(page.locator('.config-editor__diagnostic-status')).toContainText('1 error');
        // The whole editor fits the window, so its footer can be seen with it.
        const frame = await page.locator('.config-editor').first().boundingBox();
        expect(frame.height).toBeLessThanOrEqual(height);
        const clipped = await page.locator('.config-editor__status').evaluate((status) => {
          const box = status.getBoundingClientRect();
          return [...status.querySelectorAll('*')]
            .filter((element) => element.checkVisibility() && element.getClientRects().length)
            .filter((element) => {
              const inner = element.getBoundingClientRect();
              const scrolls = element.scrollHeight > element.clientHeight + 1 && getComputedStyle(element).overflow !== 'visible';
              return inner.bottom > box.bottom + 0.5 || inner.top < box.top - 0.5 || inner.right > box.right + 0.5 || scrolls;
            })
            .map((element) => element.className);
        });
        expect(clipped).toEqual([]);
        expect(errors).toEqual([]);
        await context.close();
      });
    }
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

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

  const schema = JSON.stringify({
    type: 'object',
    properties: {
      config: {
        type: 'object',
        properties: { old: { type: 'integer', deprecated: true, description: 'Old thing' }, port: { type: 'integer' } },
      },
    },
  });

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the toolbar and the footer fit', () => {
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
          // Counts and the hint to hover them, then an unplaced
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
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  const card = (page) => page.locator('.config-editor__hover');

  const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);

  async function open(page, name, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).first().waitFor();
    return errors;
  }

  // Puts the caret before the `offset`-th character of `target` in the text,
  // as a key press would (the editor notes the caret on keys).
  async function caretIn(page, target, offset) {
    await input(page).evaluate(
      (element, [target, offset]) => {
        const at = element.value.indexOf(target) + offset - 1;
        element.focus();
        element.setSelectionRange(at, at);
      },
      [target, offset],
    );
    await page.keyboard.press('ArrowRight');
  }

  const showDocs = async (page) => {
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(card(page)).toBeVisible();
  };

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('in a right-to-left page', () => {
    const rtl = (page) => page.evaluate(() => document.documentElement.setAttribute('dir', 'rtl'));

    test('the editor, its footer and its popups stay left to right; right-to-left values stay text', async ({ page }) => {
      const errors = await open(page, 'vanilla.html', {
        value: 'name: שלום עולם\ndb:\n  host: x\nport: eighty\n',
        schema: JSON.stringify({
          type: 'object',
          properties: {
            name: { type: 'string', description: 'The name.' },
            db: { type: 'object', properties: { host: { type: 'string' }, user: { type: 'string' } } },
            port: { type: 'integer' },
          },
        }),
      });
      await rtl(page);
      expect(await page.evaluate(() => getComputedStyle(document.body).direction)).toBe('rtl');
      const direction = (selector) => page.locator(selector).first().evaluate((element) => getComputedStyle(element).direction);
      for (const selector of ['.config-editor', '.config-editor__input', '.config-editor__highlight', '.config-editor__gutter', '.config-editor__status']) {
        expect(await direction(selector), selector).toBe('ltr');
      }
      // The gutter on the left, `db:` read in order.
      const gutter = await page.locator('.config-editor__gutter').boundingBox();
      const text = await input(page).boundingBox();
      expect(gutter.x).toBeLessThan(text.x);
      const order = await page.locator('.config-editor__highlight .config-editor__code-line').nth(1).evaluate((line) => {
        const node = document.createTreeWalker(line, NodeFilter.SHOW_TEXT).nextNode();
        const box = (at) => {
          const range = document.createRange();
          range.setStart(node, at);
          range.setEnd(node, at + 1);
          return range.getBoundingClientRect().left;
        };
        return node.textContent.startsWith('db') ? box(0) < box(1) : null;
      });
      expect(order).toBe(true);
      await expect(page.locator('.config-editor__highlight .config-editor__code-line').first()).toContainText('שלום עולם');

      // Each popup: the card, the tip, the find widget and the suggestions.
      await caretIn(page, 'name', 2);
      await showDocs(page);
      expect(await direction('.config-editor__hover')).toBe('ltr');
      await page.keyboard.press('Escape');
      await page.keyboard.press('F8');
      await expect(page.locator('.config-editor__problem-tip')).toBeVisible();
      expect(await direction('.config-editor__problem-tip')).toBe('ltr');
      await page.keyboard.press('Escape');
      await page.keyboard.press('Control+f');
      await expect(page.locator('.config-editor__find')).toBeVisible();
      expect(await direction('.config-editor__find')).toBe('ltr');
      await page.keyboard.press('Escape');
      await caretIn(page, '  host', 3);
      await page.keyboard.press('End');
      await page.keyboard.press('Enter');
      await page.keyboard.press('Control+Space');
      const list = page.locator('.config-editor__completions');
      await expect(list.locator('[role="option"]').first()).toBeVisible();
      expect(await direction('.config-editor__completions')).toBe('ltr');
      // By the caret, near the text's left edge, not across the editor.
      const listBox = await list.boundingBox();
      expect(listBox.x - text.x).toBeLessThan(120);
      expect(errors).toEqual([]);
    });

    test('the documents view keeps its tabs left to right', async ({ page }) => {
      const errors = await open(page, 'documents.html');
      await rtl(page);
      const strip = page.locator('.config-editor-documents');
      expect(await strip.evaluate((element) => getComputedStyle(element).direction)).toBe('ltr');
      const first = await tab(page, 'main').boundingBox();
      const second = await tab(page, 'limits').boundingBox();
      expect(first.x).toBeLessThan(second.x);
      // ArrowRight goes to the tab on the right.
      await tab(page, 'main').focus();
      await page.keyboard.press('ArrowRight');
      await expect(tab(page, 'limits')).toBeFocused();
      expect(errors).toEqual([]);
    });
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const footer = (page) => page.locator('.config-editor__diagnostic-status');

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

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

  test.describe('the footer keeps one row on a phone', () => {
    for (const width of [390, 480]) {
      test(`at ${width} px`, async ({ browser }) => {
        const context = await browser.newContext({ viewport: { width, height: 500 } });
        const page = await context.newPage();
        await open(page, { value: 'listen: 7835\nflag: nope\n', schema: JSON.stringify({ properties: { flag: { type: 'boolean' } } }) });
        await expect(footer(page)).toContainText('1 error');
        const layout = await page.locator('.config-editor__status').evaluate((status) => {
          const box = status.getBoundingClientRect();
          const summary = status.querySelector('.config-editor__diagnostic-status').getBoundingClientRect();
          return { height: Math.round(box.height), leftGap: Math.round(summary.left - box.left) };
        });
        expect(layout.height).toBeLessThanOrEqual(29);
        // No empty half: the summary starts at the footer's left.
        expect(layout.leftGap).toBeLessThanOrEqual(12);
        await context.close();
      });
    }

    test('at 320 px a long problem wraps in its own box, the counts stay on its first row', async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width: 320, height: 500 } });
      const page = await context.newPage();
      await open(page, {
        value: 'listen: 7835\nflag_with_a_rather_long_name: nope\n',
        schema: JSON.stringify({ properties: { flag_with_a_rather_long_name: { type: 'boolean' } } }),
      });
      await expect(footer(page)).toContainText('1 error');
      await input(page).click();
      await page.keyboard.press('End');
      await page.keyboard.type('x');
      await expect(page.locator('.config-editor__status-meta')).toContainText('change');
      const rows = await page.locator('.config-editor__status-meta').evaluate((meta) => {
        const summary = meta.querySelector('.config-editor__diagnostic-status').getBoundingClientRect();
        const lines = meta.querySelector('.config-editor__line-count').getBoundingClientRect();
        return { summaryBottom: summary.bottom, lines: lines.top, right: Math.round(lines.right) };
      });
      // The line count sits beside the summary (it may wrap to two lines),
      // not on a row of its own below it, and inside the window.
      expect(rows.lines).toBeLessThan(rows.summaryBottom);
      expect(rows.right).toBeLessThanOrEqual(320);
      await context.close();
    });
  });
});

// A group of its own (needs the built npm packages, a 800×400 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl('vanilla.html', options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  const portSchema = (description, extra = {}) =>
    JSON.stringify({ type: 'object', properties: { port: { type: 'integer', description, ...extra } } });

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 800, height: 400 } });

  test.describe('the footer hint says tap on a touch screen', () => {
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
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('the read-only notice wraps a long word or URL inside a phone screen', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 700 });
    await open(page);
    const reason = `Managed_by_the_operator_https://example.com/${'very/long/path/'.repeat(8)} and more words after it.`;
    await page.evaluate((reason) => {
      document.querySelectorAll('header, #editor, pre').forEach((element) => element.remove());
      const element = window.mountPoint();
      new window.module.ConfigEditor(element, { value: 'k: 1\n'.repeat(3), readOnly: true, readOnlyReason: reason });
    }, reason);
    await page.waitForTimeout(300);
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.type('x');
    const notice = page.locator('.config-editor__read-only-notice');
    await expect(notice).toBeVisible();
    const box = await notice.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, right: rect.right, overflows: element.scrollWidth > element.clientWidth };
    });
    expect(box.overflows).toBe(false);
    expect(box.left).toBeGreaterThanOrEqual(0);
    expect(box.right).toBeLessThanOrEqual(375);
  });
});
