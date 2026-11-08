import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Everything that moves the focus into the editor from code leaves the host
// page where it is and shows the caret inside the editor's own scroll area:
// the host's `focus()`, `reveal(path)` and `revealRange(range)`, a tab
// switch, go to line, find (Ctrl+F, its next/previous, Escape), the
// baseline-conflict notice's buttons, a refused save and a change reverted.
// The host page is taller than the window, its editor below the fold, the
// page scrolled to the top (burrow: a plain `element.focus()` scrolled the
// page to the editor and hid the host's controls above it). Steps that only
// set things up (typing an edit) scroll to the editor first and back after:
// typing into a field off screen scrolls the page in any browser.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;
test.skip(!built, 'js packages not built: run `npm run build` in js/');

const long =
  Array.from({ length: 300 }, (_, index) => (index % 50 === 7 ? `# note ${index}\n` : '') + `key${index}: ${index}`).join('\n') + '\n';

const input = (browserPage) => browserPage.locator('.config-editor__input:visible');

/** A host page taller than the window, the editor below what is in view. */
async function hostPage(browserPage, name) {
  await browserPage.setViewportSize({ width: 1000, height: 700 });
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.addStyleTag({ content: 'body { padding-top: 1200px; padding-bottom: 4000px; }' });
}

const settled = (browserPage) =>
  browserPage.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 100)))));

/** Scrolls the page to `y`: by default the top, the editor below the fold. */
async function toTop(browserPage, y = 0) {
  await browserPage.evaluate((y) => window.scrollTo(0, y), y);
  await settled(browserPage);
  expect(await browserPage.evaluate(() => window.scrollY)).toBe(y);
  if (y === 0) {
    expect(await input(browserPage).evaluate((element) => element.getBoundingClientRect().top)).toBeGreaterThan(700);
  }
}

/** Runs a setup step with the editor in view, then scrolls back to the top. */
async function inView(browserPage, step) {
  await input(browserPage).evaluate((element) => element.scrollIntoView({ block: 'center' }));
  await step();
  await toTop(browserPage);
}

/** Where the caret is, and whether its line is inside the text area's own visible area. */
const caret = (browserPage) =>
  input(browserPage).evaluate((element) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    const line = element.value.slice(0, element.selectionStart).split('\n').length - 1;
    const top = parseFloat(style.paddingTop) + line * lineHeight;
    return {
      line: line + 1,
      start: element.selectionStart,
      end: element.selectionEnd,
      inView: top >= element.scrollTop && top + lineHeight <= element.scrollTop + element.clientHeight,
      scrolled: element.scrollTop > 0,
    };
  });

/** `action` leaves the page at `y` (the top) and the caret in view; its caret. */
async function pageStays(browserPage, action, y = 0) {
  await toTop(browserPage, y);
  await action();
  await settled(browserPage);
  expect(await browserPage.evaluate(() => window.scrollY)).toBe(y);
  const at = await caret(browserPage);
  expect(at.inView, JSON.stringify(at)).toBe(true);
  return at;
}

const focused = (browserPage) => browserPage.evaluate(() => document.activeElement?.className ?? '');

/** Sets `field`'s text as typing would, without the browser revealing it. */
const fill = (field, text) =>
  field.evaluate((element, value) => {
    element.value = value;
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: value }));
  }, text);

test.describe('vanilla: the page stays where it is', () => {
  test.beforeEach(async ({ page: browserPage }) => {
    await hostPage(browserPage, 'vanilla.html');
    await browserPage.evaluate((value) => window.editor.update({ value, baseline: value }), long);
    await expect(input(browserPage)).toHaveValue(/key299: 299/);
  });

  test('focus, reveal and revealRange', async ({ page: browserPage }) => {
    await pageStays(browserPage, () => browserPage.evaluate(() => window.editor.focus()));
    expect(await focused(browserPage)).toContain('config-editor__input');
    const revealed = await pageStays(browserPage, () => browserPage.evaluate(() => window.editor.reveal('/key250')));
    expect(revealed.scrolled).toBe(true);
    expect(await browserPage.evaluate(() => window.editor.text).then((text) => text.slice(revealed.start, revealed.end))).toBe('key250: 250');
    const range = await browserPage.evaluate(() => window.editor.rangeOf('/key280'));
    await pageStays(browserPage, () => browserPage.evaluate(() => window.editor.focus()));
    const shown = await pageStays(browserPage, () => browserPage.evaluate((range) => window.editor.revealRange(range), range));
    expect([shown.start, shown.end]).toEqual([range.start, range.end]);
  });

  test('go to line', async ({ page: browserPage }) => {
    await browserPage.evaluate(() => window.editor.focus());
    await pageStays(browserPage, () => browserPage.keyboard.press('Control+g'));
    const field = browserPage.getByRole('textbox', { name: 'Go to line' });
    await expect(field).toBeFocused();
    await fill(field, '260');
    const at = await pageStays(browserPage, () => browserPage.keyboard.press('Enter'));
    await expect(input(browserPage)).toBeFocused();
    expect(at.line).toBe(260);
    expect(at.scrolled).toBe(true);
  });

  test('find, its next and previous, and Escape', async ({ page: browserPage }) => {
    await browserPage.evaluate(() => window.editor.focus());
    await pageStays(browserPage, () => browserPage.keyboard.press('Control+f'));
    const query = browserPage.locator('.config-editor__find-input').first();
    await expect(query).toBeFocused();
    await fill(query, 'key27');
    const count = browserPage.locator('.config-editor__find-count');
    await expect(count).toHaveText(/of 11/);
    const first = await pageStays(browserPage, () => browserPage.keyboard.press('Enter'));
    const second = await pageStays(browserPage, () => browserPage.keyboard.press('Enter'));
    expect(second.line).toBeGreaterThan(first.line);
    await pageStays(browserPage, () => browserPage.keyboard.press('Shift+Enter'));
    await pageStays(browserPage, () => browserPage.keyboard.press('F3'));
    const next = browserPage.getByRole('button', { name: 'Next match' });
    const previous = browserPage.getByRole('button', { name: 'Previous match' });
    await pageStays(browserPage, () => next.evaluate((button) => button.click()));
    await pageStays(browserPage, () => previous.evaluate((button) => button.click()));
    await query.evaluate((element) => element.focus({ preventScroll: true }));
    const closed = await pageStays(browserPage, () => browserPage.keyboard.press('Escape'));
    await expect(input(browserPage)).toBeFocused();
    expect(closed.scrolled).toBe(true);
  });

  for (const button of ['reload', 'keep']) {
    test(`the baseline-conflict notice's ${button} button`, async ({ page: browserPage }) => {
      await inView(browserPage, async () => {
        await browserPage.evaluate(() => window.editor.reveal('/key250'));
        await browserPage.keyboard.press('End');
        await browserPage.keyboard.type('0');
      });
      await expect.poll(() => browserPage.evaluate(() => window.editor.isDirty)).toBe(true);
      await browserPage.evaluate((value) => window.editor.update({ baseline: value.replace('key3: 3', 'key3: 33') }), long);
      const notice = browserPage.locator('.config-editor__notice');
      await expect(notice).toBeVisible();
      await pageStays(browserPage, () => browserPage.keyboard.press('F6'));
      await browserPage.locator(`.config-editor__notice-${button}`).evaluate((element) => element.focus({ preventScroll: true }));
      await pageStays(browserPage, () => browserPage.keyboard.press('Enter'));
      await expect(notice).toBeHidden();
      await expect(input(browserPage)).toBeFocused();
    });
  }

  test('a refused save', async ({ page: browserPage }) => {
    await browserPage.evaluate(() => window.editor.update({ editAs: 'json' }));
    await expect(input(browserPage)).toHaveValue(/^\{/);
    await inView(browserPage, async () => {
      await browserPage.evaluate(() => window.editor.focus());
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type(' ]');
    });
    const saves = await browserPage.evaluate(() => window.saves.length);
    // The caret goes to the problem, at the end of the text, scrolled there inside the editor.
    await browserPage.evaluate(() => window.editor.reveal('/key0'));
    await toTop(browserPage);
    const at = await pageStays(browserPage, () => browserPage.keyboard.press('Control+s'));
    expect(await browserPage.evaluate(() => window.saves.length)).toBe(saves);
    expect(at.scrolled).toBe(true);
    await expect(input(browserPage)).toBeFocused();
  });

  // The preview hides while its line is out of the window (popups follow
  // their anchor), so here the editor is in view, the page scrolled down to
  // it: reverting still leaves the page exactly where it is.
  test('a change reverted', async ({ page: browserPage }) => {
    await inView(browserPage, async () => {
      await browserPage.evaluate(() => window.editor.reveal('/key250'));
      await browserPage.keyboard.press('End');
      await browserPage.keyboard.type('0');
    });
    await expect(browserPage.locator('.config-editor__change-status')).toHaveText('1 change');
    const y = await input(browserPage).evaluate((element) => Math.round(element.getBoundingClientRect().top - 40));
    await pageStays(browserPage, () => browserPage.evaluate(() => window.editor.reveal('/key0')), y);
    await pageStays(browserPage, () => browserPage.keyboard.press('Alt+F3'), y);
    const revert = browserPage.locator('.config-editor__diff-peek').getByRole('button', { name: /Revert this change/ });
    await expect(revert).toBeFocused();
    const at = await pageStays(browserPage, () => browserPage.keyboard.press('Enter'), y);
    await expect(input(browserPage)).toBeFocused();
    await expect(input(browserPage)).toHaveValue(/key250: 250\n/);
    expect(at.scrolled).toBe(true);
  });
});

test('documents: a tab switch leaves the page where it is', async ({ page: browserPage }) => {
  await hostPage(browserPage, 'documents.html');
  await browserPage.evaluate((value) => window.updateFile('limits', { value: `[limits]\n${value.replace(/: /g, ' = ')}` }), long);
  await toTop(browserPage);
  // A click on the tab (element.click(): Playwright's own click would scroll the tab into view first).
  await browserPage.getByRole('tab', { name: /limits/ }).evaluate((tab) => tab.click());
  await expect(input(browserPage)).toHaveValue(/key299 = 299/);
  await pageStays(browserPage, () => Promise.resolve());
  const range = await browserPage.evaluate(() => window.documentsView.editor('limits').rangeOf('/limits/key280'));
  const shown = await pageStays(browserPage, () => browserPage.evaluate((range) => window.documentsView.editor('limits').revealRange(range), range));
  expect([shown.start, shown.end]).toEqual([range.start, range.end]);
});

for (const name of ['react.html', 'vue.html', 'svelte.html']) {
  test(`${name}: focus, reveal and revealRange leave the page where it is`, async ({ page: browserPage }) => {
    await hostPage(browserPage, name);
    await browserPage.evaluate((value) => window.setParentValue(value), long);
    await expect(input(browserPage)).toHaveValue(/key299: 299/);
    const handle = 'window.editorHandle.current';
    await pageStays(browserPage, () => browserPage.evaluate(`${handle}.focus()`));
    await expect(input(browserPage)).toBeFocused();
    const revealed = await pageStays(browserPage, () => browserPage.evaluate(`${handle}.reveal('/key250')`));
    expect(revealed.scrolled).toBe(true);
    await browserPage.evaluate(`${handle}.focus()`);
    const range = await browserPage.evaluate(`${handle}.rangeOf('/key280')`);
    const shown = await pageStays(browserPage, () => browserPage.evaluate(`${handle}.revealRange(${JSON.stringify(range)})`));
    expect([shown.start, shown.end]).toEqual([range.start, range.end]);
  });
}
