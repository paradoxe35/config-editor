import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 21 (UX): a click, double-click, drag or tap on underlined text
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

test.skip(!built, 'run `npm run build` in js/ first');

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

test.describe('U21-1: the pointer on underlined text reaches the text', () => {
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

test.describe('U21-3: forced colors keep every mark visible', () => {
  test.use({ viewport: { width: 900, height: 500 } });

  const schema = JSON.stringify({
    type: 'object',
    properties: { port: { type: 'integer' }, host: { type: 'string' }, alpha: { type: 'string' }, beta: { type: 'boolean' } },
  });
  const value = 'port: 1\nhost: example\nk1: a\nk2: b\nk1x: c\nblock:\n  a: 1\n  b: 2\n';

  // Whether `locator` (its first match) changes what is drawn in its box
  // when `change` undoes it there: hidden, or its marking class removed.
  async function shows(page, locator, change) {
    const element = locator.first();
    await expect(element).toBeVisible();
    const box = await element.boundingBox();
    const clip = { x: Math.max(0, box.x - 1), y: Math.max(0, box.y - 1), width: box.width + 2, height: box.height + 2 };
    await page.waitForTimeout(150);
    const before = await page.screenshot({ clip, animations: 'disabled' });
    await element.evaluate((element, change) => {
      if (change.remove) element.classList.remove(change.remove);
      else element.style.visibility = 'hidden';
    }, change);
    const after = await page.screenshot({ clip, animations: 'disabled' });
    return !before.equals(after);
  }
  const hide = {};
  const unmark = (name) => ({ remove: name });

  for (const forcedColors of ['none', 'active']) {
    test(`forced colors ${forcedColors}: matches, the current match, the active suggestion, the lock, a fold's problem mark and an on toggle show`, async ({ browser }) => {
      const context = await browser.newContext({ forcedColors, colorScheme: 'dark', viewport: { width: 900, height: 500 } });
      const page = await context.newPage();
      const errors = await open(page, { value, schema, readOnlyPaths: ['/host'] });
      const invisible = [];
      const check = async (what, locator, change) => {
        if (!(await shows(page, locator, change))) invisible.push(what);
      };
      await check('the gutter lock', page.locator('.config-editor__lock-marker'), hide);
      // The active suggestion, told from the other rows.
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Control+Space');
      await expect(page.locator('.config-editor__completion-active')).toBeVisible();
      await check('the active suggestion', page.locator('.config-editor__completion-active'), unmark('config-editor__completion-active'));
      await page.keyboard.press('Escape');
      // Find: every match, and the current one told from the others.
      await page.keyboard.press('Control+f');
      await page.keyboard.type('k1');
      await expect(page.locator('.config-editor__search-match')).toHaveCount(2);
      await check('a find match', page.locator('.config-editor__search-match:not(.config-editor__search-match-current)'), hide);
      await check('the current find match', page.locator('.config-editor__search-match-current'), unmark('config-editor__search-match-current'));
      await page.locator('.config-editor__find-toggle').first().click();
      await check('a find toggle that is on', page.locator('.config-editor__find-toggle-on'), unmark('config-editor__find-toggle-on'));
      await page.keyboard.press('Escape');
      // A closed fold hiding a problem: its chip's mark.
      await setHost(page, [{ range: { startLine: 7, startColumn: 3, endColumn: 4 }, severity: 'error', message: 'hidden' }]);
      await input(page).focus();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('ArrowUp');
      await page.keyboard.press('ArrowUp');
      await page.keyboard.press('ArrowUp');
      await page.keyboard.press('Control+Shift+BracketLeft');
      await check("a fold's problem mark", page.locator('.config-editor__fold-chip--error'), unmark('config-editor__fold-chip--error'));
      expect(invisible).toEqual([]);
      expect(errors).toEqual([]);
      await context.close();
    });
  }
});

test.describe('U21-8: the textarea, Ctrl+K Ctrl+I, docs links and the footer', () => {
  test.use({ viewport: { width: 1000, height: 600 } });

  test('the textarea is a multi-line text box; the open list shows through aria-controls, not aria-expanded', async ({ page }) => {
    const errors = await open(page, { value: 'port: 1\n', schema: JSON.stringify({ type: 'object', properties: { port: {}, host: {} } }) });
    await expect(input(page)).toHaveAttribute('role', 'textbox');
    await expect(input(page)).toHaveAttribute('aria-multiline', 'true');
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    await expect(page.locator('.config-editor__completion-active')).toBeVisible();
    await expect(input(page)).not.toHaveAttribute('aria-expanded', /./);
    await expect(input(page)).toHaveAttribute('aria-activedescendant', /./);
    expect(errors).toEqual([]);
  });

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
  for (const [language, [value, target]] of Object.entries(values)) {
    test(`${language}: Ctrl+K Ctrl+I on a value shows its field's docs`, async ({ page }) => {
      const errors = await open(page, { value, language, schema: listenSchema });
      await input(page).click();
      await input(page).evaluate((element, target) => {
        const at = element.value.indexOf(target) + 1;
        element.setSelectionRange(at, at);
      }, target);
      await page.keyboard.press('ArrowRight');
      await page.keyboard.press('ArrowLeft');
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+i');
      await expect(page.locator('.config-editor__hover')).toContainText('Port to listen on.');
      expect(errors).toEqual([]);
    });
  }

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
