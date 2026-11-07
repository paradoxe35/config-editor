import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// `update`, the one way to change a mounted `ConfigEditor` or
// `ConfigDocuments` from JavaScript: a key left out or `undefined` stays,
// `null` removes a callback, clears a list or resets a value, settings
// merge key by key (and `strings` text by text), and a key the instance is
// built from throws, changing nothing (every key is checked before any
// applies). Build the packages first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

/** Opens the vanilla page and mounts a fresh editor of the package's main entry as `window.fresh`. */
async function open(browserPage, options = {}) {
  await browserPage.goto(page('vanilla.html'));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.evaluate(async (options) => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const element = document.body.appendChild(document.createElement('div'));
    element.id = 'fresh';
    window.changes = [];
    window.fresh = new ConfigEditor(element, {
      value: 'listen: 7835\n',
      onChange: (text) => window.changes.push(text),
      ...options,
    });
  }, options);
}

const fresh = (browserPage) => browserPage.locator('#fresh');
const freshInput = (browserPage) => fresh(browserPage).locator('.config-editor__input');

async function typeInFresh(browserPage, text) {
  await freshInput(browserPage).click();
  await browserPage.keyboard.press('Control+Home');
  await browserPage.keyboard.press('End');
  await browserPage.keyboard.type(text);
}

test.describe('ConfigEditor.update', () => {
  test('settings merge key by key; a key left out or undefined stays', async ({ page: browserPage }) => {
    await open(browserPage, { theme: 'nord', showFooter: true, folding: false });
    const settings = await browserPage.evaluate(() => {
      window.fresh.update({ showToolbar: true });
      window.fresh.update({ theme: undefined, folding: undefined, showDiff: undefined });
      const { theme, showFooter, folding, showToolbar, showDiff } = window.fresh.settings;
      return { theme, showFooter, folding, showToolbar, showDiff };
    });
    expect(settings).toEqual({ theme: 'nord', showFooter: true, folding: false, showToolbar: true, showDiff: true });
    await expect(fresh(browserPage).locator('.config-editor__toolbar')).toBeVisible();
    await expect(fresh(browserPage).locator('.config-editor__status')).toBeVisible();
  });

  test('a setting set to null returns to its default', async ({ page: browserPage }) => {
    await open(browserPage, { theme: 'nord', showFooter: true });
    await expect(fresh(browserPage).locator('.config-editor__status')).toBeVisible();
    const settings = await browserPage.evaluate(() => {
      window.fresh.update({ theme: null, showFooter: null });
      const { theme, showFooter } = window.fresh.settings;
      return { theme, showFooter };
    });
    expect(settings).toEqual({ theme: 'vscode-dark', showFooter: false });
    await expect(fresh(browserPage).locator('.config-editor__status')).toHaveCount(0);
  });

  test('strings merge text by text; null returns a text, or all of them, to the defaults', async ({ page: browserPage }) => {
    await open(browserPage, { showFooter: true, strings: { linesOne: '{count} Zeile', linesOther: '{count} Zeilen' } });
    const footer = fresh(browserPage).locator('.config-editor__status');
    await expect(footer).toContainText('2 Zeilen');
    const texts = await browserPage.evaluate(() => {
      const read = () => {
        const { linesOne, linesOther, schemaConnected } = window.fresh.settings.strings;
        return { linesOne, linesOther, schemaConnected };
      };
      window.fresh.update({ strings: { schemaConnected: 'Schema verbunden' } });
      const merged = read();
      window.fresh.update({ strings: { linesOne: null } });
      const one = read();
      window.fresh.update({ strings: null });
      return { merged, one, cleared: read() };
    });
    expect(texts.merged).toEqual({ linesOne: '{count} Zeile', linesOther: '{count} Zeilen', schemaConnected: 'Schema verbunden' });
    expect(texts.one).toEqual({ linesOne: '{count} line', linesOther: '{count} Zeilen', schemaConnected: 'Schema verbunden' });
    expect(texts.cleared).toEqual({ linesOne: '{count} line', linesOther: '{count} lines', schemaConnected: 'Schema connected' });
    await expect(footer).toContainText('2 lines');
  });

  test('a callback set to null is removed, undefined keeps it, a function replaces it', async ({ page: browserPage }) => {
    await open(browserPage);
    await browserPage.evaluate(() => window.fresh.update({ onChange: undefined }));
    await typeInFresh(browserPage, '1');
    await expect.poll(() => browserPage.evaluate(() => window.changes.length)).toBe(1);
    await browserPage.evaluate(() => window.fresh.update({ onChange: null }));
    await typeInFresh(browserPage, '2');
    await browserPage.waitForTimeout(200);
    expect(await browserPage.evaluate(() => window.changes.length)).toBe(1);
    await browserPage.evaluate(() => window.fresh.update({ onChange: (text) => window.changes.push(`new ${text}`) }));
    await typeInFresh(browserPage, '3');
    await expect.poll(() => browserPage.evaluate(() => window.changes.at(-1))).toBe('new listen: 7835123\n');
  });

  test('null clears the lists and turns editAs and readOnly off', async ({ page: browserPage }) => {
    await open(browserPage, {
      editAs: 'json',
      readOnly: true,
      readOnlyPaths: ['/listen'],
      hostDiagnostics: [{ path: '/listen', message: 'taken' }],
    });
    await expect(freshInput(browserPage)).toHaveValue(/"listen": 7835/);
    await expect(fresh(browserPage).locator('.config-editor__diagnostic-marker-host')).toHaveCount(1);
    await browserPage.evaluate(() =>
      window.fresh.update({ editAs: null, readOnly: null, readOnlyPaths: null, hostDiagnostics: null, readOnlyRanges: null }),
    );
    await expect(freshInput(browserPage)).toHaveValue('listen: 7835\n');
    await expect(fresh(browserPage).locator('.config-editor__diagnostic-marker-host')).toHaveCount(0);
    // Editable again, the field included.
    await typeInFresh(browserPage, '9');
    await expect.poll(() => browserPage.evaluate(() => window.fresh.value)).toBe('listen: 78359\n');
  });

  test('value, baseline and readOnly apply in one call, the caret and history kept', async ({ page: browserPage }) => {
    await open(browserPage);
    const result = await browserPage.evaluate(() => {
      window.fresh.update({ value: 'listen: 1\n', baseline: 'listen: 0\n', readOnly: true });
      return { value: window.fresh.value, dirty: window.fresh.isDirty, diff: window.fresh.unifiedDiff(0) };
    });
    expect(result.value).toBe('listen: 1\n');
    expect(result.dirty).toBe(true);
    expect(result.diff).toContain('-listen: 0');
    await expect(freshInput(browserPage)).toHaveAttribute('aria-readonly', 'true');
  });

  test('the keys the editor is built from throw, and nothing applies', async ({ page: browserPage }) => {
    await open(browserPage);
    const result = await browserPage.evaluate(() => {
      const caught = (options) => {
        try {
          window.fresh.update(options);
          return null;
        } catch (error) {
          return error.message;
        }
      };
      return {
        language: caught({ language: 'json', readOnly: true }),
        schema: caught({ schema: '{}' }),
        fileName: caught({ fileName: 'a.yaml' }),
        className: caught({ className: 'x' }),
        secrets: caught({ secrets: {} }),
        // A bad value stops the whole call: the valid `readOnly` waits too.
        atomic: caught({ readOnly: true, hostDiagnostics: 'bad' }),
        undefinedIsFine: caught({ language: undefined, schema: undefined }),
        readOnly: document.querySelector('#fresh .config-editor__input').getAttribute('aria-readonly'),
      };
    });
    for (const key of ['language', 'schema', 'fileName', 'className', 'secrets']) {
      expect(result[key], key).toContain(`${key} cannot change on a mounted ConfigEditor`);
      expect(result[key], key).toContain('Create a new ConfigEditor');
    }
    expect(result.atomic).toContain('hostDiagnostics');
    expect(result.undefinedIsFine).toBeNull();
    expect(result.readOnly).toBe('false');
  });
});

test.describe('ConfigDocuments.update', () => {
  async function openDocuments(browserPage) {
    await browserPage.goto(page('documents.html'));
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await browserPage.locator('[role="tab"]').first().waitFor();
  }

  test('settings merge under each document\'s own; activeId shows a document, leaving the focus', async ({ page: browserPage }) => {
    await openDocuments(browserPage);
    await browserPage.locator('#save-all').focus();
    const result = await browserPage.evaluate(() => {
      const view = window.documentsView;
      view.update({ theme: 'nord' });
      view.update({ showFooter: false });
      window.updateFile('limits', { settings: { theme: 'dracula' } });
      view.update({ activeId: 'limits' });
      return {
        view: [view.settings.theme, view.settings.showFooter],
        main: view.editor('main').settings.theme,
        limits: [view.editor('limits').settings.theme, view.editor('limits').settings.showFooter],
        active: view.activeId,
        focused: document.activeElement?.id,
      };
    });
    expect(result).toEqual({ view: ['nord', false], main: 'nord', limits: ['dracula', false], active: 'limits', focused: 'save-all' });
    await expect(browserPage.locator('[role="tab"][aria-selected="true"]')).toContainText('limits.toml');
  });

  test('a document\'s editor is an EditorView: value, settings, unifiedDiff', async ({ page: browserPage }) => {
    await openDocuments(browserPage);
    await browserPage.locator('.config-editor__input').first().click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('# note');
    const result = await browserPage.evaluate(() => {
      const main = window.documentsView.editor('main');
      return { value: main.value, diff: main.unifiedDiff(0), dirty: main.isDirty, theme: main.settings.theme, notShown: window.documentsView.editor('limits').value };
    });
    expect(result.value).toBe('listen: 7835\npublic_address: tunnel.example.com\n# note');
    expect(result.diff).toContain('+# note');
    expect(result.dirty).toBe(true);
    expect(result.theme).toBe('vscode-dark');
    expect(result.notShown).toBe('[limits]\nmax_pending = 512\n');
  });

  test('className, label and an onClose the view was built without throw', async ({ page: browserPage }) => {
    await openDocuments(browserPage);
    const result = await browserPage.evaluate(async () => {
      const { ConfigDocuments } = await import('../config-editor/index.js');
      const caught = (view, options) => {
        try {
          view.update(options);
          return null;
        } catch (error) {
          return error.message;
        }
      };
      const bare = new ConfigDocuments(document.body.appendChild(document.createElement('div')), {
        documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n' }],
      });
      return {
        className: caught(window.documentsView, { className: 'x' }),
        label: caught(window.documentsView, { label: 'Files' }),
        // The page's view has close buttons: its onClose can be replaced and removed.
        replaceClose: caught(window.documentsView, { onClose: () => {} }),
        removeClose: caught(window.documentsView, { onClose: null }),
        addClose: caught(bare, { onClose: () => {} }),
        notObject: caught(bare, 'documents'),
      };
    });
    expect(result.className).toContain('className cannot change on a mounted ConfigDocuments');
    expect(result.label).toContain('label cannot change on a mounted ConfigDocuments');
    expect(result.replaceClose).toBeNull();
    expect(result.removeClose).toBeNull();
    expect(result.addClose).toContain('onClose cannot be added');
    expect(result.notObject).toContain('update takes an object of options');
  });
});
