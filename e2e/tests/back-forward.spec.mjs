import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Go back / go forward: a jump (here F12) notes where the user was, and
// `Alt+←` puts the selection and the scroll back exactly, `Alt+→` goes
// again; the places follow edits made since; with nowhere to go the key is
// left to the browser. In a documents view, going back after a jump into
// another document returns to the document it left.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const SCHEMA = JSON.stringify({
  $id: 'https://example.com/server',
  type: 'object',
  properties: {
    clients: { type: 'object' },
    default_client: { type: 'string', 'x-config-editor-keys': '/clients' },
  },
});
const filler = (prefix, count, indent = '', value = 'x') =>
  Array.from({ length: count }, (_, index) => `${indent}${prefix}${index}: ${value}`);
// Zero-based lines: `main:` is 61, `default_client: main` 123.
const VALUE = [
  ...filler('f', 60),
  'clients:',
  '  main: {}',
  '  lab: {}',
  ...filler('c', 60, '  ', '{}'),
  'default_client: main',
  ...filler('g', 80),
  '',
].join('\n');
const DEFINITION_LINE = 61;
const USE_LINE = 123;

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();

async function openEditor(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla({ value: VALUE, schema: SCHEMA }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await field(page).waitFor();
  await field(page).click();
  return errors;
}

/** Scrolls so zero-based `line` is near the top, the caret `into` the first `word` there. */
const placeCaret = (page, line, word, into) =>
  field(page).evaluate(
    (element, [line, word, into]) => {
      const lines = element.value.split('\n');
      const start = lines.slice(0, line).join('\n').length + (line > 0 ? 1 : 0);
      const at = start + lines[line].indexOf(word) + into;
      element.setSelectionRange(at, at);
      const style = getComputedStyle(element);
      element.scrollTop = parseFloat(style.paddingTop) + (line - 5) * parseFloat(style.lineHeight) + 7;
      element.dispatchEvent(new Event('scroll'));
    },
    [line, word, into],
  );

/** The selection, the scroll and the caret's zero-based line. */
const place = (page) =>
  field(page).evaluate((element) => ({
    start: element.selectionStart,
    end: element.selectionEnd,
    top: element.scrollTop,
    line: element.value.slice(0, element.selectionStart).split('\n').length - 1,
  }));

/** Sends `Alt+←` (or `Alt+→`) as a key event; returns whether it was prevented. */
const altArrow = (page, key) =>
  field(page).evaluate(
    (element, key) =>
      !element.dispatchEvent(
        new KeyboardEvent('keydown', { key, code: key, altKey: true, bubbles: true, cancelable: true }),
      ),
    key,
  );

test.describe('go back and forward after a jump, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Alt+Left after F12 far away puts the caret and the scroll back; Alt+Right goes again', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, USE_LINE, 'main', 2);
    const before = await place(page);
    await page.keyboard.press('F12');
    await expect.poll(() => place(page).then((now) => now.line)).toBe(DEFINITION_LINE);
    const there = await place(page);
    expect(there.top).not.toBe(before.top);
    expect(await page.evaluate(() => window.editor.canGoBack())).toBe(true);
    await page.keyboard.press('Alt+ArrowLeft');
    await expect.poll(() => place(page)).toEqual(before);
    expect(await page.evaluate(() => [window.editor.canGoBack(), window.editor.canGoForward()])).toEqual([
      false,
      true,
    ]);
    await page.keyboard.press('Alt+ArrowRight');
    await expect.poll(() => place(page)).toEqual(there);
    // The handle's methods walk the same history.
    expect(await page.evaluate(() => window.editor.goBack())).toBe(true);
    await expect.poll(() => place(page)).toEqual(before);
    expect(await page.evaluate(() => window.editor.goBack())).toBe(false);
    expect(await page.evaluate(() => window.editor.goForward())).toBe(true);
    await expect.poll(() => place(page)).toEqual(there);
    expect(errors).toEqual([]);
  });

  test('the place gone back to follows the lines typed above it since', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, USE_LINE, 'main', 2);
    const before = await place(page);
    await page.keyboard.press('F12');
    await expect.poll(() => place(page).then((now) => now.line)).toBe(DEFINITION_LINE);
    // Two lines typed at the top of the definition's line.
    await page.keyboard.press('Home');
    await page.keyboard.insertText('  new1: {}\n  new2: {}\n');
    await page.keyboard.press('Alt+ArrowLeft');
    await expect.poll(() => place(page).then((now) => now.line)).toBe(USE_LINE + 2);
    const back = await place(page);
    const after = await field(page).evaluate((element) => element.value.slice(element.selectionStart, element.selectionStart + 2));
    expect(after).toBe('in');
    expect(back.start).toBeGreaterThan(before.start);
    expect(errors).toEqual([]);
  });

  test('with nowhere to go, Alt+Left and Alt+Right are left to the browser', async ({ page }) => {
    const errors = await openEditor(page);
    expect(await altArrow(page, 'ArrowLeft')).toBe(false);
    expect(await altArrow(page, 'ArrowRight')).toBe(false);
    await placeCaret(page, USE_LINE, 'main', 2);
    await page.keyboard.press('F12');
    await expect.poll(() => place(page).then((now) => now.line)).toBe(DEFINITION_LINE);
    expect(await altArrow(page, 'ArrowRight')).toBe(false);
    expect(await altArrow(page, 'ArrowLeft')).toBe(true);
    expect(await altArrow(page, 'ArrowLeft')).toBe(false);
    expect(await altArrow(page, 'ArrowRight')).toBe(true);
    expect(errors).toEqual([]);
  });
});

// The documents example's `?keys` setup (js/examples/key-names.js): the
// server file's `default_client` names a key of the drop-in `lab`.
const SERVER = 'server';
const LAB = 'lab';
const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const input = (page, id) =>
  page.locator(`[role="tabpanel"][data-document-id="${id}"]`).locator('.config-editor__input');

test.describe('go back across documents, documents.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Alt+Left after F12 into another document returns to the document and place it left', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(`${pathToFileURL(join(js, 'examples', 'documents.html')).href}?keys`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await expect(page.locator('[role="tab"]')).toHaveCount(3);
    await tab(page, SERVER).click();
    await input(page, SERVER).click();
    await input(page, SERVER).evaluate((element) => {
      const at = element.value.indexOf('default_client: ') + 'default_client: '.length;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('lab');
    await page.keyboard.press('Escape');
    await page.keyboard.press('ArrowLeft');
    const left = await input(page, SERVER).evaluate((element) => element.selectionStart);
    await page.keyboard.press('F12');
    await expect(tab(page, LAB)).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('Alt+ArrowLeft');
    await expect(tab(page, SERVER)).toHaveAttribute('aria-selected', 'true');
    await expect.poll(() => input(page, SERVER).evaluate((element) => element.selectionStart)).toBe(left);
    await expect(input(page, SERVER)).toBeFocused();
    // And forward again, from the view's handle.
    expect(await page.evaluate(() => window.documentsView.goForward())).toBe(true);
    await expect(tab(page, LAB)).toHaveAttribute('aria-selected', 'true');
    await expect
      .poll(() => input(page, LAB).evaluate((element) => element.value.slice(element.selectionStart).startsWith('lab')))
      .toBe(true);
    expect(errors).toEqual([]);
  });
});
