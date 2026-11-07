import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// The path <-> position API (`rangeOf`, `pathAt`, `reveal`, `onPathChange`)
// through the npm packages' example pages. Build them first with
// `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');
const breadcrumb = (browserPage) => browserPage.locator('#breadcrumb');

// The selected text and whether the editor has focus.
const selection = (browserPage) =>
  input(browserPage).evaluate((element) => ({
    text: element.value.slice(element.selectionStart, element.selectionEnd),
    focused: document.activeElement === element,
  }));

// Puts the caret at the first `marker` in the text, plus `delta`, by clicking
// there as a user would (the key handlers track the caret on click).
async function caretAt(browserPage, marker, delta = 0) {
  await input(browserPage).evaluate(
    (element, [text, shift]) => {
      const at = element.value.indexOf(text) + shift;
      element.focus();
      element.setSelectionRange(at, at);
      element.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    },
    [marker, delta],
  );
}

test.describe('vanilla.html', () => {
  test('reveal selects a field, scrolls to it and focuses the editor', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await browserPage.locator('#reveal').click();
    await expect.poll(() => selection(browserPage)).toEqual({ text: 'insecure_plaintext: false', focused: true });
    await expect(breadcrumb(browserPage)).toHaveText('/insecure_plaintext');
    expect(await browserPage.evaluate(() => window.editor.reveal('/nope'))).toBe(false);
    expect(errors).toEqual([]);
  });

  test('reveal scrolls a field far down into view', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const long = Array.from({ length: 300 }, (_, index) => `key_${index}: ${index}`).join('\n') + '\n';
    await browserPage.evaluate((text) => window.editor.update({ value: text }), long);
    expect(await browserPage.evaluate(() => window.editor.reveal('/key_280'))).toBe(true);
    await expect.poll(() => selection(browserPage)).toEqual({ text: 'key_280: 280', focused: true });
    const view = await input(browserPage).evaluate((element) => {
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
      const top = 280 * lineHeight;
      return { top, scrollTop: element.scrollTop, height: element.clientHeight };
    });
    expect(view.top).toBeGreaterThanOrEqual(view.scrollTop);
    expect(view.top).toBeLessThan(view.scrollTop + view.height);
  });

  test('rangeOf and pathAt answer both ways, with pointers or segments', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const found = await browserPage.evaluate(() => {
      const editor = window.editor;
      const text = document.querySelector('.config-editor__input').value;
      const range = editor.rangeOf(['public_address']);
      return {
        range,
        slice: text.slice(range.start, range.end),
        same: JSON.stringify(editor.rangeOf('/public_address')) === JSON.stringify(range),
        path: editor.pathAt(text.indexOf('tunnel.example.com') + 3),
        missing: editor.rangeOf('/nope'),
        past: editor.pathAt(text.length + 10),
      };
    });
    expect(found.slice).toBe('public_address: tunnel.example.com');
    expect(found.range).toMatchObject({ startLine: 3, startColumn: 1, endLine: 3, endColumn: 35 });
    expect(found.same).toBe(true);
    expect(found.path).toEqual({ pointer: '/public_address', segments: ['public_address'] });
    expect(found.missing).toBeNull();
    expect(found.past).toBeNull();
    await expect(browserPage.evaluate(() => window.editor.rangeOf([{}]))).rejects.toThrow(/path segment/);
  });

  test('onPathChange follows the caret', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await caretAt(browserPage, 'tunnel.example.com', 4);
    await expect(breadcrumb(browserPage)).toHaveText('/public_address');
    await browserPage.keyboard.press('ArrowUp');
    await expect(breadcrumb(browserPage)).toHaveText('/listen');
    await browserPage.keyboard.press('Control+Home');
    await expect(breadcrumb(browserPage)).toHaveText('/');
    // Typing a new key follows too.
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('web:\n  http: 8');
    await browserPage.keyboard.press('Escape');
    await expect(breadcrumb(browserPage)).toHaveText('/web/http');
    expect(await browserPage.evaluate(() => window.editor.currentPath.pointer)).toBe('/web/http');
  });

  test('paths are the same while editing as another language', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#edit-as').selectOption('toml');
    await expect(input(browserPage)).toHaveValue(/listen = 7835/);
    await browserPage.locator('#reveal').click();
    await expect.poll(() => selection(browserPage)).toEqual({ text: 'insecure_plaintext = false', focused: true });
    await caretAt(browserPage, '7835', 2);
    await expect(breadcrumb(browserPage)).toHaveText('/listen');
  });

  test('a removed onPathChange stops, and a new one starts', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(() => window.editor.update({ onPathChange: null }));
    await caretAt(browserPage, 'tunnel.example.com', 4);
    await browserPage.waitForTimeout(100);
    await expect(breadcrumb(browserPage)).not.toHaveText('/public_address');
    await browserPage.evaluate(() =>
      window.editor.update({ onPathChange: (path) => (document.getElementById('breadcrumb').textContent = `new ${path.pointer}`) }),
    );
    await expect(breadcrumb(browserPage)).toHaveText('new /public_address');
  });
});

for (const name of FRAMEWORK_PAGES) test.describe(name, () => {
  test('ref.reveal, ref.rangeOf, ref.pathAt and onPathChange', async ({ page: browserPage }) => {
    const errors = await open(browserPage, name);
    await browserPage.locator('#reveal').click();
    await expect.poll(() => selection(browserPage)).toEqual({ text: 'public_address: tunnel.example.com', focused: true });
    await expect(breadcrumb(browserPage)).toHaveText('/public_address');
    const found = await browserPage.evaluate(() => {
      const handle = window.editorHandle.current;
      const range = handle.rangeOf('/listen');
      return {
        range: [range.startLine, range.startColumn, range.endColumn],
        path: handle.pathAt(range.start + 9).pointer,
        current: handle.currentPath.pointer,
      };
    });
    expect(found).toEqual({ range: [2, 1, 13], path: '/listen', current: '/public_address' });
    await caretAt(browserPage, '7835', 1);
    await expect(breadcrumb(browserPage)).toHaveText('/listen');
    expect(errors).toEqual([]);
  });
});
