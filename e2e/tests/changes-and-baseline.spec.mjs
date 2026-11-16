import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// The change preview (Alt+F3, Alt+F5, Revert) on a phone and from the keyboard, and a file loaded with
// its baseline staying clean.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

function listen(page) {
  page.errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page) {
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.documentsView?.destroy();
    window.module = await import('../config-editor/index.js');
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.saved = [];
    window.mountPoint = () => {
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '300px';
      document.body.prepend(element);
      return element;
    };
  });
}

test.describe('the change preview on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 } });
  setupEditor(test);

  test('it is as tall as its content when there is room: the Current side shows', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835');
      element.setSelectionRange(at, at + 4);
    });
    await page.keyboard.type('9000');
    await page.locator('.config-editor__diff-marker-modified').first().click();
    const peek = page.locator('.config-editor__diff-peek');
    await expect(peek).toBeVisible();
    const sizes = await peek.evaluate((element) => ({ scroll: element.scrollHeight, client: element.clientHeight }));
    expect(sizes.scroll, 'nothing scrolled out of the preview').toBeLessThanOrEqual(sizes.client + 1);
    const box = await peek.boundingBox();
    const current = await peek.locator('h3').last().boundingBox();
    expect(current.y + current.height).toBeLessThanOrEqual(box.y + box.height);
    await expect(peek.locator('.config-editor__diff-peek-current')).toBeVisible();
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

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the change preview stays inside the editor on a phone', () => {
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
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, name, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).first().waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the change preview from the keyboard', () => {
    const options = { value: 'a: 1\nb: 2\nc: 3\n', baseline: 'a: 1\nb: 9\nc: 3\n' };

    test('Enter on a diff marker focuses Revert; the preview is right after the text in the Tab order', async ({ page }) => {
      const errors = await open(page, 'vanilla.html', options);
      // Where Tab goes from the text with no preview (Ctrl+M mode).
      await input(page).click();
      await page.keyboard.press('Control+m');
      await page.keyboard.press('Tab');
      await page.evaluate(() => (window.afterText = document.activeElement));
      expect(await page.evaluate(() => window.afterText !== document.body)).toBe(true);

      const revert = page.locator('.config-editor__diff-peek-revert');
      await page.locator('.config-editor__diff-change-trigger').first().focus();
      await page.keyboard.press('Enter');
      await expect(revert).toBeFocused();
      // Shift+Tab from Revert: back to the text.
      await page.keyboard.press('Shift+Tab');
      await expect(input(page)).toBeFocused();
      await expect(revert).toBeVisible();
      // Tab from the text: Revert, the close button, then what follows the text.
      await page.keyboard.press('Tab');
      await expect(revert).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(page.locator('.config-editor__diff-peek-close')).toBeFocused();
      await page.keyboard.press('Tab');
      expect(await page.evaluate(() => document.activeElement === window.afterText)).toBe(true);
      expect(await input(page).inputValue()).toBe(options.value);
      expect(errors).toEqual([]);
    });
  });
});

// --- `update({ value, baseline })` never reports the document dirty.

test.describe('a file loaded with its baseline is never dirty', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // How the host loads `b: 2` over `a: 1`.
  const LOADS = {
    'value and baseline': { value: 'b: 2\n', baseline: 'b: 2\n' },
    'baseline and value': { baseline: 'b: 2\n', value: 'b: 2\n' },
    CRLF: { value: 'b: 2\r\n', baseline: 'b: 2\r\n' },
    'editing as JSON': { value: 'b: 2\n', baseline: 'b: 2\n', editAs: 'json' },
  };
  for (const [name, load] of Object.entries(LOADS)) {
    test(`ConfigEditor, ${name}: clean in the same task, and no dirty flag or change count meanwhile`, async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async (load) => {
        const heard = { dirty: [], changed: [] };
        const { editAs, ...update } = load;
        const editor = new window.module.ConfigEditor(window.mountPoint(), {
          value: 'a: 1\n',
          editAs,
          onDirtyChange: (dirty) => heard.dirty.push(dirty),
          onChangeSummary: (summary) => heard.changed.push(summary.added + summary.removed + summary.modified),
        });
        window.editor = editor;
        await window.pause(200);
        editor.update(update);
        const now = { dirty: editor.isDirty, diff: editor.unifiedDiff() };
        await window.pause(600);
        return { now, heard };
      }, load);
      expect(result).toEqual({ now: { dirty: false, diff: '' }, heard: { dirty: [false], changed: [0] } });
      expect(page.errors).toEqual([]);
    });
  }

  test('ConfigDocuments: a document loaded with its baseline is clean at once, and onDirtyChange stays quiet', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const heard = [];
      const view = new window.module.ConfigDocuments(window.mountPoint(), {
        documents: [
          { id: 'm', fileName: 'm.yaml', value: 'a: 1\n' },
          { id: 'h', fileName: 'h.yaml', value: 'a: 1\n' },
        ],
        onDirtyChange: (id, dirty) => heard.push(`${id} ${dirty}`),
      });
      window.documentsView = view;
      await window.pause(200);
      view.update({
        documents: [
          { id: 'm', fileName: 'm.yaml', value: 'b: 2\n', baseline: 'b: 2\n' },
          { id: 'h', fileName: 'h.yaml', value: 'b: 2\n', baseline: 'b: 2\n' },
        ],
      });
      const now = { m: view.isDirty('m'), editor: view.editor('m').isDirty };
      await window.pause(600);
      return { now, heard: heard.filter((entry) => entry.endsWith('true')) };
    });
    expect(result).toEqual({ now: { m: false, editor: false }, heard: [] });
    expect(page.errors).toEqual([]);
  });
});
