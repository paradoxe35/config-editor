import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Jumps (go to definition, a use, a problem, a line) put their target in
// the middle of the editor when it is out of view or near an edge, leave a
// target well in view where it is, and flash it for a second (no fade with
// reduced motion). Go to definition selects the key, as a use is selected.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const ID = 'https://example.com/server';
const SCHEMA = JSON.stringify({
  $id: ID,
  type: 'object',
  properties: {
    clients: { type: 'object' },
    default_client: { type: 'string', 'x-config-editor-keys': '/clients' },
    port: { type: 'integer' },
  },
});
const filler = (prefix, count, indent = '', value = 'x') =>
  Array.from({ length: count }, (_, index) => `${indent}${prefix}${index}: ${value}`);
// Zero-based lines: `main:` is 61, `default_client: main` 123, `port` 184.
const VALUE = [
  ...filler('f', 60),
  'clients:',
  '  main: {}',
  '  lab: {}',
  ...filler('c', 60, '  ', '{}'),
  'default_client: main',
  ...filler('g', 60),
  'port: oops',
  ...filler('h', 40),
  '',
].join('\n');
const DEFINITION_LINE = 61;
const USE_LINE = 123;
const PROBLEM_LINE = 184;

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();
// The edge around a flashed range, and the bars across its rows.
const flash = (page) => page.locator('.config-editor__jump-flash');
const bars = (page) => page.locator('.config-editor__jump-flash-line');

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

/** Scrolls the text so zero-based `line` is the top row, the caret on it `into` the first `word` there. */
const placeCaret = (page, line, word, into = 1) =>
  field(page).evaluate(
    (element, [line, word, into]) => {
      const lines = element.value.split('\n');
      const start = lines.slice(0, line).join('\n').length + (line > 0 ? 1 : 0);
      const at = start + lines[line].indexOf(word) + into;
      element.setSelectionRange(at, at);
      const style = getComputedStyle(element);
      element.scrollTop = parseFloat(style.paddingTop) + (line - 5) * parseFloat(style.lineHeight);
      element.dispatchEvent(new Event('scroll'));
    },
    [line, word, into],
  );

/** Where the caret's row is: its zero-based line, and its middle as a share of the view's height (0 top, 1 bottom). */
const caretPlace = (page) =>
  field(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    const line = element.value.slice(0, element.selectionStart).split('\n').length - 1;
    const middle = parseFloat(style.paddingTop) + (line + 0.5) * lineHeight - element.scrollTop;
    return { line, share: middle / element.clientHeight };
  });

const selected = (page) => field(page).evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
const scrollTop = (page) => field(page).evaluate((element) => element.scrollTop);

/** The caret's row is in the middle band of the view. */
async function expectCentered(page, line) {
  await expect.poll(() => caretPlace(page).then((place) => place.line)).toBe(line);
  const { share } = await caretPlace(page);
  expect(share, `the line sits at ${Math.round(share * 100)}% of the view`).toBeGreaterThan(0.35);
  expect(share, `the line sits at ${Math.round(share * 100)}% of the view`).toBeLessThan(0.65);
}

test.describe('jumps center their target and flash it, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('go to definition from far below selects the key in the middle of the view, flashed', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, USE_LINE, 'main', 2);
    await page.keyboard.press('F12');
    await expectCentered(page, DEFINITION_LINE);
    expect(await selected(page)).toBe('main');
    await expect(flash(page)).toHaveText('main');
    await expect(bars(page)).toHaveCount(1);
    // Gone after its second.
    await expect(flash(page)).toHaveCount(0, { timeout: 3000 });
    await expect(bars(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('going to a use from far above puts it in the middle of the view, flashed', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, DEFINITION_LINE, 'main', 2);
    await page.keyboard.press('Shift+F12');
    await expect(page.locator('.config-editor__uses [role="option"]')).toHaveCount(1);
    await page.keyboard.press('Enter');
    await expectCentered(page, USE_LINE);
    expect(await selected(page)).toBe('main');
    await expect(flash(page)).toHaveText('main');
    expect(errors).toEqual([]);
  });

  test('the next problem and go to line center their line and flash the whole row', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, 0, 'f0', 0);
    await page.keyboard.press('F8');
    await expectCentered(page, PROBLEM_LINE);
    await expect(bars(page)).toHaveCount(1);
    await page.keyboard.press('Control+g');
    await page.keyboard.type('31');
    await page.keyboard.press('Enter');
    await expectCentered(page, 30);
    await expect(bars(page)).toHaveCount(1);
    expect(errors).toEqual([]);
  });

  test('a jump to a line well in view does not scroll; one at the bottom edge centers it', async ({ page }) => {
    const errors = await openEditor(page);
    await placeCaret(page, 40, 'f40', 0);
    const before = await scrollTop(page);
    await page.keyboard.press('Control+g');
    await page.keyboard.type('49');
    await page.keyboard.press('Enter');
    await expect.poll(() => caretPlace(page).then((place) => place.line)).toBe(48);
    expect(await scrollTop(page)).toBe(before);
    // The last row in view: hard to notice there, so centered.
    const last = await field(page).evaluate((element) => {
      const style = getComputedStyle(element);
      const rows = Math.floor((element.clientHeight - parseFloat(style.paddingTop)) / parseFloat(style.lineHeight));
      return 35 + rows - 1;
    });
    await page.keyboard.press('Control+g');
    await page.keyboard.type(String(last + 1));
    await page.keyboard.press('Enter');
    await expectCentered(page, last);
    expect(errors).toEqual([]);
  });

  test('with reduced motion the flash shows without a fade, then goes', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    const errors = await openEditor(page);
    await placeCaret(page, USE_LINE, 'main', 2);
    await page.keyboard.press('F12');
    await expect(bars(page)).toHaveCount(1);
    const look = await bars(page).evaluate((element) => {
      const style = getComputedStyle(element);
      return { animation: style.animationName, opacity: style.opacity, fill: style.backgroundColor };
    });
    expect(look.animation).toBe('none');
    expect(look.opacity).toBe('1');
    expect(look.fill).not.toBe('rgba(0, 0, 0, 0)');
    await expect(bars(page)).toHaveCount(0, { timeout: 3000 });
    expect(errors).toEqual([]);
  });
});
