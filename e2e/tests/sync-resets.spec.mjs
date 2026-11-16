import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// setValue and replaceValue resets with echoes on their way: the editor ends where the host does.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

function listen(page) {
  page.errors = [];
  page.warnings = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
    if (message.type() === 'warning') page.warnings.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page) {
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.module = await import('../config-editor/index.js');
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.mountPoint = () => {
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '240px';
      document.body.prepend(element);
      return element;
    };
  });
}

// --- a reset while echoes are on their way (replaceValue; the text --
// given last, given again in an update, is a render, no news). ------------------

test.describe('a reset to the text the host gave last', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('with echoes on their way applies at once, and the editor ends where the host does', async ({ page }) => {
    test.slow();
    await open(page);
    const failures = [];
    for (const kind of ['slow', 'latest', 'random']) {
      for (const drops of [true, false]) {
        await page.evaluate(
          async ([kind, drops]) => {
            const original = 'name: x\n';
            window.store = original;
            // A host that drops the answers to what it sent before a reset
            // (`drops`), or one that takes every answer.
            let version = 0;
            let timer = null;
            let pending = null;
            const element = window.mountPoint();
            const editor = new window.module.ConfigEditor(element, {
              value: original,
              echoes: true,
              onChange: (text) => {
                const sent = version;
                const apply = (text) => {
                  if (drops && sent !== version) return;
                  window.store = text;
                  editor.update({ value: text });
                };
                if (kind === 'slow') setTimeout(() => apply(text), 600);
                if (kind === 'random') setTimeout(() => apply(text), Math.random() * 200);
                if (kind === 'latest') {
                  pending = text;
                  clearTimeout(timer);
                  timer = setTimeout(() => apply(pending), 150);
                }
              },
            });
            await window.pause(150);
            editor.revealRange({ start: 7, end: 7 });
            window.editorUnderTest = editor;
            window.reset = () => {
              version += 1;
              window.store = original;
              editor.replaceValue(original);
              return editor.value;
            };
          },
          [kind, drops],
        );
        await page.locator('[data-host] .config-editor__input').focus();
        await page.keyboard.type('abc', { delay: 20 });
        const afterReset = await page.evaluate(() => window.reset());
        await page.waitForTimeout(30);
        await page.evaluate(() => window.editorUnderTest.revealRange({ start: 7, end: 7 }));
        await page.keyboard.type('Q', { delay: 20 });
        await page.waitForTimeout(1500);
        const end = await page.evaluate(() => ({ text: window.editorUnderTest.value, store: window.store }));
        const expected = drops ? { text: 'name: xQ\n', store: 'name: xQ\n' } : { text: end.store, store: end.store };
        if (afterReset !== 'name: x\n' || JSON.stringify(end) !== JSON.stringify(expected)) {
          failures.push({ kind, drops, afterReset, ...end });
        }
      }
    }
    expect(failures).toEqual([]);
    expect(page.errors).toEqual([]);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  function listen(page) {
    page.errors = [];
    page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
    page.on('pageerror', (error) => page.errors.push(String(error)));
  }

  async function open(page, name, query = '') {
    listen(page);
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').first().waitFor();
  }

  /** Mounts a documents view of `documents` with `echoes: true` in the
   * vanilla page, its `onChange` texts kept in `window.changes`. */
  async function mountDocuments(page, documents, options = {}) {
    await open(page, 'documents.html');
    await page.evaluate(
      async ([documents, options]) => {
        window.documentsView.destroy();
        const module = await import('../config-editor/index.js');
        const host = document.createElement('div');
        host.id = 'probe';
        host.style.height = options.height ?? '420px';
        document.body.prepend(host);
        window.changes = [];
        window.view = new module.ConfigDocuments(host, {
          documents,
          echoes: true,
          onChange: (id, value) => window.changes.push({ id, value }),
          onSave: () => {},
        });
      },
      [documents, options],
    );
  }


  test.describe('ConfigDocuments with echoes: replaceValue, then the list as before', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('a list given again as before (another document edited) keeps the text replaced', async ({ page }) => {
      const files = [
        { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' },
        { id: 'b', fileName: 'b.yaml', value: 'b: 2\n' },
      ];
      await mountDocuments(page, files);
      const after = await page.evaluate(async (files) => {
        window.view.replaceValue('a', 'a: 9\n');
        // The host's render for another document's edit, before it took the
        // replaced text.
        window.view.update({ documents: files.map((file) => (file.id === 'b' ? { ...file, value: 'b: 3\n' } : file)) });
        await new Promise((resolve) => setTimeout(resolve, 600));
        return { a: window.view.editor('a').value, b: window.view.editor('b').value };
      }, files);
      expect(after).toEqual({ a: 'a: 9\n', b: 'b: 3\n' });
      expect(page.errors).toEqual([]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // A documents view's reset (the handle's `setValue`,
  // `ConfigDocuments.replaceValue`) then typing, with the parent applying
  // every text late and in any order, the reset's own too: the typing stays.
  // Before, the reset's text listed after the typing was taken for the
  // host's own and rolled the typing back.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  function listen(page) {
    page.errors = [];
    page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
    page.on('pageerror', (error) => page.errors.push(String(error)));
  }

  async function open(page, name, query = '') {
    listen(page);
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').first().waitFor();
  }

  /** The order the texts held after the typing before the reset come back
   * in: `resets` are the resets' reports, `typed` what was typed after. */
  const ORDERS = {
    'in order': ({ resets, typed }) => [...resets, ...typed],
    'reset last': ({ resets, typed }) => [...typed, ...resets],
    'reset in the middle': ({ resets, typed }) => [...typed.slice(0, 2), ...resets, ...typed.slice(2)],
    backwards: ({ resets, typed }) => [...resets, ...typed].reverse(),
  };

  /** The cases: the order, how many resets, whether the echoes of what was
   * typed before are still held, whether the tabs are switched in between. */
  const CASES = [
    ...Object.keys(ORDERS).map((order) => ({ order, resets: 1, before: 'back', switches: false })),
    { order: 'reset last', resets: 1, before: 'held', switches: false },
    { order: 'reset in the middle', resets: 1, before: 'held', switches: false },
    { order: 'reset last', resets: 2, before: 'back', switches: false },
    { order: 'backwards', resets: 2, before: 'held', switches: false },
    { order: 'reset last', resets: 1, before: 'back', switches: true },
  ];

  const describe = ({ order, resets, before, switches }) =>
    `${resets} reset${resets > 1 ? 's' : ''}, typed echoes ${before}, ${order}${switches ? ', tabs switched' : ''}`;

  /** Releases the held texts `order` one at a time, then what the editor
   * sends again (at once, or after the pause) until nothing more comes. */
  async function release(page, order, count) {
    for (const index of order) {
      await page.evaluate((index) => window.releaseEchoes([index]), index);
      await page.waitForTimeout(5);
    }
    for (let released = count; ; ) {
      await page.waitForTimeout(400);
      const now = await page.evaluate(() => window.heldEchoes());
      if (now === released) break;
      expect(now - count).toBeLessThan(8);
      for (; released < now; released += 1) {
        await page.evaluate((index) => window.releaseEchoes([index]), released);
        await page.waitForTimeout(20);
      }
    }
  }

  // --- The vanilla view: the host lists `replaceValue`'s text late. ----------

  /** Mounts a documents view with `echoes: true` in the vanilla page whose
   * host holds every text it is to list (`onChange`'s and the resets'):
   * `window.held` of them, `window.release(index)` lists one. */
  async function mountHolding(page) {
    await open(page, 'documents.html');
    await page.evaluate(async () => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      host.id = 'probe';
      host.style.height = '420px';
      document.body.prepend(host);
      window.files = [
        { id: 'main', fileName: 'main.yaml', value: 'listen: 7835\n' },
        { id: 'other', fileName: 'other.yaml', value: 'other: 1\n' },
      ];
      window.held = [];
      window.hold = (id, value) =>
        window.held.push(() => {
          window.files = window.files.map((file) => (file.id === id ? { ...file, value } : file));
          window.view.update({ documents: window.files });
        });
      window.release = (index) => window.held[index]();
      window.view = new module.ConfigDocuments(host, {
        documents: window.files,
        echoes: true,
        onChange: (id, value) => window.hold(id, value),
      });
    });
  }

  // --- A reset to the text the host listed last. -------

  // A discard back to the file as loaded (its echoes of what was typed still
  // held, so the host lists that text last), or back to the echo that came
  // back last, then a key, with the three texts listed late in each of the 6
  // orders: the key stays. Before, the reset to the text listed last was not
  // counted as on its way, so in 2 orders its listing after the key's echo
  // was taken for the host's own and threw the key away.

  /** Every order of `items`. */
  const permutations = (items) =>
    items.length === 0
      ? [[]]
      : items.flatMap((item, at) =>
          permutations([...items.slice(0, at), ...items.slice(at + 1)]).map((rest) => [item, ...rest]),
        );

  /** The pages' drivers: `typeKey(key)` types at the end of the document,
   * `reset(text)` resets it (its listing held), `held()` counts the texts
   * held, `release(index)` lists one, `current()` is the editor's text and
   * the host's. */
  const HOSTS = {
    ...Object.fromEntries(
      DOCUMENTS_FRAMEWORK_PAGES.map((name) => [
        name,
        {
          loaded: '[limits]\nmax_pending = 512\n',
          async open(page) {
            await open(page, name, '?manual');
            await page.click('[role="tab"][data-document-id="limits"]');
            return page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
          },
          reset: (text) => window.documentsHandle.current.setValue('limits', text),
          held: () => window.heldEchoes(),
          release: (index) => window.releaseEchoes([index]),
          current: () => ({
            editor: window.documentsHandle.current.editor('limits').value,
            host: window.currentFiles().find((file) => file.id === 'limits').value,
          }),
        },
      ]),
    ),
    ConfigDocuments: {
      loaded: 'listen: 7835\n',
      async open(page) {
        await mountHolding(page);
        return page.locator('#probe textarea.config-editor__input');
      },
      reset: (text) => {
        window.view.replaceValue('main', text);
        window.hold('main', text);
      },
      held: () => window.held.length,
      release: (index) => window.release(index),
      current: () => ({
        editor: window.view.editor('main').value,
        host: window.files.find((file) => file.id === 'main').value,
      }),
    },
    ConfigEditor: {
      loaded: 'listen: 7835\n',
      async open(page) {
        await open(page, 'documents.html');
        await page.evaluate(async () => {
          window.documentsView.destroy();
          const module = await import('../config-editor/index.js');
          const host = document.createElement('div');
          host.id = 'probe';
          host.style.height = '420px';
          document.body.prepend(host);
          window.stored = 'listen: 7835\n';
          window.held = [];
          window.hold = (value) =>
            window.held.push(() => {
              window.stored = value;
              window.view.update({ value });
            });
          window.release = (index) => window.held[index]();
          window.view = new module.ConfigEditor(host, {
            language: 'yaml',
            value: window.stored,
            echoes: true,
            onChange: (value) => window.hold(value),
          });
        });
        return page.locator('#probe textarea.config-editor__input');
      },
      reset: (text) => {
        window.view.replaceValue(text);
        window.hold(text);
      },
      held: () => window.held.length,
      release: (index) => window.release(index),
      current: () => ({ editor: window.view.value, host: window.stored }),
    },
  };


  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test.describe(`${name}: setValue, then typing, every text handed back late`, () => {
      test.skip(!built, 'js packages not built: run `npm run build` in js/');

      for (const scenario of CASES) {
        test(describe(scenario), async ({ page }) => {
          await open(page, name, '?manual');
          const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
          const typeAtEnd = async (text) => {
            await input.focus();
            await page.keyboard.press('Control+End');
            await page.keyboard.type(text, { delay: 10 });
            await page.waitForTimeout(100);
          };
          await page.click('[role="tab"][data-document-id="limits"]');
          await typeAtEnd('x = 1');
          const first = await page.evaluate(() => window.heldEchoes());
          if (scenario.before === 'back') {
            await page.evaluate((count) => window.releaseEchoes([...Array(count).keys()]), first);
            await page.waitForTimeout(400);
          }
          let reset = '';
          for (let index = 0; index < scenario.resets; index += 1) {
            reset = `[limits]\nmax_pending = ${index + 1}\n`;
            expect(await page.evaluate((reset) => window.documentsHandle.current.setValue('limits', reset), reset)).toBe(
              true,
            );
            await page.waitForTimeout(50);
          }
          if (scenario.switches) {
            await page.click('[role="tab"][data-document-id="main"]');
            await page.waitForTimeout(100);
            await page.click('[role="tab"][data-document-id="limits"]');
            await page.waitForTimeout(100);
          }
          const reported = await page.evaluate(() => window.heldEchoes());
          await typeAtEnd('y = 2');
          const count = await page.evaluate(() => window.heldEchoes());
          const range = (from, to) => [...Array(to - from).keys()].map((index) => index + from);
          const typedBefore = scenario.before === 'held' ? range(0, first) : [];
          const resets = range(first, reported);
          const typed = range(reported, count);
          const order = [...typedBefore, ...ORDERS[scenario.order]({ resets, typed })];
          await release(page, order, count);
          const expected = `${reset}y = 2`;
          expect(
            await page.evaluate(() => ({
              editor: window.documentsHandle.current.editor('limits').value,
              parent: window.currentFiles().find((file) => file.id === 'limits').value,
            })),
          ).toEqual({ editor: expected, parent: expected });
          expect(page.errors).toEqual([]);
        });
      }
    });
  }

  test.describe('ConfigDocuments: replaceValue, then typing, the host listing every text late', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    for (const scenario of CASES) {
      test(describe(scenario), async ({ page }) => {
        await mountHolding(page);
        const typeAtEnd = async (text) => {
          await page.evaluate(() => window.view.focus());
          await page.keyboard.press('Control+End');
          await page.keyboard.type(text, { delay: 10 });
          await page.waitForTimeout(100);
        };
        await typeAtEnd('a: 1');
        const first = await page.evaluate(() => window.held.length);
        if (scenario.before === 'back') {
          await page.evaluate((count) => [...Array(count).keys()].forEach(window.release), first);
          await page.waitForTimeout(400);
        }
        let reset = '';
        for (let index = 0; index < scenario.resets; index += 1) {
          reset = `listen: ${index + 1}\n`;
          await page.evaluate((reset) => {
            window.view.replaceValue('main', reset);
            window.hold('main', reset);
          }, reset);
          await page.waitForTimeout(50);
        }
        if (scenario.switches) {
          await page.evaluate(() => window.view.select('other'));
          await page.waitForTimeout(100);
          await page.evaluate(() => window.view.select('main'));
          await page.waitForTimeout(100);
        }
        const reported = await page.evaluate(() => window.held.length);
        await typeAtEnd('b: 2');
        const count = await page.evaluate(() => window.held.length);
        const range = (from, to) => [...Array(to - from).keys()].map((index) => index + from);
        const typedBefore = scenario.before === 'held' ? range(0, first) : [];
        const order = [
          ...typedBefore,
          ...ORDERS[scenario.order]({ resets: range(first, reported), typed: range(reported, count) }),
        ];
        for (const index of order) {
          await page.evaluate((index) => window.release(index), index);
          await page.waitForTimeout(5);
        }
        for (let released = count; ; ) {
          await page.waitForTimeout(400);
          const now = await page.evaluate(() => window.held.length);
          if (now === released) break;
          expect(now - count).toBeLessThan(8);
          for (; released < now; released += 1) await page.evaluate((index) => window.release(index), released);
        }
        const expected = `${reset}b: 2`;
        expect(
          await page.evaluate(() => ({
            editor: window.view.editor('main').value,
            host: window.files.find((file) => file.id === 'main').value,
          })),
        ).toEqual({ editor: expected, host: expected });
        expect(page.errors).toEqual([]);
      });
    }
  });

  for (const [name, host] of Object.entries(HOSTS)) {
    test.describe(`${name}: a reset to the text listed last, then a key, listed in any order`, () => {
      test.skip(!built, 'js packages not built: run `npm run build` in js/');

      for (const to of ['loaded', 'echo back']) {
        for (const order of permutations([0, 1, 2])) {
          test(`back to the ${to === 'loaded' ? 'file as loaded' : 'echo that came back'}, listed ${order}`, async ({
            page,
          }) => {
            const input = await host.open(page);
            const held = () => page.evaluate(host.held);
            const typeKey = async (key) => {
              const before = await held();
              await input.focus();
              await page.keyboard.press('Control+End');
              await page.keyboard.type(key);
              await expect.poll(held).toBe(before + 1);
            };
            const reset = async (text) => {
              const before = await held();
              await page.evaluate(host.reset, text);
              await expect.poll(held).toBe(before + 1);
            };
            const release = (index) => page.evaluate(host.release, index);
            await typeKey('a');
            let target = host.loaded;
            if (to === 'echo back') {
              await release(0);
              await page.waitForTimeout(400);
              await typeKey('b');
              target = `${host.loaded}a`;
            }
            const first = (await held()) - 1;
            await reset(target);
            await typeKey('c');
            for (const index of order) {
              await release(first + index);
              await page.waitForTimeout(10);
            }
            // What the editor sends again (at once, or after the pause).
            for (let released = await held(); ; ) {
              await page.waitForTimeout(400);
              const now = await held();
              if (now === released) break;
              expect(now - released).toBeLessThan(8);
              for (; released < now; released += 1) await release(released);
            }
            const expected = `${target}c`;
            expect(await page.evaluate(host.current)).toEqual({ editor: expected, host: expected });
            expect(page.errors).toEqual([]);
          });
        }
      }
    });
  }
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // A reset while echoes are still on their way.
  //
  // - a host handing every text back late with `readOnlyRanges`
  //   worked out on its own copy, a reset (`replaceValue`, the handle's
  //   `setValue`) after typing a line above a locked one: the lock stays on
  //   its field. Before, it was placed again where it was last given, a line
  //   below, and the field could be edited (and was saved).
  // - two resets to the same text (a double Discard), then typing,
  //   then the host's own revert to that text: it applies. Before, the
  //   second reset waited for a listing of its own, so for 5 seconds the
  //   revert was taken for its echo and the editor overwrote the host.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  function listen(page) {
    page.errors = [];
    page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
    page.on('pageerror', (error) => page.errors.push(String(error)));
  }

  async function open(page, name, query = '') {
    listen(page);
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').first().waitFor();
  }

  const LOADED = 'listen: 7835\npublic_address: tunnel.example.com\n';

  const RESET = 'listen: 7835\npublic_address: reset.example.com\n';

  /** Installs `window.lockFor(text)`: the host's lock on the
   * `public_address` line of its text, in `form` (`line` or `index`). */
  const lockFor = (page, form) =>
    page.evaluate((form) => {
      window.lockFor = (text) => {
        const lines = text.split('\n');
        const at = lines.findIndex((line) => line.startsWith('public_address'));
        if (at < 0) return [];
        if (form === 'line') return [{ startLine: at + 1, startColumn: 1, endLine: at + 1, endColumn: lines[at].length + 1 }];
        const start = text.indexOf('public_address');
        return [{ start, end: start + lines[at].length }];
      };
    }, form);

  /** Mounts, in the vanilla page, a `ConfigEditor` (`kind` 'editor') or a
   * `ConfigDocuments` with `echoes: true` whose host holds every text it is
   * to give back with its lock: `window.held` of them, `window.release(i)`
   * gives one. `window.reset(text)` resets; `window.current()` is the
   * editor's text and the host's. */
  async function mountHolding(page, kind, form) {
    await open(page, 'documents.html');
    await lockFor(page, form);
    await page.evaluate(
      async ({ kind, loaded }) => {
        window.documentsView.destroy();
        const module = await import('../config-editor/index.js');
        const element = document.createElement('div');
        element.id = 'probe';
        element.style.height = '420px';
        document.body.prepend(element);
        window.stored = loaded;
        window.held = [];
        window.release = (index) => window.held[index]();
        const list = () => [{ id: 'main', fileName: 'main.yaml', value: window.stored, readOnlyRanges: window.lockFor(window.stored) }];
        const give = (value) => {
          window.stored = value;
          if (kind === 'editor') window.view.update({ value, readOnlyRanges: window.lockFor(value) });
          else window.view.update({ documents: list() });
        };
        const hold = (value) => window.held.push(() => give(value));
        if (kind === 'editor') {
          window.view = new module.ConfigEditor(element, {
            language: 'yaml',
            value: loaded,
            readOnlyRanges: window.lockFor(loaded),
            echoes: true,
            onChange: hold,
          });
          window.reset = (text) => window.view.replaceValue(text);
          window.current = () => ({ editor: window.view.value, host: window.stored });
        } else {
          window.view = new module.ConfigDocuments(element, { documents: list(), echoes: true, onChange: (id, value) => hold(value) });
          window.reset = (text) => window.view.replaceValue('main', text);
          window.current = () => ({ editor: window.view.editor('main').value, host: window.stored });
        }
      },
      { kind, loaded: LOADED },
    );
    return page.locator('#probe textarea.config-editor__input');
  }

  /** Puts the caret at the end of the document's `public_address` line and
   * tries to change it: two Backspaces and an `X`. */
  async function editLockedLine(page, input, text) {
    const line = text.split('\n').findIndex((each) => each.startsWith('public_address'));
    await input.focus();
    await page.keyboard.press('Control+Home');
    for (let at = 0; at < line; at += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('X');
    await page.waitForTimeout(200);
  }

  const lockedLine = (text) => text.split('\n').find((line) => line.startsWith('public_address'));

  // --- a double Discard, typing, then the host's own revert. --------

  const LIMITS = '[limits]\nmax_pending = 1\n';

  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const kind of ['editor', 'documents']) {
    const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';
    test.describe(`${name}: a reset with echoes and their locks still on their way`, () => {
      for (const form of ['line', 'index']) {
        for (const typedAfter of [false, true]) {
          test(`the lock stays on its field (${form} ranges${typedAfter ? ', typing after the reset' : ''})`, async ({
            page,
          }) => {
            const input = await mountHolding(page, kind, form);
            await input.focus();
            await page.keyboard.press('Control+Home');
            await page.keyboard.press('End');
            await page.keyboard.press('Enter');
            await page.keyboard.type('a: 1', { delay: 15 });
            await expect.poll(() => page.evaluate(() => window.held.length)).toBeGreaterThan(2);
            // The host takes only the first three texts back (its lock on
            // line 3 of each), then the user resets.
            await page.evaluate(() => [0, 1, 2].forEach(window.release));
            await page.waitForTimeout(100);
            await page.evaluate((text) => window.reset(text), RESET);
            await page.waitForTimeout(100);
            if (typedAfter) {
              await input.focus();
              await page.keyboard.press('Control+End');
              await page.keyboard.type('b: 2', { delay: 15 });
              await page.keyboard.press('Escape');
            }
            const before = await page.evaluate(() => window.current().editor);
            await editLockedLine(page, input, before);
            const after = await page.evaluate(() => window.current().editor);
            expect(lockedLine(after)).toBe(lockedLine(RESET));
            // Every text the host still holds given back: the field stays.
            const count = await page.evaluate(() => window.held.length);
            for (let index = 3; index < count; index += 1) await page.evaluate((index) => window.release(index), index);
            await page.waitForTimeout(400);
            expect(lockedLine(await page.evaluate(() => window.current().editor))).toBe(lockedLine(RESET));
          });
        }
      }
    });
  }

  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test.describe(`${name}: setValue with echoes and their locks still on their way`, () => {
      for (const form of ['line', 'index']) {
        test(`the lock stays on its field (${form} ranges)`, async ({ page }) => {
          await open(page, name, '?manual');
          await lockFor(page, form);
          // The host gives its text back with its lock now and then, worked
          // out on the text it holds.
          await page.evaluate(() => {
            window.relisting = setInterval(() => {
              const file = window.currentFiles().find((each) => each.id === 'main');
              window.updateFile('main', { value: file.value, readOnlyRanges: window.lockFor(file.value) });
            }, 17);
          });
          await page.waitForTimeout(200);
          const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
          await input.focus();
          await page.keyboard.press('Control+Home');
          await page.keyboard.press('End');
          await page.keyboard.press('Enter');
          await page.keyboard.type('a: 1', { delay: 15 });
          await expect.poll(() => page.evaluate(() => window.heldEchoes())).toBeGreaterThan(2);
          await page.evaluate(() => window.releaseEchoes([0, 1, 2]));
          await page.waitForTimeout(100);
          await page.evaluate((text) => window.documentsHandle.current.setValue('main', text), RESET);
          await input.focus();
          await page.keyboard.press('Control+End');
          await page.keyboard.type('b: 2', { delay: 15 });
          await page.keyboard.press('Escape');
          await page.waitForTimeout(100);
          const value = () => page.evaluate(() => window.documentsHandle.current.editor('main').value);
          await editLockedLine(page, input, await value());
          await page.evaluate(() => clearInterval(window.relisting));
          expect(lockedLine(await value())).toBe(lockedLine(RESET));
          expect(page.errors.filter((error) => !error.includes('past the end'))).toEqual([]);
        });
      }
    });
  }

  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test.describe(`${name}: setValue twice to one text, typing, then the host's revert`, () => {
      for (const when of ['in one task', '30 ms apart']) {
        test(`the revert applies (${when})`, async ({ page }) => {
          await open(page, name);
          await page.click('[role="tab"][data-document-id="limits"]');
          const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
          const typeAtEnd = async (text) => {
            await input.focus();
            await page.keyboard.press('Control+End');
            await page.keyboard.type(text, { delay: 20 });
            await page.waitForTimeout(400);
          };
          await typeAtEnd('x = 1');
          await page.evaluate(
            async ({ text, apart }) => {
              const handle = window.documentsHandle.current;
              handle.setValue('limits', text);
              if (apart) await new Promise((resolve) => setTimeout(resolve, 30));
              handle.setValue('limits', text);
            },
            { text: LIMITS, apart: when !== 'in one task' },
          );
          await page.waitForTimeout(400);
          await typeAtEnd('y = 2');
          await page.waitForTimeout(400);
          await page.evaluate((text) => window.updateFile('limits', { value: text }), LIMITS);
          await page.waitForTimeout(1000);
          expect(
            await page.evaluate(() => ({
              editor: window.documentsHandle.current.editor('limits').value,
              host: window.currentFiles().find((file) => file.id === 'limits').value,
            })),
          ).toEqual({ editor: LIMITS, host: LIMITS });
          expect(page.errors).toEqual([]);
        });
      }
    });
  }

  for (const kind of ['editor', 'documents']) {
    const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';
    test.describe(`${name}: replaceValue twice to one text, typing, then the host's revert`, () => {
      for (const listings of [1, 2]) {
        test(`the revert applies (the host lists the reset ${listings === 1 ? 'once' : 'twice'})`, async ({ page }) => {
          await open(page, 'documents.html');
          await page.evaluate(
            async ({ kind }) => {
              window.documentsView.destroy();
              const module = await import('../config-editor/index.js');
              const element = document.createElement('div');
              element.id = 'probe';
              element.style.height = '420px';
              document.body.prepend(element);
              window.stored = 'listen: 7835\n';
              window.give = (value) => {
                window.stored = value;
                if (kind === 'editor') window.view.update({ value });
                else window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value }] });
              };
              if (kind === 'editor') {
                window.view = new module.ConfigEditor(element, {
                  language: 'yaml',
                  value: window.stored,
                  echoes: true,
                  onChange: window.give,
                });
                window.reset = (text) => window.view.replaceValue(text);
                window.current = () => ({ editor: window.view.value, host: window.stored });
              } else {
                window.view = new module.ConfigDocuments(element, {
                  documents: [{ id: 'main', fileName: 'main.yaml', value: window.stored }],
                  echoes: true,
                  onChange: (id, value) => window.give(value),
                });
                window.reset = (text) => window.view.replaceValue('main', text);
                window.current = () => ({ editor: window.view.editor('main').value, host: window.stored });
              }
            },
            { kind },
          );
          const input = page.locator('#probe textarea.config-editor__input');
          const typeAtEnd = async (text) => {
            await input.focus();
            await page.keyboard.press('Control+End');
            await page.keyboard.type(text, { delay: 20 });
            await page.waitForTimeout(400);
          };
          const reset = 'listen: 1\n';
          await typeAtEnd('a: 1');
          await page.evaluate(
            ({ reset, listings }) => {
              window.reset(reset);
              window.reset(reset);
              for (let listed = 0; listed < listings; listed += 1) window.give(reset);
            },
            { reset, listings },
          );
          await page.waitForTimeout(100);
          await typeAtEnd('b: 2');
          await page.waitForTimeout(400);
          await page.evaluate((reset) => window.give(reset), reset);
          await page.waitForTimeout(1000);
          expect(await page.evaluate(() => window.current())).toEqual({ editor: reset, host: reset });
          expect(page.errors).toEqual([]);
        });
      }
    });
  }
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // A host's texts and the ranges it works out on them.
  // - right after a reset, a host still listing the text it held
  //   before (its own copy lags) with the lock it worked out on that text:
  //   typing on the line above the lock is taken. Before, those ranges were
  //   moved across the reset as if it were an edit, the lock grew over the
  //   line above, and the typing was refused.
  // - a framework's render listing a newer text with the lock worked
  //   out on the text it rendered before (line ranges): the lock stays on its
  //   field. Before, they were read in the newer text, on the line above.
  // - a framework page's `setValue(id, text)` twice in one task
  //   reports the reset once (the second changes nothing).
  // - a document listed with an Edit-as text, reset in the same task
  //   as `new ConfigDocuments`: its value is the reset's text at once.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  const LOADED = 'listen: 7835\npublic_address: tunnel.example.com\n';

  const RESET = 'listen: 2\npublic_address: two.example.com\nx: 1\n';

  async function open(page, name, query = '') {
    page.errors = [];
    page.on('pageerror', (error) => page.errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  }

  /** Mounts on the vanilla page a `ConfigEditor` (`kind` 'editor') or a
   * `ConfigDocuments` with `echoes`, whose host keeps its own copy
   * (`window.stored`), takes each text back only when told
   * (`window.release()`), and gives its copy back with its lock on the
   * `public_address` line every 17 ms: `window.lockText(stored)` names the
   * text the lock is worked out on (its copy, unless a test says another). */
  async function mountLagging(page, kind) {
    await open(page, 'documents.html');
    await page.evaluate(
      async ({ kind, loaded }) => {
        window.documentsView.destroy();
        const module = await import('../config-editor/index.js');
        const element = document.createElement('div');
        element.id = 'probe';
        element.style.height = '420px';
        document.body.prepend(element);
        window.lockFor = (text) => {
          const lines = text.split('\n');
          const at = lines.findIndex((line) => line.startsWith('public_address'));
          if (at < 0) return [];
          return [{ startLine: at + 1, startColumn: 1, endLine: at + 1, endColumn: lines[at].length + 1 }];
        };
        window.stored = loaded;
        window.lockText = (stored) => stored;
        window.held = [];
        window.release = () => {
          for (const take of window.held.splice(0)) take();
        };
        const hold = (value) => window.held.push(() => (window.stored = value));
        const give = () => {
          const value = window.stored;
          const readOnlyRanges = window.lockFor(window.lockText(value));
          if (kind === 'editor') window.view.update({ value, readOnlyRanges });
          else window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value, readOnlyRanges }] });
        };
        if (kind === 'editor') {
          window.view = new module.ConfigEditor(element, {
            language: 'yaml',
            value: loaded,
            readOnlyRanges: window.lockFor(loaded),
            echoes: true,
            onChange: hold,
          });
          window.reset = (text) => {
            window.view.replaceValue(text);
            hold(text);
          };
          window.current = () => window.view.value;
        } else {
          const list = [{ id: 'main', fileName: 'main.yaml', value: loaded, readOnlyRanges: window.lockFor(loaded) }];
          window.view = new module.ConfigDocuments(element, { documents: list, echoes: true, onChange: (id, value) => hold(value) });
          window.reset = (text) => {
            window.view.replaceValue('main', text);
            hold(text);
          };
          window.current = () => window.view.editor('main').value;
        }
        window.relisting = setInterval(give, 17);
      },
      { kind, loaded: LOADED },
    );
    return page.locator('#probe textarea.config-editor__input');
  }

  /** Types `text` at the end of line `line` (one-based). */
  async function typeAtEndOf(page, input, line, text) {
    await input.focus();
    await page.keyboard.press('Control+Home');
    for (let at = 1; at < line; at += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.type(text, { delay: 15 });
  }

  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const kind of ['editor', 'documents']) {
    const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';

    test(`${name}: right after a reset, a host still giving its older copy with its lock: the line above the lock takes typing`, async ({
      page,
    }) => {
      const input = await mountLagging(page, kind);
      await page.waitForTimeout(100);
      await typeAtEndOf(page, input, 2, '');
      await page.keyboard.press('End');
      await page.keyboard.press('Enter');
      await page.keyboard.type('a: 1', { delay: 15 });
      await page.evaluate((text) => window.reset(text), RESET);
      await page.waitForTimeout(100);
      // The host still gives its copy from before the reset.
      await typeAtEndOf(page, input, 1, '0');
      await page.waitForTimeout(100);
      // The lock is on its field.
      await typeAtEndOf(page, input, 2, 'Z');
      await page.waitForTimeout(100);
      expect(await page.evaluate(() => window.current())).toBe('listen: 20\npublic_address: two.example.com\nx: 1\n');
      await page.evaluate(() => window.release());
      await page.waitForTimeout(400);
      await page.evaluate(() => window.release());
      await page.waitForTimeout(400);
      await page.evaluate(() => clearInterval(window.relisting));
      expect(await page.evaluate(() => window.current())).toBe('listen: 20\npublic_address: two.example.com\nx: 1\n');
      expect(page.errors).toEqual([]);
    });

    test(`${name}: a newer text given with the lock worked out on the text given before: the lock stays on its field`, async ({ page }) => {
      const input = await mountLagging(page, kind);
      await page.waitForTimeout(100);
      // From now on the host works its lock out on the copy it gave before.
      await page.evaluate(() => {
        let before = window.stored;
        let given = window.stored;
        window.lockText = (stored) => {
          if (stored !== given) [before, given] = [given, stored];
          return before;
        };
      });
      // Two new lines, each given back on its own: the second with the lock
      // worked out on the text with the first.
      for (const _ of [0, 1]) {
        await typeAtEndOf(page, input, 1, '');
        await page.keyboard.press('Enter');
        await page.waitForTimeout(50);
        await page.evaluate(() => window.release());
        await page.waitForTimeout(100);
      }
      await typeAtEndOf(page, input, 4, 'Z');
      await page.waitForTimeout(100);
      await page.evaluate(() => window.release());
      await page.waitForTimeout(300);
      await page.evaluate(() => clearInterval(window.relisting));
      expect(await page.evaluate(() => window.current())).toBe('listen: 7835\n\n\npublic_address: tunnel.example.com\n');
      expect(page.errors).toEqual([]);
    });
  }

  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test(`${name}: setValue twice to one text reports it once`, async ({ page }) => {
      await open(page, name);
      await page.evaluate(() => {
        const handle = window.documentsHandle.current;
        handle.setValue('limits', '[limits]\nmax_pending = 1\n');
        handle.setValue('limits', '[limits]\nmax_pending = 1\n');
      });
      await page.waitForTimeout(200);
      const log = await page.locator('#log').textContent();
      expect(log.split('\n').filter((line) => line === 'change limits')).toHaveLength(1);
    });
  }

  for (const active of ['x', 'other']) {
    test(`ConfigDocuments: a document listed with an Edit-as text, reset at once (${active === 'x' ? 'shown' : 'not shown'}), reads as the reset`, async ({
      page,
    }) => {
      await open(page, 'documents.html');
      const read = await page.evaluate(async (active) => {
        window.documentsView.destroy();
        const module = await import('../config-editor/index.js');
        const element = document.createElement('div');
        element.style.height = '420px';
        document.body.prepend(element);
        const documents = [
          { id: 'other', fileName: 'o.yaml', value: 'o: 1\n' },
          { id: 'x', fileName: 'x.toml', value: 'a = 1\n', baseline: 'a = 1\n', editAs: 'yaml', editAsText: 'a: 2\n' },
        ];
        const view = new module.ConfigDocuments(element, { documents, activeId: active, onChange: () => {} });
        view.replaceValue('x', 'a = 5\n');
        const now = { value: view.editor('x').value, diff: view.editor('x').unifiedDiff() };
        await new Promise((resolve) => setTimeout(resolve, 300));
        return { now, later: { value: view.editor('x').value, diff: view.editor('x').unifiedDiff() } };
      }, active);
      expect(read.now.value).toBe('a = 5\n');
      expect(read.later.value).toBe('a = 5\n');
      expect(read.now.diff).toBe(read.later.diff);
      expect(read.now.diff).toContain('+a = 5');
    });
  }
});
