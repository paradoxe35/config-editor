import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Hover cards: docs and a problem in one card, the pointer reaching the card, long cards, taps.

test.describe('hover docs by a tap', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 740 } });
  setupEditor(test, '?language=yaml');

  test('a tap on a key shows its docs', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    const at = await pointAt(page, 0, 2);
    await page.touchscreen.tap(at.x, at.y);
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // A key with docs and a problem shows one popup, the
  // hover card listing the problems above the docs, by the pointer, the
  // keyboard (`Ctrl+K Ctrl+I`) and a tap, while F8 and a problem without
  // docs keep the problem tip. Through the npm package's vanilla page (run
  // `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

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

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('docs and a problem at one place show in one popup', () => {
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
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // The hover card stays while the pointer travels to it;
  // a host problem saying something else never hides the editor's own; hover
  // docs on values; the card fits the window; a tap outside closes it; the
  // footer keeps one row on a phone; tabs drop their folder when narrow; the
  // "deprecated" badge reads in every theme. Through the npm package's pages
  // (run `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const tip = (page) => page.locator('.config-editor__problem-tip');

  const card = (page) => page.locator('.config-editor__hover');

  const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

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

  // Which popups show now: 'card', 'tip', both or none.
  const shown = (page) =>
    page.evaluate(() =>
      ['hover', 'problem-tip']
        .filter((name) => document.querySelector(`.config-editor__${name}`)?.checkVisibility({ visibilityProperty: true }))
        .join('+'),
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

  const DOCS = {
    yaml: { value: 'old: 1\nport: 2\nz: 3\n', line: 1, column: 1 },
    json: { value: '{\n  "old": 1,\n  "port": 2\n}\n', line: 2, column: 4 },
    toml: { value: 'old = 1\nport = 2\n', line: 1, column: 1 },
    hcl: { value: 'old = 1\nport = 2\n', line: 1, column: 1 },
    xml: { value: '<old>1</old>\n<port>2</port>\n', line: 1, column: 2 },
  };

  const valueSchema = JSON.stringify({
    type: 'object',
    properties: { port: { type: 'integer', description: 'The port' }, n: { type: 'integer', description: 'A number' } },
  });

  // Each language's `port: x` (not an integer) and `n: 3`: (line, column of the value).
  const VALUES = {
    yaml: { value: 'port: x\nn: 3\n', at: [[1, 7], [2, 4]] },
    json: { value: '{\n  "port": "x",\n  "n": 3\n}\n', at: [[2, 12], [3, 8]] },
    toml: { value: 'port = "x"\nn = 3\n', at: [[1, 9], [2, 5]] },
    hcl: { value: 'port = "x"\nn = 3\n', at: [[1, 9], [2, 5]] },
    xml: { value: '<port>x</port>\n<n>3</n>\n', at: [[1, 7], [2, 4]] },
  };

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the card with docs and a problem stays on the way to it', () => {
    test.use({ viewport: { width: 800, height: 400 } });

    for (const [language, doc] of Object.entries(DOCS)) {
      test(`${language}: small steps across the next key reach the card and its link`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema });
        await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
        const key = await charBox(page, doc.line, doc.column + 1);
        await page.mouse.move(key.x, key.y);
        await expect(card(page)).toBeVisible();
        await expect(card(page)).toContainText(/deprecated/i);
        const box = await card(page).boundingBox();
        // Towards the card's lower left, over the next line's key.
        const target = { x: box.x + 30, y: box.y + box.height - 8 };
        const trace = [];
        for (let step = 1; step <= 12; step += 1) {
          await page.mouse.move(key.x + ((target.x - key.x) * step) / 12, key.y + ((target.y - key.y) * step) / 12);
          trace.push(await shown(page));
        }
        // Slower than the hover pause, the way a hand moves.
        await page.waitForTimeout(400);
        trace.push(await shown(page));
        expect(trace).toEqual(Array(13).fill('hover'));
        await expect(card(page)).toContainText('Old thing');
        const link = card(page).locator('.config-editor__hover-link');
        const linkBox = await link.boundingBox();
        const popup = page.waitForEvent('popup');
        await page.mouse.move(linkBox.x + 5, linkBox.y + linkBox.height / 2, { steps: 4 });
        await page.mouse.click(linkBox.x + 5, linkBox.y + linkBox.height / 2);
        expect((await popup).url()).toBe('https://example.com/old');
        expect(errors).toEqual([]);
      });

      test(`${language}: leaving the card's key elsewhere closes it without the problem tip for that place`, async ({ page }) => {
        await open(page, { value: doc.value, language, schema });
        const key = await charBox(page, doc.line, doc.column + 1);
        await page.mouse.move(key.x, key.y);
        await expect(card(page)).toBeVisible();
        // Sample every frame while the pointer goes up and away.
        const sampled = page.evaluate(
          () =>
            new Promise((resolve) => {
              const seen = new Set();
              const end = performance.now() + 700;
              const look = () => {
                const tip = document.querySelector('.config-editor__problem-tip');
                if (tip?.checkVisibility({ visibilityProperty: true })) seen.add('tip');
                if (performance.now() < end) requestAnimationFrame(look);
                else resolve([...seen]);
              };
              look();
            }),
        );
        await page.mouse.move(key.x + 400, 2, { steps: 5 });
        expect(await sampled).toEqual([]);
        await expect(card(page)).toBeHidden();
      });
    }
  });

  test.describe('the pointer on a value shows its property’s docs', () => {
    for (const [language, doc] of Object.entries(VALUES)) {
      test(`${language}: with its problem, and without one`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema: valueSchema });
        await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
        const [[line, column], [plainLine, plainColumn]] = doc.at;
        const value = await charBox(page, line, column);
        await page.mouse.move(value.x, value.y);
        await expect(card(page)).toBeVisible();
        await expect(card(page)).toContainText('The port');
        await expect(card(page).locator('.config-editor__problem-tip-entry')).toHaveCount(1);
        await expect(tip(page)).toBeHidden();
        await page.mouse.move(790, 390);
        await expect(card(page)).toBeHidden();
        const plain = await charBox(page, plainLine, plainColumn);
        await page.mouse.move(plain.x, plain.y);
        await expect(card(page)).toContainText('A number');
        expect(errors).toEqual([]);
      });
    }
  });

  test.describe('a card with many problems fits the window and scrolls', () => {
    test.use({ viewport: { width: 800, height: 400 } });

    for (const line of [1, 12]) {
      test(`on line ${line}`, async ({ page }) => {
        const value = Array.from({ length: 14 }, (_, i) => `k${i}: 1`).join('\n') + '\n';
        const keySchema = JSON.stringify({
          type: 'object',
          additionalProperties: { type: 'integer', description: 'A key', 'x-docs-url': 'https://example.com/k' },
        });
        await open(page, { value, schema: keySchema });
        await page.addStyleTag({ content: 'header { display: none }' });
        await setHost(
          page,
          Array.from({ length: 25 }, (_, i) => ({
            range: { startLine: line, startColumn: 1, endColumn: 3 },
            severity: 'warning',
            message: `problem number ${i} with a message long enough to take a line or two of its own`,
          })),
        );
        const key = await charBox(page, line, 1);
        await page.mouse.move(key.x, key.y);
        await expect(card(page)).toBeVisible();
        await expect(card(page)).toHaveClass(/config-editor__hover--scrolls/);
        const fits = await card(page).evaluate((element) => {
          const box = element.getBoundingClientRect();
          return { top: box.top >= 0, bottom: box.bottom <= innerHeight, scrolls: element.scrollHeight > element.clientHeight };
        });
        expect(fits).toEqual({ top: true, bottom: true, scrolls: true });
        // The pointer reaches it and the wheel scrolls it.
        const box = await card(page).boundingBox();
        await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 6 });
        await page.mouse.wheel(0, 200);
        await expect.poll(() => card(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
      });
    }
  });

  test.describe('a tap outside the editor closes the card', () => {
    test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });

    for (const [language, doc] of Object.entries(DOCS)) {
      test(`${language}`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema });
        const key = await charBox(page, doc.line, doc.column + 1);
        await page.touchscreen.tap(key.x, key.y);
        await expect(card(page)).toBeVisible();
        const below = await page.locator('.config-editor').evaluate((editor) => editor.getBoundingClientRect().bottom + 30);
        await page.touchscreen.tap(200, Math.min(below, 690));
        await expect(card(page)).toBeHidden();
        await expect(tip(page)).toBeHidden();
        expect(errors).toEqual([]);
      });
    }
  });
});

// A group of its own (needs the built npm packages, a 800×400 window).
test.describe(() => {
  // A long hover card scrolls with the wheel and the
  // keyboard whether or not it has a link; the pointer resting in the gap
  // between the line and the card keeps it. Through the npm package's pages
  // (run `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

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
        return { x: box.left + box.width / 2, y: box.top + box.height / 2, top: box.top, bottom: box.bottom };
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

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 800, height: 400 } });

  test.describe('a long card without a link stays under the pointer and scrolls', () => {
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

  test.describe('the pointer resting between the line and the card keeps it', () => {
    for (const [language, value] of Object.entries(DOCS)) {
      for (const link of [true, false]) {
        test(`${language}: ${link ? 'with a link' : 'long, no link'}`, async ({ page }) => {
          const extra = link ? { 'x-docs-url': 'https://example.com/port' } : {};
          const errors = await open(page, { value, language, schema: portSchema(link ? 'The port.' : LONG, extra) });
          const key = await charBox(page, 3, portColumn(value) + 1);
          await page.mouse.move(key.x, key.y);
          await expect(card(page)).toBeVisible();
          const box = await card(page).boundingBox();
          // Below the line, or above it when it fits there.
          const below = box.y > key.bottom;
          if (!below) expect(box.y + box.height).toBeLessThan(key.top);
          // In the gap between the line and the card.
          const gap = below ? (key.bottom + box.y) / 2 + 0.5 : (box.y + box.height + key.top) / 2 - 0.5;
          await page.mouse.move(key.x, gap, { steps: 3 });
          await page.waitForTimeout(1200);
          expect(await shown(page)).toBe('hover');
          // Then on into it, and it stays.
          await page.mouse.move(key.x, below ? box.y + 10 : box.y + box.height - 10, { steps: 2 });
          await page.waitForTimeout(600);
          expect(await shown(page)).toBe('hover');
          expect(errors).toEqual([]);
        });
      }
    }
  });
});
