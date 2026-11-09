import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 27 (host API). A host echoing `value` late (a store that
// re-renders after a delay) never rolls typing back, in a ConfigEditor and a
// ConfigDocuments with one or several documents, while a text of the host's
// own still wins; `destroy()` works from a callback a method of the same
// instance fired, and a document selected in answer to a selection stands;
// `onDirtyChange` and `onChangeSummary` given later hear the state at once;
// the READMEs' React snippets import the hooks they use; and the read-only
// notice wraps a long word.

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

// How late the host hands each text back, and how fast the user types.
const ECHO_DELAYS = [0, 16, 60, 150];
const KEY_DELAYS = [20, 40];
const TYPED = 'abcdefghij';
const HOSTS = ['editor', 'one document', 'two documents'];

/** Mounts `host` in the page, its host echoing every `onChange` text
 * `delay` ms late, the caret after `name: x`. */
async function mountEchoing(page, host, delay) {
  await page.evaluate(
    async ([host, delay]) => {
      document.querySelectorAll('[data-echo]').forEach((element) => element.remove());
      const element = window.mountPoint();
      element.dataset.echo = '';
      const later = (apply) => (delay === 0 ? Promise.resolve().then(apply) : setTimeout(apply, delay));
      if (host === 'editor') {
        window.store = { a: 'name: x\n' };
        const editor = new window.module.ConfigEditor(element, {
          value: window.store.a,
          onChange: (text) =>
            later(() => {
              window.store.a = text;
              editor.update({ value: text });
            }),
        });
        window.echoing = editor;
        window.textOf = () => editor.value;
        await window.pause(300);
        editor.revealRange({ start: 7, end: 7 });
        return;
      }
      window.store = host === 'one document' ? { a: 'name: x\n' } : { a: 'name: x\n', b: 'other: 1\n' };
      const list = () => Object.entries(window.store).map(([id, value]) => ({ id, fileName: `${id}.yaml`, value }));
      const view = new window.module.ConfigDocuments(element, {
        documents: list(),
        onChange: (id, text) =>
          later(() => {
            window.store[id] = text;
            view.update({ documents: list() });
          }),
      });
      window.echoing = view;
      window.textOf = () => view.editor('a').value;
      await window.pause(400);
      view.editor('a').revealRange({ start: 7, end: 7 });
    },
    [host, delay],
  );
}

test.describe('QA round 27: host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a late echo of value never loses what was typed (every host, echo delay and typing speed)', async ({ page }) => {
    test.slow();
    await open(page);
    const failures = [];
    for (const host of HOSTS) {
      for (const delay of ECHO_DELAYS) {
        for (const keyDelay of KEY_DELAYS) {
          await mountEchoing(page, host, delay);
          await page.keyboard.type(TYPED, { delay: keyDelay });
          await page.waitForTimeout(delay + 500);
          const { text, store } = await page.evaluate(() => ({ text: window.textOf(), store: window.store }));
          const expected = `name: x${TYPED}\n`;
          if (text !== expected || store.a !== expected || (store.b !== undefined && store.b !== 'other: 1\n')) {
            failures.push({ host, delay, keyDelay, text, store });
          }
        }
      }
    }
    expect(failures).toEqual([]);
    expect(page.errors).toEqual([]);
  });

  test("a host text of its own still wins over echoes on their way, and the host hears the document again", async ({ page }) => {
    await open(page);
    for (const host of HOSTS) {
      await mountEchoing(page, host, 150);
      await page.keyboard.type('abc', { delay: 20 });
      // The host loads a file while those echoes are on their way.
      await page.evaluate(() => {
        window.store.a = 'reset: 1\n';
        if (window.echoing.editor) {
          const list = Object.entries(window.store).map(([id, value]) => ({ id, fileName: `${id}.yaml`, value }));
          window.echoing.update({ documents: list });
        } else {
          window.echoing.update({ value: 'reset: 1\n' });
        }
      });
      await page.waitForTimeout(800);
      const { text, store } = await page.evaluate(() => ({ text: window.textOf(), store: window.store }));
      expect({ host, text, store: store.a }).toEqual({ host, text: 'reset: 1\n', store: 'reset: 1\n' });
    }
    expect(page.errors).toEqual([]);
  });

  test('a host that does not echo reverts with update, and replaceValue reverts while echoes are on their way', async ({ page }) => {
    await open(page);
    // No echo: onChange only saves; a revert to a text it gave is meant.
    await page.evaluate(async () => {
      document.querySelectorAll('[data-echo]').forEach((element) => element.remove());
      const element = window.mountPoint();
      element.dataset.echo = '';
      window.given = [];
      window.echoing = new window.module.ConfigEditor(element, { value: 'name: x\n', onChange: (text) => window.given.push(text) });
      await window.pause(300);
      window.echoing.revealRange({ start: 7, end: 7 });
    });
    await page.keyboard.type('abc', { delay: 30 });
    await page.waitForTimeout(1200);
    await page.keyboard.type('d', { delay: 30 });
    const reverted = await page.evaluate(() => {
      window.echoing.update({ value: window.given[2] });
      return window.echoing.value;
    });
    expect(reverted).toBe('name: xabc\n');
    // A late echo host: replaceValue goes back to a text it was handed, now.
    await mountEchoing(page, 'editor', 150);
    await page.keyboard.type('ab', { delay: 20 });
    const replaced = await page.evaluate(() => {
      window.echoing.replaceValue('name: xa\n');
      return window.echoing.value;
    });
    expect(replaced).toBe('name: xa\n');
    await page.waitForTimeout(600);
    expect(await page.evaluate(() => ({ text: window.echoing.value, store: window.store.a }))).toEqual({
      text: 'name: xa\n',
      store: 'name: xa\n',
    });
  });

  test('destroy() works from a callback a method of the same instance fired', async ({ page }) => {
    await open(page);
    const results = await page.evaluate(async () => {
      const documents = () => [
        { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', baseline: 'a: 0\n' },
        { id: 'b', fileName: 'b.yaml', value: 'b: 1\n' },
      ];
      const cases = [
        ['ConfigDocuments saveAll() -> onSave', 'documents', 'onSave', (view) => view.saveAll()],
        ['ConfigDocuments save(id) -> onSave', 'documents', 'onSave', (view) => view.save('a')],
        ['ConfigDocuments select() -> onSelect', 'documents', 'onSelect', (view) => view.select('b')],
        ['ConfigDocuments update({ activeId }) -> onSelect', 'documents', 'onSelect', (view) => view.update({ activeId: 'b' })],
        ['ConfigDocuments update({ documents }) -> onSelect', 'documents', 'onSelect', (view) => view.update({ documents: [documents()[1]] })],
        ['ConfigDocuments editAs(id) -> onEditAsChange', 'documents', 'onEditAsChange', (view) => view.editAs('a', 'json')],
        ['ConfigEditor save() -> onSave', 'editor', 'onSave', (editor) => editor.save()],
        ['ConfigEditor update({ editAs }) -> onEditAsChange', 'editor', 'onEditAsChange', (editor) => editor.update({ editAs: 'json' })],
        ['ConfigEditor update({ baseline }) -> onDirtyChange', 'editor', 'onDirtyChange', (editor) => editor.update({ baseline: 'a: 1\n' })],
      ];
      const results = [];
      for (const [name, kind, callback, act] of cases) {
        const element = window.mountPoint();
        let instance;
        let armed = false;
        let fired = false;
        let error = null;
        const destroy = () => {
          if (!armed || fired) return;
          fired = true;
          try {
            instance.destroy();
          } catch (thrown) {
            error = String(thrown.message ?? thrown);
          }
        };
        instance =
          kind === 'documents'
            ? new window.module.ConfigDocuments(element, { documents: documents(), [callback]: destroy })
            : new window.module.ConfigEditor(element, { value: 'a: 1\n', baseline: 'a: 0\n', [callback]: destroy });
        await window.pause(300);
        armed = true;
        try {
          act(instance);
        } catch (thrown) {
          error = `${error ?? ''} act: ${thrown.message ?? thrown}`;
        }
        await window.pause(100);
        results.push({ name, fired, error, left: element.children.length });
        element.remove();
      }
      return results;
    });
    for (const result of results) expect(result).toEqual({ name: result.name, fired: true, error: null, left: 0 });
  });

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

  test('onDirtyChange and onChangeSummary given later hear the state at once', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\nb: 2\n', baseline: 'a: 0\nb: 2\n' });
      const view = new window.module.ConfigDocuments(window.mountPoint(), {
        documents: [
          { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', baseline: 'a: 0\n' },
          { id: 'b', fileName: 'b.yaml', value: 'b: 1\n' },
        ],
      });
      await window.pause(400);
      const editorHeard = [];
      editor.update({
        onDirtyChange: (dirty) => editorHeard.push(['dirty', dirty]),
        onChangeSummary: (summary) => editorHeard.push(['summary', summary]),
      });
      const documentsHeard = [];
      view.update({ onDirtyChange: (id, dirty) => documentsHeard.push(['dirty', id, dirty]) });
      return { editorHeard, documentsHeard };
    });
    expect(result.editorHeard).toEqual([
      ['dirty', true],
      ['summary', { added: 0, removed: 0, modified: 1, hunks: 1 }],
    ]);
    expect(result.documentsHeard).toEqual([
      ['dirty', 'a', true],
      ['dirty', 'b', false],
    ]);
  });

  test('the read-only notice wraps a long word or URL inside a phone screen', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 700 });
    await open(page);
    const reason = `Managed_by_the_operator_https://example.com/${'very/long/path/'.repeat(8)} and more words after it.`;
    await page.evaluate((reason) => {
      document.querySelectorAll('header, #editor, pre').forEach((element) => element.remove());
      const element = window.mountPoint();
      new window.module.ConfigEditor(element, { value: 'k: 1\n'.repeat(3), readOnly: true, readOnlyReason: reason });
    }, reason);
    await page.waitForTimeout(300);
    await page.locator('.config-editor__input').first().click();
    await page.keyboard.type('x');
    const notice = page.locator('.config-editor__read-only-notice');
    await expect(notice).toBeVisible();
    const box = await notice.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return { left: rect.left, right: rect.right, overflows: element.scrollWidth > element.clientWidth };
    });
    expect(box.overflows).toBe(false);
    expect(box.left).toBeGreaterThanOrEqual(0);
    expect(box.right).toBeLessThanOrEqual(375);
  });
});

// The hooks a README's React snippet calls are imported in it (QA 27: the
// root README's snippet called useState without importing it).
test("the READMEs' React snippets import the hooks they call", () => {
  const readmes = [
    'README.md',
    'js/README.md',
    'js/config-editor/README.md',
    'js/config-editor-react/README.md',
    'crates/config-editor-leptos/README.md',
  ];
  const missing = [];
  let snippets = 0;
  for (const readme of readmes) {
    const text = readFileSync(join(root, readme), 'utf8');
    for (const [, code] of text.matchAll(/```(?:tsx|jsx)\n([\s\S]*?)```/g)) {
      snippets += 1;
      const imported = new Set(
        [...code.matchAll(/import\s+(?:\w+\s*,\s*)?\{([^}]*)\}\s+from\s+"[^"]+"/g)].flatMap((match) =>
          match[1].split(',').map((name) => name.trim().replace(/^type /, '')),
        ),
      );
      for (const [, hook] of code.matchAll(/(?<![.\w])(use[A-Z]\w*)\(/g)) {
        // A hook defined in the snippet itself.
        if (new RegExp(`function ${hook}\\b|const ${hook}\\b`).test(code)) continue;
        if (!imported.has(hook) && !/^React\./.test(hook)) missing.push(`${readme}: ${hook}`);
      }
    }
  }
  expect(snippets).toBeGreaterThan(3);
  expect([...new Set(missing)]).toEqual([]);
});
