import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A document's read-only ranges after a reset of the host's own
// (`setValue`, `replaceValue`): ranges the host keeps listing, worked out on
// the text before the reset, name places past the reset's end. They are read
// in the text they came with (as a `ConfigEditor` reads them), so the list
// goes on applying: an update of another document lands, and a save's new
// baseline leaves the document clean. And a document whose ranges fit no
// text holds no other document's update back: that one is reported.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

function listen(page) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
}

/** The `main` document's `public_address` line locked through its line break, as the host lists it. */
const LOCK = [{ startLine: 2, startColumn: 1, endLine: 3, endColumn: 1 }];

for (const name of ['documents.html', 'documents-react.html', 'documents-vue.html', 'documents-svelte.html']) {
  test(`${name}: ranges past a reset's end are read in the text they came with; the list goes on applying`, async ({ page }) => {
    listen(page);
    await page.goto(example(name));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate((lock) => window.updateFile('main', { readOnlyRanges: lock }), LOCK);
    await page.waitForTimeout(200);
    await page.evaluate(() => {
      const view = window.documentsView ?? window.documentsHandle.current;
      if (window.documentsView) {
        // The vanilla host lists the reset text itself, its ranges kept.
        view.replaceValue('main', 'listen: 1\n');
        window.updateFile('main', { value: 'listen: 1\n' });
      } else {
        view.setValue('main', 'listen: 1\n');
      }
    });
    await page.waitForTimeout(300);
    await page.evaluate(() => window.updateFile('limits', { value: '[limits]\nmax_pending = 9\n' }));
    await expect
      .poll(() => page.evaluate(() => (window.documentsView ?? window.documentsHandle.current).editor('limits').value))
      .toBe('[limits]\nmax_pending = 9\n');
    // Typed, then saved: the host's new baseline is the text.
    const box = page.locator('.config-editor__input').first();
    await box.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# x');
    await expect(box).toHaveValue('listen: 1\n# x');
    await page.keyboard.press('Control+s');
    await expect
      .poll(() => page.evaluate(() => (window.documentsView ?? window.documentsHandle.current).isDirty('main')))
      .toBe(false);
    expect(page.errors).toEqual([]);
  });
}

test('a document whose ranges fit no text holds no other document back; it is reported, its locks kept', async ({ page }) => {
  listen(page);
  await page.goto(example('documents.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  const result = await page.evaluate(async () => {
    window.documentsView.destroy();
    const module = await import('../config-editor/index.js');
    const element = document.createElement('div');
    document.body.prepend(element);
    const lock = [{ startLine: 2, startColumn: 1, endLine: 2, endColumn: 10 }];
    const list = (a, b, ranges, extra = []) => [
      { id: 'a', fileName: 'a.yaml', value: a, readOnlyRanges: ranges },
      { id: 'b', fileName: 'b.yaml', value: b },
      ...extra,
    ];
    const view = new module.ConfigDocuments(element, { documents: list('x: 1\nlisten: 1\n', 'b: 1\n', lock) });
    await new Promise((resolve) => setTimeout(resolve, 100));
    const thrown = (apply) => {
      try {
        apply();
        return null;
      } catch (error) {
        return String(error.message ?? error);
      }
    };
    const bad = [{ startLine: 9, startColumn: 1, endLine: 9, endColumn: 2 }];
    const known = thrown(() => view.update({ documents: list('y: 2\nlisten: 1\n', 'b: 2\n', bad) }));
    await new Promise((resolve) => setTimeout(resolve, 100));
    const out = { known, a: view.editor('a').value, b: view.editor('b').value };
    // The lock it had stays on `listen: 1` (carried into its new text).
    view.select('a');
    await new Promise((resolve) => setTimeout(resolve, 100));
    const input = element.querySelector('.config-editor__input');
    for (const [at, text] of [[4, 'Q'], [15, 'Z']]) {
      input.focus();
      input.setSelectionRange(at, at);
      document.execCommand('insertText', false, text);
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    out.typed = view.editor('a').value;
    out.locks = [...element.querySelectorAll('.config-editor__lock-marker')].length;
    // A document listed for the first time with ranges that fit nothing
    // is locked whole; the others still update.
    const added = { id: 'c', fileName: 'c.yaml', value: 'c: 1\n', readOnlyRanges: bad };
    out.added = thrown(() => view.update({ documents: list(out.typed, 'b: 3\n', lock, [added]) }));
    await new Promise((resolve) => setTimeout(resolve, 100));
    out.b3 = view.editor('b').value;
    view.select('c');
    await new Promise((resolve) => setTimeout(resolve, 100));
    const inputs = [...element.querySelectorAll('.config-editor__input')];
    const shown = inputs.find((each) => each.value === 'c: 1\n');
    shown.focus();
    shown.setSelectionRange(0, 0);
    document.execCommand('insertText', false, 'W');
    await new Promise((resolve) => setTimeout(resolve, 100));
    out.c = view.editor('c').value;
    return out;
  });
  expect(result.known).toContain('documents[0].readOnlyRanges[0]');
  expect(result.a).toBe('y: 2\nlisten: 1\n');
  expect(result.b).toBe('b: 2\n');
  expect(result.typed, JSON.stringify(result)).toBe('y: 2Q\nlisten: 1\n');
  expect(result.added).toContain('documents[2].readOnlyRanges[0]');
  expect(result.b3).toBe('b: 3\n');
  expect(result.c).toBe('c: 1\n');
});

test('ConfigEditor: a reset, then its text with ranges that fit only the text before it, reads them there', async ({ page }) => {
  listen(page);
  await page.goto(example('documents.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  const result = await page.evaluate(async () => {
    window.documentsView.destroy();
    const module = await import('../config-editor/index.js');
    const element = document.createElement('div');
    document.body.prepend(element);
    const lock = [{ startLine: 2, startColumn: 1, endLine: 3, endColumn: 1 }];
    const editor = new module.ConfigEditor(element, { language: 'yaml', value: 'a: 1\nlisten: 1\nb: 2\n', readOnlyRanges: lock });
    await new Promise((resolve) => setTimeout(resolve, 100));
    editor.replaceValue('listen: 1\n');
    try {
      editor.update({ value: 'listen: 1\n', readOnlyRanges: lock });
    } catch (error) {
      return String(error.message ?? error);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
    const input = element.querySelector('.config-editor__input');
    input.focus();
    input.setSelectionRange(9, 9);
    document.execCommand('insertText', false, 'Z');
    await new Promise((resolve) => setTimeout(resolve, 100));
    return editor.value;
  });
  // The lock carried into the reset's text stays on `listen: 1`.
  expect(result).toBe('listen: 1\n');
  expect(page.errors).toEqual([]);
});
