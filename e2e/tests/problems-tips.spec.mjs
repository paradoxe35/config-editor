import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointerOn, setupEditor } from './helpers.mjs';

// Problem tips and underlines: F8, the underline cap, docs links, host problems, tips that take focus.

async function select(page, text, from, to) {
  const e = editor(page);
  await e.paste(text);
  await e.input.evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [from, to],
  );
  return e;
}

async function at(page, marked) {
  const position = marked.indexOf('¦');
  return select(page, marked.replace('¦', ''), position, position);
}

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const markers = (page) => page.locator('.config-editor__diagnostic-marker');

const footer = (page) => page.locator('.config-editor__diagnostic-status');

const input = (page) => page.locator('.config-editor__input');

const tip = (page) => page.locator('.config-editor__problem-tip');

const announcement = (page) => page.locator('.config-editor__announcement');

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

const lines = (count) => Array.from({ length: count }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

// "line:column" of the caret, and whether the editor has the focus.
const caret = (page) =>
  input(page).evaluate((element) => {
    const before = element.value.slice(0, element.selectionStart).split('\n');
    return `${before.length}:${before.at(-1).length + 1}${document.activeElement === element ? '' : ' (unfocused)'}`;
  });

const keyLines = (count) => Array.from({ length: count }, (_, index) => `key_number_${index}: value ${index}`).join('\n') + '\n';

const WITH_DOCS = [
  { range: { startLine: 6, startColumn: 3, endColumn: 9 }, severity: 'error', message: 'one with docs', docsUrl: 'https://example.com/d', source: 'srv' },
];

test.describe('the footer counts warnings apart from errors', () => {
  test('a value the XML view cannot show is a warning, not an error', async ({ page, baseURL }) => {
    await page.addInitScript(() => {
      // `self.allow: []` meets the schema; XML writes no empty list. (A
      // root `allow` would be unknown to the schema, and the view now shows
      // the host's own problems too.)
      window.configEditorSample = 'listen: 1\nself:\n  allow: []\n';
    });
    await page.goto(`${baseURL}?language=yaml&edit=xml&sample=page`);
    await page.locator('.config-editor__input').waitFor();
    await expect(page.locator('.config-editor__diagnostic-marker-warning')).toHaveCount(1);
    await expect(page.locator('.config-editor__diagnostic-status')).toContainText('1 warning');
    await expect(page.locator('.config-editor__diagnostic-status')).not.toContainText('error');
  });
});

test.describe('tooltips on a phone', () => {
  test.use({ viewport: { width: 390, height: 800 } });
  setupEditor(test, '?language=yaml');

  test('a long message stays inside the editor', async ({ page }) => {
    const e = editor(page);
    await e.paste('heartbeat: {interval_secs: 99999}\n');
    const marker = page.locator('.config-editor__diagnostic-marker');
    await expect(marker).toHaveCount(1);
    await pointerOn(page, marker);
    // The message shows in the problem tip, or in the card of the documented
    // key it sits on (here inside a one-line map): either stays inside.
    const tip = page.locator('.config-editor__problem-tip:visible, .config-editor__hover:visible').first();
    await expect(tip).toContainText('99999');
    const overflow = await tip.evaluate(
      (element) => element.getBoundingClientRect().right - document.querySelector('.config-editor__input').getBoundingClientRect().right,
    );
    expect(overflow).toBeLessThanOrEqual(0);
  });
});

test.describe('F8', () => {
  setupEditor(test, '?language=yaml');

  test('F8 shows the message of the problem it moves to, until the next key', async ({ page }) => {
    const e = await at(page, '¦listen: 1\nbogus: 2\n');
    const marker = page.locator('.config-editor__diagnostic-marker').first();
    await expect(marker).toBeVisible();
    const tip = page.locator('.config-editor__problem-tip');
    await expect(tip).toBeHidden();
    await page.keyboard.press('F8');
    await expect(tip).toBeVisible();
    await expect(tip).toContainText('bogus');
    expect((await e.caret()).line).toBe(2);
    await page.keyboard.press('ArrowRight');
    await expect(tip).toBeHidden();
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  async function at(page, marked) {
    const e = editor(page);
    const first = marked.indexOf('¦');
    const last = marked.lastIndexOf('¦');
    const [start, end] = first === last ? [first, first] : [first, last - 1];
    await e.paste(marked.replaceAll('¦', ''));
    await e.input.evaluate(
      (element, [from, to]) => {
        element.focus();
        element.setSelectionRange(from, to);
      },
      [start, end],
    );
    return e;
  }


  test.describe('F8 right after an edit', () => {
    setupEditor(test, '?language=yaml');

    test('jumps to the problem just typed, without waiting for the pause', async ({ page }) => {
      const e = await at(page, 'listen: 1\n¦');
      await page.keyboard.type('bogus: 2');
      await page.keyboard.press('Home');
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      // Straight away: no wait for the typing pause.
      expect((await e.caret()).line).toBe(2);
    });
  });
});

test.describe('yaml', () => {
  setupEditor(test, '?language=yaml');

  test('a cut problem list says 20+ and does not count its note', async ({ page }) => {
    const e = editor(page);
    await e.paste(Array.from({ length: 30 }, (_, index) => `unknown_${index}: 1`).join('\n') + '\n');
    await expect(page.locator('.config-editor__status')).toContainText('20+ errors', { timeout: 5000 });
    await expect(page.locator('.config-editor__status')).not.toContainText('warning');
  });
});

// Multi-line host
// ranges counted line by line in the underline cap, several problems at one
// place each with its own source, code and docs link (the underline in the
// most serious one's colour), a tall problem tip kept inside the editor and
// scrollable, every problem at a place listed past the cap, and the hover
// docs reached from the keyboard (`Ctrl+K Ctrl+I`) and by a tap.
// Host problems through the npm package's vanilla page (run `npm run build`
// in js/ first).
test.describe('problems at scale and at one place', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('ranges over hundreds of lines draw at most 200 underlines in all; every problem is counted and walked', async ({ page }) => {
    const errors = await open(page, { value: lines(3000), schema: '{}' });
    const ranges = Array.from({ length: 20 }, (_, index) => ({
      range: { startLine: 1 + index * 7, startColumn: 1, endLine: 201 + index * 7, endColumn: 2 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await setHost(page, ranges);
    await expect(footer(page)).toHaveText(/^20 warnings/);
    // 20 × 201 lines would be 4020 underlines.
    expect(await markers(page).count()).toBeLessThanOrEqual(200);
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText(' of 20');
    expect(errors).toEqual([]);
  });

  test('several problems at one place each show their own source, code and docs link, the error first and in its colour', async ({ page }) => {
    // No docs for `listen`: the tip shows its problems, not a card.
    const errors = await open(page, { schema: '{}' });
    await setHost(page, [
      { path: '/listen', severity: 'warning', message: 'deprecated port', source: 'lint', code: 'W7' },
      { path: '/listen', severity: 'error', message: 'port in use', source: 'burrow', code: 'E_PORT', docsUrl: 'https://example.com/ports' },
      { path: '/listen', severity: 'hint', message: 'consider 443' },
      { path: '/listen', severity: 'hint', message: 'consider 8443' },
    ]);
    await expect(markers(page)).toHaveCount(4);
    // Only the error's underline is drawn in colour over the same text.
    await expect(page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)')).toHaveCount(1);
    await expect(page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)')).toHaveAttribute(
      'data-message',
      'port in use',
    );
    await pointerOn(page, page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)'));
    const entries = tip(page).locator('.config-editor__problem-tip-entry');
    await expect(entries).toHaveCount(4);
    await expect(entries.nth(0)).toHaveClass(/--error/);
    await expect(entries.nth(0).locator('.config-editor__problem-tip-message')).toHaveText('Error port in use');
    await expect(entries.nth(0).locator('.config-editor__problem-tip-detail')).toHaveText('burrow · E_PORT · Open docs ↗ (Alt+Enter)');
    await expect(entries.nth(0).locator('a')).toHaveAttribute('href', 'https://example.com/ports');
    await expect(entries.nth(1).locator('.config-editor__problem-tip-message')).toHaveText('Warning deprecated port');
    await expect(entries.nth(1).locator('.config-editor__problem-tip-detail')).toHaveText('lint · W7');
    await expect(entries.nth(1).locator('a')).toHaveCount(0);
    // Each default-source problem says its own source, under its own message.
    for (const index of [2, 3]) {
      await expect(entries.nth(index)).toHaveClass(/--hint/);
      await expect(entries.nth(index).locator('.config-editor__problem-tip-detail')).toHaveText('host');
    }
    expect(errors).toEqual([]);
  });

  test('a tall tip near the bottom stays inside the editor, scrolls, and F8 scrolls to the problem it went to', async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 600 });
    const errors = await open(page, { value: lines(60), schema: '{}' });
    const box = await input(page).boundingBox();
    const lineHeight = await input(page).evaluate((element) => parseFloat(getComputedStyle(element).lineHeight));
    // The last fully visible line.
    const last = Math.floor((box.height - 16) / lineHeight) - 1;
    const long = 'a long message that wraps over a few lines of the tip '.repeat(3);
    await setHost(page, [
      ...Array.from({ length: 11 }, (_, index) => ({
        range: { startLine: last + 1, startColumn: 1, endColumn: 3 },
        severity: 'warning',
        message: `warning ${index} ${long}`,
      })),
      { range: { startLine: last + 1, startColumn: 1, endColumn: 3 }, severity: 'error', message: 'the error' },
    ]);
    await expect(markers(page)).toHaveCount(12);
    await pointerOn(page, page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)'));
    await expect(tip(page)).toBeVisible();
    const placed = await tip(page).evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const area = document.querySelector('.config-editor__input').getBoundingClientRect();
      return {
        top: rect.top,
        bottom: rect.bottom,
        areaTop: area.top,
        areaBottom: area.bottom,
        scrolls: element.scrollHeight > element.clientHeight,
        first: element.querySelector('.config-editor__problem-tip-message').textContent,
      };
    });
    expect(placed.top).toBeGreaterThanOrEqual(placed.areaTop);
    expect(placed.bottom).toBeLessThanOrEqual(placed.areaBottom);
    expect(placed.scrolls).toBe(true);
    // The most serious first, at the top, in view.
    expect(placed.first).toBe('Error the error');
    // F8 to the last one at that place: its entry is scrolled into view.
    await input(page).click();
    await page.keyboard.press('Control+Home');
    for (let press = 0; press < 12; press += 1) await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText('12 of 12');
    const current = tip(page).locator('.config-editor__problem-tip-entry--current');
    await expect(current).toHaveCount(1);
    await expect
      .poll(() =>
        current.evaluate((entry) => {
          const own = entry.getBoundingClientRect();
          const box = entry.parentElement.getBoundingClientRect();
          return own.top >= box.top - 1 && own.top < box.bottom;
        }),
      )
      .toBe(true);
    expect(errors).toEqual([]);
  });

  test('U19 low: hovering a place lists every problem there, those past the underline cap included', async ({ page }) => {
    const errors = await open(page, { value: lines(300), schema: '{}' });
    const list = Array.from({ length: 210 }, (_, index) => ({
      range: { startLine: index === 0 || index > 200 ? 1 : 1 + index, startColumn: 1, endColumn: 3 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await setHost(page, list);
    await expect(footer(page)).toHaveText(/^210 warnings/);
    await pointerOn(page, markers(page).first());
    await expect(tip(page).locator('.config-editor__problem-tip-entry')).toHaveCount(10);
    expect(errors).toEqual([]);
  });
});

test.describe('host problems', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('host hints on fields hide none of their schema errors, past the cap included', async ({ page }) => {
    const value = Array.from({ length: 30 }, (_, index) => `k${index}: x${index}`).join('\n') + '\n';
    const schema = JSON.stringify({ type: 'object', additionalProperties: { type: 'integer' } });
    const errors = await open(page, { value, schema });
    await expect(footer(page)).toHaveText(/^20\+ errors/);
    await setHost(page, Array.from({ length: 20 }, (_, index) => ({ path: `/k${index}`, severity: 'hint', message: `h${index}` })));
    await expect(footer(page)).toHaveText(/^20\+ errors, 20 hints/);
    expect(await page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-hint)').count()).toBe(20);
    expect(errors).toEqual([]);
  });

  test('a range with its end left out is marked on an empty line and at a line end', async ({ page }) => {
    const errors = await open(page);
    // Line 5 is the empty line after the last line break; line 2 is 12 characters.
    const thrown = await page.evaluate(() => {
      try {
        window.editor.update({
          hostDiagnostics: [
            { range: { startLine: 5, startColumn: 1 }, message: 'on the empty line' },
            { range: { startLine: 2, startColumn: 13 }, message: 'at the line end' },
          ],
        });
        return null;
      } catch (error) {
        return String(error);
      }
    });
    expect(thrown).toBeNull();
    await expect(footer(page)).toHaveText(/^2 errors/);
    expect(errors).toEqual([]);
  });

  test('a closed fold hiding a warning shows it on its chip', async ({ page }) => {
    const errors = await open(page, { value: 'web:\n  http: 80\n  hosts: []\nlisten: 1\n', schema: '{}' });
    await setHost(page, [{ range: { startLine: 3, startColumn: 3, endColumn: 8 }, severity: 'warning', message: 'hidden' }]);
    await page.evaluate(() => window.editor.foldAll());
    await expect(page.locator('.config-editor__fold-chip')).toHaveClass(/config-editor__fold-chip--warning/);
    expect(errors).toEqual([]);
  });
});

// A click or a tap on the underline of a problem with a
// docs link moves the caret there and shows its tip (the tip, not yet
// measured, covered the underline and took the click), and the link in the
// tip still opens; the underline cap draws the most serious problems first
// and the footer says when it left some out; a tip in an editor too short
// for it takes the window's room instead of scrolling; `Ctrl+K` and an
// unknown key types nothing; `Ctrl+K Ctrl+I` says each part once with one
// full stop, and finds the XML element with the caret on its `<`; a closed
// fold's chip names the most serious problem it hides. Through the npm
// package's vanilla page (run `npm run build` in js/ first).
test.describe('an underline with a docs link', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const mode of ['mouse', 'touch']) {
    test.describe(`by ${mode}`, () => {
      if (mode === 'touch') test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });
      else test.use({ viewport: { width: 1000, height: 600 } });

      test('a click or tap moves the caret and shows the tip; its link opens the docs', async ({ page, context }) => {
        const errors = await open(page, { value: keyLines(30), schema: '{}' });
        const opened = [];
        context.on('page', (popup) => opened.push(popup.url()));
        await page.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
        await setHost(page, WITH_DOCS);
        await expect(markers(page)).toHaveCount(1);
        const box = await markers(page).first().boundingBox();
        // On the left of the first character: the caret lands before it.
        const x = box.x + 2;
        const y = box.y + box.height / 2;
        if (mode === 'touch') await page.touchscreen.tap(x, y);
        else await page.mouse.click(x, y);
        await expect.poll(() => caret(page)).toBe('6:3');
        await expect(tip(page)).toBeVisible();
        await expect(tip(page)).toContainText('one with docs');
        expect(opened).toEqual([]);
        // The tip sits beside the line, never over it.
        const placed = await tip(page).boundingBox();
        expect(placed.y + placed.height <= box.y + 1 || placed.y >= box.y + box.height - 1).toBe(true);
        // The link inside the tip still opens the docs.
        const link = page.locator('.config-editor__problem-tip-link');
        if (mode === 'touch') {
          const at = await link.boundingBox();
          await page.touchscreen.tap(at.x + at.width / 2, at.y + at.height / 2);
        } else {
          await page.mouse.move(x, y);
          await link.click();
        }
        await expect.poll(() => opened).toEqual(['https://example.com/d']);
        expect(errors).toEqual([]);
      });
    });
  }
});

test.describe('the underline cap', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 600 } });

  const LINES = 400;
  const cases = {
    'an info over the whole file': [
      { range: { startLine: 1, startColumn: 1, endLine: LINES, endColumn: 1 }, severity: 'info', message: 'file is generated' },
      { range: { startLine: 10, startColumn: 1, endColumn: 4 }, severity: 'error', message: 'REAL ERROR' },
    ],
    '250 hints listed first': [
      ...Array.from({ length: 250 }, (_, index) => ({ range: { startLine: 30 + index, startColumn: 1, endColumn: 3 }, severity: 'hint', message: `hint ${index}` })),
      { range: { startLine: 10, startColumn: 1, endColumn: 4 }, severity: 'error', message: 'REAL ERROR' },
    ],
  };
  for (const [name, problems] of Object.entries(cases)) {
    test(`${name} leaves the error underlined and the footer says underlines were left out`, async ({ page }) => {
      const value = Array.from({ length: LINES }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';
      const errors = await open(page, { value, schema: '{}' });
      await setHost(page, problems);
      const error = page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-info):not(.config-editor__diagnostic-marker-hint)');
      await expect(error).toHaveCount(1);
      await expect(error).toHaveAttribute('data-message', 'REAL ERROR');
      await expect(markers(page)).toHaveCount(200);
      await expect(footer(page)).toContainText('only 200 underlines are drawn, the most serious first');
      // Fewer problems: every one drawn, and the note goes.
      await setHost(page, problems.slice(-1));
      await expect(footer(page)).not.toContainText('underlines are drawn');
      expect(errors).toEqual([]);
    });
  }
});

test.describe('the problem tip in a short editor', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 700, height: 260 } });

  test('shows one message whole, beside its line, without scrolling', async ({ page }) => {
    const errors = await open(page, { value: keyLines(60), schema: '{}' });
    // The middle line in view.
    const line = await page.evaluate(() => {
      const box = document.querySelector('.config-editor__input').getBoundingClientRect();
      const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
      const shown = rows.map((row, index) => [row.getBoundingClientRect(), index + 1]).filter(([rect]) => rect.top >= box.top - 1 && rect.bottom <= Math.min(box.bottom, innerHeight) + 1);
      return shown[Math.floor(shown.length / 2)][1];
    });
    await setHost(page, [
      { range: { startLine: line, startColumn: 3, endColumn: 9 }, severity: 'error', message: 'The value here is not allowed by the server because it overlaps another listener', code: 'E0', source: 'burrow' },
    ]);
    const box = await markers(page).first().boundingBox();
    await page.mouse.move(box.x + 6, box.y + box.height / 2);
    await expect(tip(page)).toBeVisible();
    await expect
      .poll(() => tip(page).evaluate((element) => element.scrollHeight <= element.clientHeight + 1))
      .toBe(true);
    const placed = await tip(page).boundingBox();
    expect(placed.y).toBeGreaterThanOrEqual(0);
    expect(placed.y + placed.height).toBeLessThanOrEqual(260);
    expect(placed.y + placed.height <= box.y + 1 || placed.y >= box.y + box.height - 1).toBe(true);
    expect(errors).toEqual([]);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // A click, double-click, drag or tap on underlined text
  // reaches the text (the underline jumped the caret to the problem's start,
  // and a host's info over the whole file made selecting with the mouse
  // impossible), while hovering or tapping it still shows the tip, which
  // lists every problem covering the place (nested ones too); under forced
  // colors (Windows high contrast) find matches, the active suggestion, the
  // gutter's lock and other marks stay visible; the textarea is a multi-line
  // text box without a combobox's `aria-expanded`; `Ctrl+K Ctrl+I` on a
  // value shows its field's docs; a schema's `x-docs-url` links its
  // problems to the docs; the footer fits a narrow editor. Through the npm
  // package's vanilla page (run `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const tip = (page) => page.locator('.config-editor__problem-tip');

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

  // "line:column" of the caret.
  const caret = (page) =>
    input(page).evaluate((element) => {
      const before = element.value.slice(0, element.selectionStart).split('\n');
      return `${before.length}:${before.at(-1).length + 1}`;
    });

  const selected = (page) => input(page).evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));

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
        return { x: box.left, y: box.top + box.height / 2, width: box.width };
      },
      [line, column],
    );

  // Each language's document: `listen` on line `line` from column `column`.
  const DOCS = {
    yaml: { value: 'config:\n  listen: 7835\n  host: example\n', line: 2, column: 3 },
    json: { value: '{\n  "listen": 7835,\n  "host": "example"\n}\n', line: 2, column: 4 },
    toml: { value: '[config]\nlisten = 7835\nhost = "example"\n', line: 2, column: 1 },
    hcl: { value: 'config {\n  listen = 7835\n  host = "example"\n}\n', line: 2, column: 3 },
    xml: { value: '<config>\n  <listen>7835</listen>\n  <host>example</host>\n</config>\n', line: 2, column: 4 },
  };

  // A host's info over the whole file, an error on `listen`.
  const problems = ({ value, line, column }) => [
    { range: { startLine: 1, startColumn: 1, endLine: value.split('\n').length, endColumn: 1 }, severity: 'info', message: 'This file is generated' },
    { range: { startLine: line, startColumn: column, endColumn: column + 6 }, severity: 'error', message: 'listen is taken' },
  ];

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the pointer on underlined text reaches the text', () => {
    test.use({ viewport: { width: 1000, height: 600 } });

    for (const [language, doc] of Object.entries(DOCS)) {
      test(`${language}: a click places the caret where it lands, double-click and drag select, hover lists every problem there`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema: '{}' });
        await setHost(page, problems(doc));
        await expect(page.locator('.config-editor__diagnostic-marker[data-message="listen is taken"]')).toHaveCount(1);
        // Inside `listen`, on the left of its fourth letter: the caret lands
        // before it, not at the problem's start.
        const fourth = await charBox(page, doc.line, doc.column + 3);
        await page.mouse.click(fourth.x + 1, fourth.y);
        await expect.poll(() => caret(page)).toBe(`${doc.line}:${doc.column + 3}`);
        // The tip is up with both problems covering the place, the error first.
        await expect(tip(page)).toBeVisible();
        await expect(page.locator('.config-editor__problem-tip-entry')).toHaveText([/listen is taken/, /This file is generated/]);
        // Inside the info only: its message alone.
        const other = await charBox(page, doc.line + 1, doc.column + 1);
        await page.mouse.move(other.x + 1, other.y);
        await expect(page.locator('.config-editor__problem-tip-entry')).toHaveText([/This file is generated/]);
        // A double-click selects the word.
        await page.mouse.dblclick(fourth.x + 1, fourth.y);
        await expect.poll(() => selected(page)).toBe('listen');
        // A drag selects from where it starts to where it ends.
        const from = await charBox(page, 1, 1);
        await page.mouse.move(from.x + 1, from.y);
        await page.mouse.down();
        await page.mouse.move(fourth.x + 1, fourth.y, { steps: 5 });
        await page.mouse.up();
        const lines = doc.value.split('\n');
        await expect.poll(() => selected(page)).toBe(`${lines[0]}\n${lines[1].slice(0, doc.column + 2)}`);
        expect(errors).toEqual([]);
      });
    }

    test.describe('by touch', () => {
      test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });

      for (const [language, doc] of Object.entries(DOCS)) {
        test(`${language}: a tap places the caret where it lands and shows the problems there`, async ({ page }) => {
          const errors = await open(page, { value: doc.value, language, schema: '{}' });
          await setHost(page, problems(doc));
          await expect(page.locator('.config-editor__diagnostic-marker[data-message="listen is taken"]')).toHaveCount(1);
          const fourth = await charBox(page, doc.line, doc.column + 3);
          await page.touchscreen.tap(fourth.x + 1, fourth.y);
          await expect.poll(() => caret(page)).toBe(`${doc.line}:${doc.column + 3}`);
          await expect(tip(page)).toBeVisible();
          await expect(page.locator('.config-editor__problem-tip-entry')).toHaveText([/listen is taken/, /This file is generated/]);
          expect(errors).toEqual([]);
        });
      }
    });
  });

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
    test("a schema's x-docs-url links its problems to the docs, by the tip and Alt+Enter", async ({ page, context }) => {
      const schema = JSON.stringify({
        type: 'object',
        properties: { port: { type: 'integer', 'x-docs-url': 'https://example.com/port' } },
      });
      const opened = [];
      context.on('page', (popup) => opened.push(popup.url()));
      await page.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
      const errors = await open(page, { value: 'port: eighty\n', schema });
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
      await input(page).click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      await expect(page.locator('.config-editor__problem-tip-link')).toHaveAttribute('href', 'https://example.com/port');
      await page.keyboard.press('Alt+Enter');
      await expect.poll(() => opened).toEqual(['https://example.com/port']);
      expect(errors).toEqual([]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  async function open(page) {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.module = await import('../config-editor/index.js');
      window.tools = await import('../config-editor/tools.js');
    });
  }


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('a host problem at a list item holding a block is marked on its line, and reveal goes there', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const host = document.createElement('div');
        host.style.height = '300px';
        document.body.prepend(host);
        const value = 'l:\n  - n: 1\n  - n: 2\n';
        const editor = new window.module.ConfigEditor(host, {
          value,
          language: 'yaml',
          hostDiagnostics: [{ path: '/l/1', severity: 'error', message: 'second item' }],
        });
        await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50)));
        // The line of the gutter row level with each host marker.
        const rows = [...host.querySelectorAll('.config-editor__gutter-content > div[data-line]')];
        const marked = [...host.querySelectorAll('.config-editor__diagnostic-marker-host')].map((marker) => {
          const top = marker.getBoundingClientRect().top;
          const distance = (row) => Math.abs(row.getBoundingClientRect().top - top);
          return rows.reduce((best, row) => (distance(row) < distance(best) ? row : best)).dataset.line;
        });
        const revealed = editor.reveal('/l/1');
        const input = host.querySelector('.config-editor__input');
        const caretLine = input.value.slice(0, input.selectionStart).split('\n').length;
        editor.destroy();
        host.remove();
        return { marked, revealed, caretLine };
      });
      expect(result).toEqual({ marked: ['3'], revealed: true, caretLine: 3 });
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

  const announcement = (page) => page.locator('.config-editor__announcement');

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

  // Each language's document: `old` (deprecated, with docs and a docs link)
  // on `line`, from `column`; `port` (documented too) on the next line.
  const schema = JSON.stringify({
    type: 'object',
    properties: {
      old: { type: 'integer', deprecated: true, description: 'Old thing', 'x-docs-url': 'https://example.com/old' },
      port: { type: 'integer', description: 'The port', 'x-docs-url': 'https://example.com/port' },
    },
  });

  // Each language's `c: 1`, below the schema's minimum, on `line`.
  const MINIMUM = {
    yaml: { value: 'c: 1\n', line: 1 },
    json: { value: '{\n  "c": 1\n}\n', line: 2 },
    toml: { value: 'c = 1\n', line: 1 },
    hcl: { value: 'c = 1\n', line: 1 },
    xml: { value: '<c>1</c>\n', line: 1 },
  };

  const minimumSchema = JSON.stringify({ type: 'object', properties: { c: { type: 'integer', minimum: 5 } } });

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('a host problem hides the editor’s own only when it says the same', () => {
    for (const [language, doc] of Object.entries(MINIMUM)) {
      test(`${language}: a whole-line host error with another message shows beside it`, async ({ page }) => {
        const errors = await open(page, { value: doc.value, language, schema: minimumSchema });
        const own = page.locator('.config-editor__diagnostic-marker');
        await expect(own).toHaveCount(1);
        const message = await own.getAttribute('data-message');
        const length = doc.value.split('\n')[doc.line - 1].length;
        const whole = { startLine: doc.line, startColumn: 1, endLine: doc.line, endColumn: length + 1 };
        await setHost(page, [{ range: whole, severity: 'error', message: 'H line' }]);
        await expect(footer(page)).toHaveText(/^2 errors/);
        // F8 reaches both, and so do screen readers.
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        const said = [];
        for (let press = 0; press < 2; press += 1) {
          await page.keyboard.press('F8');
          await expect(announcement(page)).toContainText(' of 2');
          said.push(await announcement(page).textContent());
        }
        expect(said.join('\n')).toContain('H line');
        expect(said.join('\n')).toContain('minimum');
        // The same message there: once, the host's.
        await setHost(page, [{ range: whole, severity: 'error', message: message.replace(/^\/c: /, '') }]);
        await expect(footer(page)).toHaveText(/^1 error/);
        expect(errors).toEqual([]);
      });
    }
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  /** Waits `count` frames. */
  const frames = (page, count = 3) =>
    page.evaluate(
      (count) =>
        new Promise((done) => {
          const step = (left) => (left ? requestAnimationFrame(() => step(left - 1)) : done());
          step(count);
        }),
      count,
    );

  const chord = async (page) => {
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
  };

  /** Records every scroll outside the editor, and each frame whose scroll positions outside it differ from now. */
  const watchHost = (page) =>
    page.evaluate(() => {
      const outside = (node) => !(node instanceof Element && node.closest('.config-editor, .config-editor-documents'));
      const boxes = () =>
        [document.scrollingElement, ...document.querySelectorAll('[data-host-box]')].map((box) => `${box.scrollTop},${box.scrollLeft}`).join(' ');
      window.__moves = [];
      window.__still = boxes();
      if (!window.__watching) {
        window.__watching = true;
        addEventListener('scroll', (event) => outside(event.target) && window.__moves.push(`scroll on ${event.target.nodeName}`), true);
        const frame = () => {
          const now = boxes();
          if (now !== window.__still) window.__moves.push(`frame at ${now}`);
          requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      }
    });

  const hostMoves = (page) => page.evaluate(() => window.__moves);

  /** Presses each of `keys` and expects nothing outside the editor to have moved. */
  async function pressStill(page, keys) {
    for (const key of keys) {
      await watchHost(page);
      await page.keyboard.press(key);
      await frames(page);
      expect(await hostMoves(page), key).toEqual([]);
    }
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('the problems tip takes the focus as the docs card does', () => {
    const many = Array.from({ length: 25 }, (_, i) => ({
      path: '/z',
      severity: 'warning',
      message: `host problem ${i} ${'words '.repeat(12)}`,
      ...(i === 0 ? { docsUrl: 'https://example.com/z' } : {}),
    }));
    for (const [language, value, down] of [
      ['yaml', 'a: 1\nz: 1\n', 1],
      ['json', '{\n  "a": 1,\n  "z": 1\n}\n', 2],
    ]) {
      for (const how of ['Ctrl+K Ctrl+I', 'F8']) {
        test(`${language}, shown by ${how}: a second Ctrl+K Ctrl+I focuses it; keys scroll it, Enter opens its docs, Escape returns`, async ({ page, context }) => {
          const errors = await open(page, { language, value, schema: '{}' });
          await context.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
          await page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), many);
          const tip = page.locator('.config-editor__problem-tip');
          await input(page).focus();
          await page.keyboard.press('Control+Home');
          if (how === 'F8') {
            await page.keyboard.press('F8');
          } else {
            for (let i = 0; i < down; i += 1) await page.keyboard.press('ArrowDown');
            await page.keyboard.press('End');
            await chord(page);
          }
          await expect(tip).toBeVisible();
          await expect(tip).toHaveAttribute('role', 'tooltip');
          await chord(page);
          await expect(tip).toBeFocused();
          await expect(tip).toHaveAttribute('role', 'dialog');
          expect((await tip.getAttribute('aria-label')).length).toBeGreaterThan(0);
          expect(await tip.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
          await page.keyboard.press('Home');
          await pressStill(page, ['PageDown']);
          expect(await tip.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
          await page.keyboard.press('Home');
          expect(await tip.evaluate((element) => element.scrollTop)).toBe(0);
          const popup = context.waitForEvent('page', { timeout: 3000 });
          await page.keyboard.press('Enter');
          expect((await popup).url()).toBe('https://example.com/z');
          await page.bringToFront();
          await page.keyboard.press('Escape');
          await expect(input(page)).toBeFocused();
          await expect(tip).toBeHidden();
          expect(errors).toEqual([]);
        });
      }
    }
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input:visible').first();

  const tip = (page) => page.locator('.config-editor__problem-tip');

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  const chord = async (page) => {
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
  };

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('the problems tip closes when the focus leaves the editor', () => {
    const schema = JSON.stringify({ type: 'object', properties: { a: { type: 'integer' } }, additionalProperties: false });
    const value = 'a: x\nzz: 1\n';
    const withHostButton = (page) =>
      page.evaluate(() => {
        const after = document.createElement('button');
        after.id = 'host-after';
        after.textContent = 'After';
        document.querySelector('.config-editor').after(after);
      });

    test('from the text, by the keyboard and the mouse', async ({ page }) => {
      const errors = await open(page, { value, schema });
      await withHostButton(page);
      for (const leave of [(page) => page.locator('#host-after').focus(), (page) => page.locator('#host-after').click()]) {
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('F8');
        await expect(tip(page)).toBeVisible();
        await leave(page);
        await expect(tip(page)).toBeHidden();
        await expect(page.locator('#host-after')).toBeFocused();
      }
      expect(errors).toEqual([]);
    });

    test('from the focused tip, by Tab', async ({ page }) => {
      const errors = await open(page, { value, schema, hostDiagnostics: [{ path: '/zz', severity: 'warning', message: 'host says', docsUrl: 'https://example.com/zz' }] });
      await withHostButton(page);
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      await page.keyboard.press('F8');
      await expect(tip(page)).toBeVisible();
      await chord(page);
      await expect(tip(page)).toBeFocused();
      // Within the tip, it stays; out of the editor, it closes.
      for (let press = 0; press < 12; press += 1) {
        await page.keyboard.press('Tab');
        const outside = await page.evaluate(() => !document.activeElement.closest('.config-editor'));
        if (outside) break;
        const inTip = await page.evaluate(() => Boolean(document.activeElement.closest('.config-editor__problem-tip')));
        if (inTip) await expect(tip(page)).toBeVisible();
      }
      expect(await page.evaluate(() => !document.activeElement.closest('.config-editor'))).toBe(true);
      await expect(tip(page)).toBeHidden();
      // Moving into it from the text keeps it.
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      await expect(tip(page)).toBeVisible();
      await chord(page);
      await expect(tip(page)).toBeFocused();
      await expect(tip(page)).toBeVisible();
      expect(errors).toEqual([]);
    });
  });

  test.describe('a tip listing many problems with docs', () => {
    const many = Array.from({ length: 30 }, (_, i) => ({
      path: '/zz',
      severity: 'warning',
      message: `Host message number ${i} that is fairly long to need wrapping in the tip`,
      docsUrl: `https://example.com/d${i}`,
    }));

    async function focusedTip(page) {
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('End');
      await page.keyboard.press('ArrowLeft');
      await chord(page);
      await expect(tip(page)).toBeVisible();
      await chord(page);
      await expect(tip(page)).toBeFocused();
    }

    test('each docs link is named after its problem', async ({ page }) => {
      const errors = await open(page, { value: 'a: 1\nzz: 1\n', hostDiagnostics: many });
      await focusedTip(page);
      const names = await tip(page)
        .locator('a')
        .evaluateAll((links) => links.map((link) => link.getAttribute('aria-label')));
      // The tip lists as many as the editor's own list (20), then says more follow.
      expect(names).toHaveLength(20);
      expect(new Set(names).size).toBe(20);
      await expect(tip(page)).toContainText('more follow');
      for (const name of names) {
        // Its visible text first (as speech input says it), then its problem.
        expect(name).toMatch(/^Open docs ↗ \(Alt\+Enter\): Host message number \d+ /);
      }
      await expect(page.getByRole('link', { name: /Host message number 7 / })).toHaveCount(1);
      expect(errors).toEqual([]);
    });

    test('Enter and Alt+Enter open the problem in view, or the link focused', async ({ page, context }) => {
      const errors = await open(page, { value: 'a: 1\nzz: 1\n', hostDiagnostics: many });
      await context.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
      const opens = async (key) => {
        const popup = context.waitForEvent('page', { timeout: 3000 });
        await page.keyboard.press(key);
        const url = (await popup).url();
        await (await popup).close();
        await page.bringToFront();
        return url;
      };
      for (const key of ['Enter', 'Alt+Enter']) {
        await focusedTip(page);
        // At the top: the first problem's.
        const first = await tip(page).locator('a').first().getAttribute('href');
        expect(await opens(key)).toBe(first);
        // Scrolled down to the twelfth problem: its.
        await focusedTip(page);
        const twelfth = tip(page).locator('.config-editor__problem-tip-entry').nth(11);
        await twelfth.evaluate((entry) => {
          const box = entry.closest('.config-editor__problem-tip');
          box.scrollTop = entry.offsetTop - box.clientTop;
        });
        expect(await opens(key)).toBe(await twelfth.locator('a').getAttribute('href'));
        await page.keyboard.press('Escape');
      }
      // Alt+Enter on a link: that link's.
      await focusedTip(page);
      const third = tip(page).locator('a').nth(2);
      await third.focus();
      expect(await opens('Alt+Enter')).toBe(await third.getAttribute('href'));
      expect(errors).toEqual([]);
    });
  });
});
