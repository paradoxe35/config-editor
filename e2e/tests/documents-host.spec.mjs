import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor } from './helpers.mjs';

// The ConfigDocuments host API: update, activeId, editor(id), documents never shown, selection answers.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const input = (page) => page.locator('#qa .config-editor__input');

const value = (page) => input(page).inputValue();

const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;

async function open(page) {
  page.errors = [];
  page.warnings = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
    if (message.type() === 'warning') page.warnings.push(message.text());
  });
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    window.bindings = await import('../config-editor/binding.js');
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.mountPoint = () => {
      const element = document.createElement('div');
      document.body.prepend(element);
      return element;
    };
    // Replaces the text on screen of `element`'s editor as typing does.
    window.typeAll = (element, text) => {
      const input = element.querySelector('.config-editor__input');
      input.focus();
      input.setSelectionRange(0, input.value.length);
      document.execCommand('insertText', false, text);
    };
  });
}

test.describe('host inputs and data', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  for (const name of ['documents.html', 'documents-react.html']) {
    test(`${name}: editor(id) of an unknown document is null`, async ({ page }) => {
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const result = await page.evaluate(() => {
        const view = window.documentsView ?? window.documentsHandle?.current;
        return view ? view.editor('no-such-document') : 'no view';
      });
      expect(result).toBeNull();
    });
  }
});

test.describe('ConfigDocuments.update', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  test('moves activeId to the new list before it returns', async ({ page }) => {
    await page.goto(example('documents.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(() => {
      const view = window.documentsView;
      view.update({ documents: [{ id: 'a', fileName: 'a.yaml', value: 'x: 1\n' }] });
      const first = view.activeId;
      view.update({ documents: [{ id: 'z', fileName: 'z.yaml', value: 'y: 2\n' }] });
      return [first, view.activeId];
    });
    expect(seen).toEqual(['a', 'z']);
    await expect(page.locator('[role=tab][aria-selected=true]')).toHaveText(/z\.yaml/);
  });
});

test.describe('ConfigDocuments', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('refuses an unknown activeId; an EditorView stays with its own document', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const { ConfigDocuments } = await import('../config-editor/index.js');
      window.editor?.destroy();
      const host = document.createElement('div');
      document.body.prepend(host);
      const view = new ConfigDocuments(host, {
        documents: [
          { id: 'main', fileName: 'main.yaml', value: 'a: 1\n' },
          { id: 'z', fileName: 'z.yaml', value: 'z: 1\n' },
        ],
        onSave: () => {},
      });
      const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
      await wait();
      let unknown = null;
      try {
        view.update({ activeId: 'nope' });
      } catch (error) {
        unknown = error.message;
      }
      const active = view.activeId;
      const old = view.editor('main');
      const before = old.value;
      view.update({ documents: [{ id: 'z', fileName: 'z.yaml', value: 'z: 1\n' }] });
      await wait();
      const removed = { value: old.value, text: old.text, save: old.save(), dirty: old.isDirty, path: old.currentPath };
      view.update({
        documents: [
          { id: 'z', fileName: 'z.yaml', value: 'z: 1\n' },
          { id: 'main', fileName: 'main.yaml', value: 'again: 2\n', baseline: 'x: 0\n' },
        ],
      });
      await wait();
      const readded = { value: old.value, text: old.text, save: old.save(), dirty: old.isDirty, fresh: view.editor('main').value };
      return { unknown, active, before, removed, readded };
    });
    expect(result.unknown).toMatch(/activeId: no document has the id "nope"/);
    expect(result.active).toBe('main');
    expect(result.before).toBe('a: 1\n');
    expect(result.removed).toEqual({ value: null, text: null, save: false, dirty: false, path: null });
    expect(result.readded).toEqual({ value: null, text: null, save: false, dirty: false, fresh: 'again: 2\n' });
  });
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  // A document never shown answered `reveal` right after `select` (or an
  // `activeId` update) with false: its editor mounted a moment later.
  for (const how of ['select', 'update']) {
    for (const call of ['reveal', 'revealRange', 'rangeOf']) {
      test(`${call} right after ${how} on a document never shown works`, async ({ page }) => {
        await page.goto(example('vanilla.html'));
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        const answer = await page.evaluate(
          async ([how, call, big]) => {
            const module = await import('../config-editor/index.js');
            const host = document.createElement('div');
            host.id = 'sized';
            document.body.prepend(host);
            const view = new module.ConfigDocuments(host, {
              documents: [
                { id: 'a', fileName: 'a.yaml', value: big },
                { id: 'b', fileName: 'b.yaml', value: big },
              ],
            });
            window.sized = view;
            await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
            if (how === 'select') view.select('b');
            else view.update({ activeId: 'b' });
            const editor = view.editor('b');
            if (call === 'reveal') return editor.reveal('/k150');
            const range = editor.rangeOf('/k150');
            if (call === 'rangeOf') return range !== null;
            return editor.revealRange(range);
          },
          [how, call, big],
        );
        expect(answer).toBe(true);
        if (call === 'rangeOf') return;
        const b = page.locator('#sized [role="tabpanel"][data-document-id="b"] .config-editor__input');
        await expect(b).toBeFocused();
        const caretLine = await b.evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
        expect(caretLine).toBe(151);
      });
    }
  }
  // The host showing another document while the user typed in one dropped
  // the focus to the page.
  test('the host showing another document keeps the focus in the view', async ({ page }) => {
    await page.goto(example('documents.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const main = page.locator('[role="tabpanel"][data-document-id="main"] .config-editor__input');
    await main.focus();
    await page.evaluate(() => window.documentsView.update({ activeId: 'limits' }));
    await expect(page.locator('[role="tabpanel"][data-document-id="limits"] .config-editor__input')).toBeFocused();
    // From outside the view, the focus stays where it was.
    await page.evaluate(() => {
      const outside = document.createElement('button');
      outside.id = 'outside';
      document.body.append(outside);
      outside.focus();
      window.documentsView.update({ activeId: 'main' });
    });
    await page.waitForTimeout(200);
    await expect(page.locator('#outside')).toBeFocused();
  });
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('the documents list is checked as an editor is', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const element = window.mountPoint();
      const documents = (extra) => [{ id: 'a', fileName: 'a.yaml', value: 'k: 1\n' }, { id: 'b', fileName: 'b.yaml', value: 'm: 2\n', ...extra }];
      const out = {
        nullValue: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ value: null }) })),
        unknownActive: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents(), activeId: 'zz' })),
        secrets: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ secrets: 5 }) })),
        secretsCheck: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ secrets: { isPlaceholder: 5 } }) })),
      };
      const view = new window.module.ConfigDocuments(element, { documents: documents(), activeId: 'b' });
      await window.pause(100);
      out.updateNull = attempt(() => view.update({ documents: documents({ value: null }) }));
      const [first, second] = documents();
      delete second.value;
      out.leftOut = attempt(() => view.update({ documents: [first, second] }));
      await window.pause(100);
      out.kept = view.editor('b').value;
      out.ignored = attempt(() => view.update({ documents: documents({ editAsText: 'm = 2\n' }) }));
      return out;
    });
    expect(result.nullValue).toContain('documents[1].value must be a string');
    expect(result.unknownActive).toContain('activeId: no document has the id "zz"');
    expect(result.secrets).toContain('documents[1].secrets must be an object');
    expect(result.secretsCheck).toContain('documents[1].secrets.isPlaceholder must be a function');
    expect(result.updateNull).toContain('documents[1].value must be a string');
    expect(result.leftOut).toBe('no error');
    expect(result.kept).toBe('m: 2\n');
    expect(result.ignored).toBe('no error');
    expect(page.warnings.filter((text) => text.includes('documents[1].editAsText is ignored'))).toHaveLength(1);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  async function open(page) {
    page.errors = [];
    page.on('console', (message) => {
      if (message.type() === 'error') page.errors.push(message.text());
    });
    page.on('pageerror', (error) => page.errors.push(String(error)));
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.editor?.destroy();
      window.module = await import('../config-editor/index.js');
      window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
      window.mountPoint = () => {
        const element = document.createElement('div');
        document.body.prepend(element);
        return element;
      };
    });
  }


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('a document selected in answer to a selection stands, its editor focused', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const selected = [];
        let view;
        view = new window.module.ConfigDocuments(window.mountPoint(), {
          documents: [
            { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' },
            { id: 'b', fileName: 'b.yaml', value: 'b: 1\n' },
          ],
          onSelect: (id) => {
            selected.push(id);
            if (id === 'b') view.select('a');
          },
        });
        await window.pause(300);
        view.select('b');
        await window.pause(300);
        const focused = document.activeElement?.closest('[role=tabpanel]');
        return { selected, active: view.activeId, focusedShowsA: !!focused && !focused.hidden && focused.textContent.includes('a: 1') };
      });
      expect(result).toEqual({ selected: ['b', 'a'], active: 'a', focusedShowsA: true });
    });
  });
});
