import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Every setting of `EditorSettings` switches live on the example pages:
// vanilla (`update`), React, Vue and Svelte (props), and the documents view, vanilla
// (`update` on the view) and each framework's (its props). Live means the editor
// is not mounted again: the textarea stays the same element, the caret
// stays where it was and the typing before the switch is undone after it.
// Also: a document's own settings over the view's, and reaching a
// document's editor from JS (`documents.editor(id)`, the React ref's
// `editor(id)`). Run against the built packages (`npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

// The framework pages (React, Vue, Svelte) share their controls and `window` hooks.
const FRAMEWORKS = ['react.html', 'vue.html', 'svelte.html'];
const DOCUMENTS_FRAMEWORKS = ['documents-react.html', 'documents-vue.html', 'documents-svelte.html'];
const PAGES = ['vanilla.html', ...FRAMEWORKS, 'documents.html', ...DOCUMENTS_FRAMEWORKS];
const isDocuments = (name) => name.startsWith('documents');

async function open(page, name) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => ['error', 'warning'].includes(message.type()) && errors.push(message.text()));
  await page.goto(example(name));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await scope(page, name).locator('.config-editor__input').waitFor();
  return errors;
}

/** Where the editor being tested is: the active document's panel, or the page. */
function scope(page, name) {
  return isDocuments(name)
    ? page.locator('[role="tabpanel"]:not(.config-editor-documents__panel-hidden)')
    : page.locator('body');
}

const input = (page, name) => scope(page, name).locator('.config-editor__input');
const root = (page, name) => scope(page, name).locator('.config-editor').first();

/** Changes some settings, keeping the others, the way each page's host does. */
async function setSettings(page, name, partial) {
  await page.evaluate(
    ([name, partial]) => {
      if (name === 'vanilla.html') window.editor.update(partial);
      else if (!name.startsWith('documents')) window.setExtraProps((props) => ({ ...props, ...partial }));
      else if (name === 'documents.html') window.documentsView.update(partial);
      else window.setViewSettings((settings) => ({ ...settings, ...partial }));
    },
    [name, partial],
  );
  // React applies props on its next render: wait until the instance has them.
  await expect
    .poll(() =>
      page.evaluate(
        ([name, partial]) => {
          const instance =
            !name.startsWith('documents') && name !== 'vanilla.html'
              ? window.editorHandle.current?.editor
              : name.startsWith('documents-')
                ? window.documentsHandle.current?.instance
                : window.editor ?? window.documentsView;
          const settings = instance?.settings ?? {};
          return Object.entries(partial).every(([key, value]) => JSON.stringify(settings[key]) === JSON.stringify(value));
        },
        [name, partial],
      ),
    )
    .toBe(true);
}

/** Marks the textarea, so a re-mounted editor (a new one) is told apart. */
const mark = (page, name) => input(page, name).evaluate((element) => (element.dataset.settingsMark = 'kept'));
const marked = (page, name) => input(page, name).evaluate((element) => element.dataset.settingsMark ?? null);
const caret = (page, name) => input(page, name).evaluate((element) => [element.selectionStart, element.selectionEnd]);

/** Types `text` on a new last line. */
async function typeAtEnd(page, name, text) {
  await input(page, name).click();
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Enter');
  await page.keyboard.type(text);
}

/** Undoes until the text is `wanted` (typing may be several undo steps). */
async function undoTo(page, name, wanted) {
  await input(page, name).focus();
  for (let step = 0; step < 8 && (await input(page, name).inputValue()) !== wanted; step += 1) {
    await page.keyboard.press('Control+z');
  }
  await expect(input(page, name)).toHaveValue(wanted);
}

/** Nothing appears within half a second. */
async function staysAbsent(locator) {
  await locator.page().waitForTimeout(500);
  await expect(locator).toHaveCount(0);
}

/**
 * What each setting shows while on and off, as a count of elements
 * (`present` when on) or a check of the root. Read after an edit that makes
 * a diff and a schema problem (`web: 5`).
 */
const SHOWN = {
  showToolbar: (page, name) => scope(page, name).locator('.config-editor__toolbar').count().then((count) => count > 0),
  showFooter: (page, name) => scope(page, name).locator('.config-editor__status').count().then((count) => count > 0),
  showDiagnostics: (page, name) => scope(page, name).locator('.config-editor__diagnostic-marker').count().then((count) => count > 0),
  showDiff: (page, name) => scope(page, name).locator('.config-editor__diff-marker').count().then((count) => count > 0),
  highlighting: (page, name) => scope(page, name).locator('.config-editor__key').count().then((count) => count > 0),
  folding: (page, name) => scope(page, name).locator('.config-editor__gutter-folding').count().then((count) => count > 0),
  styled: (page, name) =>
    page
      .locator(isDocuments(name) ? '.config-editor-documents > style' : '.config-editor > style')
      .count()
      .then((count) => count > 0),
};

for (const name of PAGES) {
  test.describe(`${name}: every setting switches live`, () => {
    test('the shown parts follow their settings, the editor is not mounted again', async ({ page }) => {
      const errors = await open(page, name);
      const before = await input(page, name).inputValue();
      await mark(page, name);
      await typeAtEnd(page, name, 'web: 5');
      const typed = await input(page, name).inputValue();
      const at = await caret(page, name);
      for (const [key, shown] of Object.entries(SHOWN)) {
        for (const on of [true, false, true]) {
          await setSettings(page, name, { [key]: on });
          await expect.poll(() => shown(page, name), { message: `${key}: ${on}` }).toBe(on);
        }
      }
      await setSettings(page, name, { theme: 'nord', colors: { key: '#123456' } });
      await expect(root(page, name)).toHaveAttribute('data-color-theme', 'nord');
      await expect(root(page, name)).toHaveAttribute('style', /--config-editor-key: #123456/);
      await setSettings(page, name, { theme: 'github-light', colors: {} });
      await expect(root(page, name)).toHaveAttribute('data-color-theme', 'github-light');
      await expect(root(page, name)).not.toHaveAttribute('style', /--config-editor-key/);

      expect(await marked(page, name)).toBe('kept');
      expect(await input(page, name).inputValue()).toBe(typed);
      expect(await caret(page, name)).toEqual(at);
      await undoTo(page, name, before);
      expect(errors).toEqual([]);
    });

    test('the keys and popups follow their settings, the editor is not mounted again', async ({ page }) => {
      const errors = await open(page, name);
      const before = await input(page, name).inputValue();
      await mark(page, name);
      const area = scope(page, name);
      const popupOpen = () =>
        area.locator('.config-editor__completions').getAttribute('class').then((names) => !names.includes('hidden'));

      // Find (Ctrl+F) and go to line (Ctrl+G).
      for (const [key, shortcut, widget] of [
        ['search', 'Control+f', '.config-editor__find'],
        ['goToLine', 'Control+g', '.config-editor__goto'],
      ]) {
        await setSettings(page, name, { [key]: false });
        await input(page, name).click();
        await page.keyboard.press(shortcut);
        await staysAbsent(area.locator(`${widget}:visible`));
        await setSettings(page, name, { [key]: true });
        await input(page, name).click();
        await page.keyboard.press(shortcut);
        await expect(area.locator(widget)).toBeVisible();
        await page.keyboard.press('Escape');
      }

      // Completions on Ctrl+Space, and while typing.
      await setSettings(page, name, { completions: false });
      await typeAtEnd(page, name, 'web');
      await page.keyboard.press('Control+Space');
      await page.waitForTimeout(400);
      expect(await popupOpen()).toBe(false);
      await setSettings(page, name, { completions: true, autoTriggerCompletions: false });
      await input(page, name).focus();
      await page.keyboard.type('s');
      await page.waitForTimeout(400);
      expect(await popupOpen()).toBe(false);
      await page.keyboard.press('Control+Space');
      await expect.poll(popupOpen).toBe(true);
      await page.keyboard.press('Escape');
      await setSettings(page, name, { autoTriggerCompletions: true });
      await input(page, name).focus();
      await page.keyboard.type('o');
      await expect.poll(popupOpen).toBe(true);
      await page.keyboard.press('Escape');

      // Auto-closing brackets.
      await setSettings(page, name, { autoClosing: false });
      await typeAtEnd(page, name, 'x: [');
      await expect(input(page, name)).toHaveValue(/x: \[$/);
      await setSettings(page, name, { autoClosing: true });
      await input(page, name).focus();
      await page.keyboard.type('[');
      await expect(input(page, name)).toHaveValue(/x: \[\[\]\]?$/);

      // Hover docs on the first line's key (`listen`).
      await undoTo(page, name, before);
      const hoverKey = async () => {
        const box = await input(page, name).boundingBox();
        const line = before.split('\n').findIndex((text) => text.startsWith('listen'));
        await page.mouse.move(box.x + 5, box.y + 5);
        const style = await input(page, name).evaluate((element) => {
          const computed = getComputedStyle(element);
          return { left: parseFloat(computed.paddingLeft), top: parseFloat(computed.paddingTop), size: parseFloat(computed.fontSize), height: parseFloat(computed.lineHeight) };
        });
        await page.mouse.move(box.x + style.left + 2.5 * style.size * 0.6, box.y + style.top + (line + 0.5) * style.height, { steps: 4 });
      };
      await setSettings(page, name, { hoverDocs: false });
      await hoverKey();
      await staysAbsent(area.locator('.config-editor__hover'));
      await setSettings(page, name, { hoverDocs: true });
      await hoverKey();
      await expect(area.locator('.config-editor__hover')).toHaveCount(1);

      expect(await marked(page, name)).toBe('kept');
      expect(errors).toEqual([]);
    });

    test('saveShortcut switches Ctrl+S live', async ({ page }) => {
      const errors = await open(page, name);
      const saves = () =>
        page.evaluate((name) => {
          if (name === 'vanilla.html') return window.saves.length;
          if (!name.startsWith('documents')) return Number(document.getElementById('saves').dataset.count);
          return (document.getElementById('log').textContent.match(/^save /gm) ?? []).length;
        }, name);
      await mark(page, name);
      await setSettings(page, name, { saveShortcut: false });
      await typeAtEnd(page, name, 'web: {}');
      await page.keyboard.press('Control+s');
      await page.waitForTimeout(300);
      expect(await saves()).toBe(0);
      await setSettings(page, name, { saveShortcut: true });
      await input(page, name).focus();
      await page.keyboard.press('Control+s');
      await expect.poll(saves).toBe(1);
      expect(await marked(page, name)).toBe('kept');
      expect(errors).toEqual([]);
    });
  });
}

for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORKS]) {
  test.describe(`${name}: a document's own settings and editor`, () => {
    test('a document\'s settings go over the view\'s and follow them', async ({ page }) => {
      const errors = await open(page, name);
      const panel = (id) => page.locator(`[role="tabpanel"][data-document-id="${id}"]`);
      const tab = (id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
      await mark(page, name);
      await page.evaluate(() => window.updateFile('limits', { settings: { showFooter: true, theme: 'dracula' } }));
      await setSettings(page, name, { theme: 'nord' });
      await tab('limits').click();
      await expect(panel('limits').locator('.config-editor__status')).toHaveCount(1);
      await expect(panel('limits').locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'dracula');
      // The view's settings still apply to what the document leaves out.
      await setSettings(page, name, { folding: false });
      await expect(panel('limits').locator('.config-editor__gutter-folding')).toHaveCount(0);
      await tab('main').click();
      await expect(panel('main').locator('.config-editor__status')).toHaveCount(0);
      await expect(panel('main').locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
      expect(await panel('main').locator('.config-editor__input').evaluate((element) => element.dataset.settingsMark)).toBe('kept');
      // Dropping them returns the document to the view's.
      await page.evaluate(() => window.updateFile('limits', { settings: undefined }));
      await tab('limits').click();
      await expect(panel('limits').locator('.config-editor__status')).toHaveCount(0);
      await expect(panel('limits').locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
      expect(errors).toEqual([]);
    });

    test('a document\'s host problems and read-only fields update live', async ({ page }) => {
      const errors = await open(page, name);
      const main = page.locator('[role="tabpanel"][data-document-id="main"]');
      await page.evaluate(() =>
        window.updateFile('main', { hostDiagnostics: [{ path: '/listen', message: 'port in use', source: 'server' }] }),
      );
      await expect(main.locator('.config-editor__diagnostic-marker')).not.toHaveCount(0);
      await page.evaluate(() => window.updateFile('main', { hostDiagnostics: [], readOnlyPaths: ['/listen'] }));
      await expect(main.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
      await main.locator('.config-editor__input').click();
      await main.locator('.config-editor__input').evaluate((element) => element.setSelectionRange(9, 9));
      await page.keyboard.type('9');
      await page.waitForTimeout(200);
      await expect(main.locator('.config-editor__input')).toHaveValue(/^listen: 7835\n/);
      expect(errors).toEqual([]);
    });

    test('JS reaches a document\'s editor', async ({ page }) => {
      const errors = await open(page, name);
      const call = (id, method, ...args) =>
        page.evaluate(
          ([name, id, method, args]) => {
            const editor =
              name === 'documents.html'
                ? window.documentsView.editor(id)
                : window.documentsHandle.current.editor(id);
            if (!editor) return 'none';
            const member = editor[method];
            return typeof member === 'function' ? member.apply(editor, args) : member;
          },
          [name, id, method, args],
        );
      expect(await call('nope', 'value')).toBe('none');
      expect(await call('main', 'value')).toContain('listen: 7835');
      expect(await call('main', 'unifiedDiff')).toBe('');
      expect((await call('main', 'settings')).folding).toBe(true);
      expect(await call('main', 'rangeOf', '/public_address')).toMatchObject({ startLine: 2, startColumn: 1 });
      expect(await call('main', 'currentPath')).not.toBeNull();
      // Not shown yet: its editor has not mounted, so it answers nothing.
      expect(await call('limits', 'rangeOf', '/limits')).toBeNull();
      expect(await call('limits', 'text')).toContain('max_pending = 512');
      await page.locator('[role="tab"][data-document-id="limits"]').click();
      await expect.poll(() => call('limits', 'rangeOf', '/limits/max_pending')).toMatchObject({ startLine: 2 });
      expect(await call('limits', 'fold', '/limits')).toBe(true);
      expect(await call('limits', 'foldedLines')).toEqual([{ startLine: 1, endLine: 2 }]);
      // Switching folding off opens every fold; nothing folds until it is on.
      await setSettings(page, name, { folding: false });
      await expect.poll(() => call('limits', 'foldedLines')).toEqual([]);
      expect(await call('limits', 'fold', '/limits')).toBe(false);
      await setSettings(page, name, { folding: true });
      await expect.poll(() => call('limits', 'fold', '/limits')).toBe(true);
      await call('limits', 'unfoldAll');
      expect(await call('limits', 'foldedLines')).toEqual([]);
      // `reveal` in the main document from while another one is shown.
      await page.locator('[role="tab"][data-document-id="main"]').click();
      expect(await call('main', 'reveal', '/public_address')).toBe(true);
      expect(await call('main', 'currentPath')).toMatchObject({ pointer: '/public_address' });
      expect(await call('main', 'isDirty')).toBe(false);
      expect(errors).toEqual([]);
    });
  });
}
