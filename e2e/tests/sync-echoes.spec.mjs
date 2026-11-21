import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES, FRAMEWORK_PAGES } from './helpers.mjs';

// Hosts that echo the value back late, in any order, or not at all: typing is never lost.

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
 * `delay` ms late (`echoes: true`), the caret after `name: x`. */
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
          echoes: true,
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
        echoes: true,
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

// A host echoing `value` late (a store that
// re-renders after a delay) never rolls typing back, in a ConfigEditor and a
// ConfigDocuments with one or several documents, while a text of the host's
// own still wins; `destroy()` works from a callback a method of the same
// instance fired, and a document selected in answer to a selection stands;
// `onDirtyChange` and `onChangeSummary` given later hear the state at once;
// the READMEs' React snippets import the hooks they use; and the read-only
// notice wraps a long word.
test.describe('host API', () => {
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
  // A host still handing back every text it was sent (a text of
  // its own given through `update` makes the texts sent before it its own
  // too, so going back to one applies) resets through `replaceValue`.
  test("a host text of its own set with replaceValue still wins over echoes on their way, and the host hears the document again", async ({ page }) => {
    await open(page);
    for (const host of HOSTS) {
      await mountEchoing(page, host, 150);
      await page.keyboard.type('abc', { delay: 20 });
      // The host loads a file while those echoes are on their way.
      await page.evaluate(() => {
        window.store.a = 'reset: 1\n';
        if (window.echoing.editor) window.echoing.replaceValue('a', 'reset: 1\n');
        else window.echoing.replaceValue('reset: 1\n');
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
});

// A group of its own (its own helpers).
test.describe(() => {
  // By default every `value` a host gives applies: a
  // host that only saves `onChange` texts switches files, discards and resets
  // with `update`, however soon after typing. A host that hands every text
  // back says so with `echoes: true`: its echoes, late and in any order, never
  // roll typing back (ConfigEditor, ConfigDocuments with one or several
  // documents, Edit-as, the framework documents pages), and `replaceValue`
  // goes back to a text still on its way.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  async function open(page) {
    page.errors = [];
    page.on('console', (message) => {
      if (message.type() === 'error' || message.type() === 'warning') page.errors.push(message.text());
    });
    page.on('pageerror', (error) => page.errors.push(String(error)));
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
        document.body.prepend(element);
        return element;
      };
    });
  }

  /** Mounts a ConfigEditor over `files` (`window.files`, the one shown
   * `window.current`) whose host only stores each `onChange` text. */
  async function mountSwitcher(page) {
    await page.evaluate(async () => {
      window.files = { A: 'a: 1\n', B: 'b: 2\n' };
      window.current = 'A';
      window.switcher = new window.module.ConfigEditor(window.mountPoint(), {
        value: window.files.A,
        baseline: window.files.A,
        onChange: (text) => (window.files[window.current] = text),
      });
      // A file switch, the update's other keys given with it (whether the
      // document differs from its baseline is worked out a moment later).
      window.switchTo = async (name) => {
        window.current = name;
        window.switcher.update({ value: window.files[name], baseline: window.files[name], readOnlyPaths: [], hostDiagnostics: [] });
        const text = window.switcher.value;
        await window.pause(100);
        return { text, dirty: window.switcher.isDirty };
      };
      await window.pause(300);
    });
  }

  /** Puts the caret at `offset` in the editor `name` names (`window[name]`, or
   * a document's editor). */
  const caretAt = (page, offset, of = 'switcher') =>
    page.evaluate(([offset, of]) => {
      const editor = of.startsWith('doc:') ? window.view.editor(of.slice(4)) : window[of];
      editor.revealRange({ start: offset, end: offset });
    }, [offset, of]);

  // --- Hosts that echo (`echoes: true`). ---------------------------------------

  // Typing `a`, `b`, a backspace and `c` (the text `…a` sent twice), the edits
  // held (0: …a, 1: …ab, 2: …a, 3: …ac) and handed back in these groups, each
  // group at once (one render for all its edits). [0], [3, 2], [1] is the kind
  // of order CI met (job 129267793501): the render taking `…ac` and then the
  // late second `…a` gives only `…a`, and nothing still to come brings the
  // host to `…ac` but the check after the pause.
  const RELEASES = [
    [[0], [3, 2], [1]],
    [[3, 2], [1], [0]],
    [[3], [0], [1], [2]],
    [[1, 3], [0, 2]],
    [[0, 1, 2, 3]],
    [[2], [1], [0], [3]],
  ];

  /** Releases `groups` of the 4 edits held (`releaseEchoes`), then what the
   * editor sends again (at once, or after the pause) one by one, until nothing
   * more comes. */
  async function releaseHeld(page, groups) {
    expect(await page.evaluate(() => window.heldEchoes())).toBe(4);
    for (const group of groups) {
      await page.evaluate((group) => window.releaseEchoes(group), group);
      await page.waitForTimeout(50);
    }
    for (let released = 4; ; ) {
      await page.waitForTimeout(400);
      const held = await page.evaluate(() => window.heldEchoes());
      if (held === released) return;
      expect(held - 4, String(groups)).toBeLessThan(6);
      for (; released < held; released += 1) {
        await page.evaluate((index) => window.releaseEchoes([index]), released);
        await page.waitForTimeout(50);
      }
    }
  }

  const HOSTS = ['editor', 'one document', 'two documents'];

  /** Mounts `host` in the page with `echoes: true`, its host handing every
   * `onChange` text back after `delay(text)` ms (`null`: never), the caret
   * after `name: x`. `window.textOf()` reads the document. */
  async function mountEchoing(page, host, kind, options = {}) {
    await page.evaluate(
      async ([host, kind, options]) => {
        const delays = {
          none: () => null,
          microtask: () => 0,
          random: () => Math.random() * 200,
          // A fetch whose answer takes longer for every other text.
          'by length': (text) => 50 + (text.length % 2) * 120,
        };
        const delay = delays[kind] ?? (() => Number(kind));
        // `store` keeps a text; `window.commit()` hands the host's texts back.
        // 'held': kept when the test says so, `releaseEchoes([3, 2])` keeping
        // those texts (numbered as sent) and handing back once for all.
        window.held = [];
        window.heldEchoes = () => window.held.length;
        window.releaseEchoes = (group) => {
          group.forEach((index) => window.held[index]());
          window.commit();
        };
        const later = (text, store) => {
          if (kind === 'held') return void window.held.push(store);
          const ms = delay(text);
          if (ms === null) return;
          const apply = () => {
            store();
            window.commit();
          };
          if (ms === 0) Promise.resolve().then(apply);
          else setTimeout(apply, ms);
        };
        const element = window.mountPoint();
        const echoes = kind !== 'none';
        if (host === 'editor') {
          window.store = { a: 'name: x\n' };
          const editor = new window.module.ConfigEditor(element, {
            value: window.store.a,
            echoes,
            ...options,
            onChange: (text) => {
              if (!echoes) window.store.a = text;
              later(text, () => (window.store.a = text));
            },
          });
          window.commit = () => editor.update({ value: window.store.a });
          window.echoing = editor;
          window.textOf = () => editor.value;
          window.replace = (text) => {
            window.store.a = text;
            editor.replaceValue(text);
          };
          await window.pause(300);
          editor.revealRange({ start: 7, end: 7 });
          return;
        }
        window.store = host === 'one document' ? { a: 'name: x\n' } : { a: 'name: x\n', b: 'other: 1\n' };
        const list = () => Object.entries(window.store).map(([id, value]) => ({ id, fileName: `${id}.yaml`, value, ...options }));
        const view = new window.module.ConfigDocuments(element, {
          documents: list(),
          echoes,
          onChange: (id, text) => {
            if (!echoes) window.store[id] = text;
            later(text, () => (window.store[id] = text));
          },
        });
        window.commit = () => view.update({ documents: list() });
        window.echoing = view;
        window.textOf = () => view.editor('a').value;
        window.replace = (text) => {
          window.store.a = text;
          view.replaceValue('a', text);
        };
        await window.pause(400);
        view.editor('a').revealRange({ start: 7, end: 7 });
      },
      [host, kind, options],
    );
  }

  /** What the page shows and its host stores once the echoes are back. */
  const settled = (page) => page.evaluate(() => ({ text: window.textOf(), store: window.store }));

  /** Types a sequence: letters, backspaces, more letters, an undo. */
  async function typeSequence(page) {
    await page.keyboard.type('abcdefghij', { delay: 25 });
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('XY', { delay: 25 });
    await page.keyboard.press('Control+z');
  }

  // --- The framework documents pages, a parent listing edits back late. --------

  // `?random` delays from a seeded generator (`&seed=`): the same delays on
  // every run.
  const seeded = () => {
    let seed = Number(new URLSearchParams(location.search).get('seed'));
    if (!seed) return;
    Math.random = () => {
      seed ^= seed << 13;
      seed ^= seed >>> 17;
      seed ^= seed << 5;
      return (seed >>> 0) / 2 ** 32;
    };
  };

  async function openDocuments(page, name, query) {
    await page.addInitScript(seeded);
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').first().waitFor();
    await page.evaluate(() => {
      const editor = window.documentsHandle.current.editor('main');
      const end = editor.value.length;
      editor.revealRange({ start: end, end });
    });
  }

  /** The main document's text in the editor, and in the parent's list. */
  const main = (page) =>
    page.evaluate(() => ({
      text: window.documentsHandle.current.editor('main').value,
      parent: window.currentFiles().find((file) => file.id === 'main').value,
    }));


  test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const wait of [300, 2000]) {
      test(`a host that does not echo switches files with update ${wait} ms after typing`, async ({ page }) => {
        await open(page);
        await mountSwitcher(page);
        await caretAt(page, 4);
        await page.keyboard.type('9');
        await page.waitForTimeout(wait);
        expect(await page.evaluate(() => window.switchTo('B'))).toEqual({ text: 'b: 2\n', dirty: false });
        await caretAt(page, 4);
        await page.keyboard.type('7');
        await page.waitForTimeout(300);
        expect(await page.evaluate(() => window.switchTo('A'))).toEqual({ text: 'a: 19\n', dirty: false });
        expect(await page.evaluate(() => window.switchTo('B'))).toEqual({ text: 'b: 27\n', dirty: false });
        await page.waitForTimeout(300);
        expect(await page.evaluate(() => window.files)).toEqual({ A: 'a: 19\n', B: 'b: 27\n' });
      expect(page.errors).toEqual([]);
      });
    }

    test('a host that does not echo discards to what it saved, and resets to a text the user passed through', async ({ page }) => {
      await open(page);
      await page.evaluate(async () => {
        window.saved = 'a: 1\n';
        const editor = new window.module.ConfigEditor(window.mountPoint(), {
          value: window.saved,
          onChange: () => {},
          onSave: (text) => {
            window.saved = text;
            editor.update({ baseline: text });
          },
        });
        window.switcher = editor;
        await window.pause(300);
      });
      await caretAt(page, 4);
      await page.keyboard.type('x');
      await page.keyboard.press('Control+s');
    await page.keyboard.type('y');
      // Discard: back to what was saved, a text onChange gave a moment ago.
      expect(
        await page.evaluate(async () => {
          window.switcher.update({ value: window.saved });
          const text = window.switcher.value;
          await window.pause(100);
          return { text, dirty: window.switcher.isDirty, saved: window.saved };
        }),
      ).toEqual({ text: 'a: 1x\n', dirty: false, saved: 'a: 1x\n' });
    // Reset to the text the user passed through (typed, deleted, typed on).
      await caretAt(page, 5);
      await page.keyboard.type('2');
      await page.keyboard.press('Backspace');
    await page.keyboard.type('3');
      await page.waitForTimeout(400);
      expect(
        await page.evaluate(async () => {
          window.switcher.update({ value: 'a: 1x\n', baseline: 'a: 1x\n' });
        const text = window.switcher.value;
          await window.pause(100);
          return { text, dirty: window.switcher.isDirty };
        }),
      ).toEqual({ text: 'a: 1x\n', dirty: false });
      expect(page.errors).toEqual([]);
    });

    test('a ConfigDocuments whose host does not echo discards a document to what it saved', async ({ page }) => {
      await open(page);
      await page.evaluate(async () => {
        window.docs = [
          { id: 'a', fileName: 'a.yaml', value: 'name: x\n' },
        { id: 'b', fileName: 'b.yaml', value: 'other: 1\n' },
      ];
        const change = (id, fields) => (window.docs = window.docs.map((doc) => (doc.id === id ? { ...doc, ...fields } : doc)));
        window.view = new window.module.ConfigDocuments(window.mountPoint(), {
          documents: window.docs,
          onChange: (id, value) => change(id, { value }),
          // Saved: the list takes the new baseline with the next update.
          onSave: (id, value) => change(id, { baseline: value }),
        });
        await window.pause(400);
      });
      await caretAt(page, 7, 'doc:a');
      await page.keyboard.type('abc', { delay: 30 });
      await page.evaluate(() => window.view.save('a'));
      await page.keyboard.type('d');
      // Discard, a moment after typing: back to what was saved.
      expect(
        await page.evaluate(async () => {
          window.docs = window.docs.map((doc) => ({ ...doc, value: doc.baseline ?? doc.value }));
          window.view.update({ documents: window.docs });
          const editor = window.view.editor('a');
          const texts = { text: editor.value, shown: editor.text };
          await window.pause(100);
          return { ...texts, dirty: window.view.isDirty('a') };
        }),
      ).toEqual({ text: 'name: xabc\n', shown: 'name: xabc\n', dirty: false });
      expect(page.errors).toEqual([]);
    });

    test('echoes cannot change on a mounted instance, and are no unknown option', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(() => {
        const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\n', echoes: true });
        const view = new window.module.ConfigDocuments(window.mountPoint(), { documents: [], echoes: true });
        const thrown = (apply) => {
          try {
            apply();
            return null;
          } catch (error) {
            return String(error.message);
          }
        };
        return [thrown(() => editor.update({ echoes: false })), thrown(() => view.update({ echoes: false }))];
      });
      expect(result[0]).toContain('echoes cannot change on a mounted ConfigEditor');
    expect(result[1]).toContain('echoes cannot change on a mounted ConfigDocuments');
    expect(page.errors).toEqual([]);
    });
  });

  test.describe('hosts that echo', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('echoes 0 to 400 ms late never lose what was typed (every host and typing speed)', async ({ page }) => {
      test.slow();
      await open(page);
      const failures = [];
      for (const host of HOSTS) {
        for (const delay of ['microtask', 16, 60, 150, 400]) {
          for (const keyDelay of [20, 40]) {
            await mountEchoing(page, host, delay);
            await page.keyboard.type('abcdefghij', { delay: keyDelay });
            await page.waitForTimeout(500 + (delay === 400 ? 400 : 0));
            const { text, store } = await settled(page);
            const expected = 'name: xabcdefghij\n';
          if (text !== expected || store.a !== expected || (store.b !== undefined && store.b !== 'other: 1\n')) {
            failures.push({ host, delay, keyDelay, text, store });
            }
          }
        }
      }
      expect(failures).toEqual([]);
      expect(page.errors).toEqual([]);
    });

    test('echoes in any order (random 0-200 ms, or by length) never lose what was typed', async ({ page }) => {
      test.slow();
      await open(page);
      const failures = [];
      for (const host of HOSTS) {
        // What the sequence types with nothing echoed.
        await mountEchoing(page, host, 'none');
        await typeSequence(page);
        await page.waitForTimeout(400);
        const expected = (await settled(page)).text;
        expect(expected).toMatch(/^name: xabcdefgh/);
        for (const kind of ['random', 'random', 'random', 'by length']) {
        await mountEchoing(page, host, kind);
          await typeSequence(page);
          await page.waitForTimeout(900);
          const { text, store } = await settled(page);
          if (text !== expected || store.a !== expected) failures.push({ host, kind, text, store });
        }
      }
      expect(failures).toEqual([]);
      expect(page.errors).toEqual([]);
    });

    test('replaceValue goes back to a text still on its way, the host ending on it', async ({ page }) => {
      await open(page);
      for (const host of HOSTS) {
        for (const kind of [150, 'random']) {
          await mountEchoing(page, host, kind);
          await page.keyboard.type('abc', { delay: 20 });
          expect(
            await page.evaluate(() => {
              window.replace('name: xa\n');
            return window.textOf();
            }),
          ).toBe('name: xa\n');
        await page.waitForTimeout(700);
          expect({ host, kind, ...(await settled(page)) }).toMatchObject({ host, kind, text: 'name: xa\n', store: { a: 'name: xa\n' } });
      }
      }
      expect(page.errors).toEqual([]);
    });

    test('a host handing several echoes back at once, in any grouping and order, ends on the document', async ({ page }) => {
      await open(page);
      for (const host of HOSTS) {
        for (const groups of RELEASES) {
          await mountEchoing(page, host, 'held');
          await page.keyboard.type('ab', { delay: 30 });
          await page.keyboard.press('Backspace');
        await page.keyboard.type('c', { delay: 30 });
          await releaseHeld(page, groups);
          expect({ host, groups, ...(await settled(page)) }).toMatchObject({ host, groups, text: 'name: xac\n', store: { a: 'name: xac\n' } });
      }
      }
      expect(page.errors).toEqual([]);
    });

    test('typing while editing as JSON against echoes in any order keeps every letter', async ({ page }) => {
      await open(page);
      for (const host of ['editor', 'two documents']) {
      await mountEchoing(page, host, 'random', { editAs: 'json' });
        await page.evaluate(() => {
          const editor = window.echoing.editor ? window.echoing.editor('a') : window.echoing;
          const at = editor.text.indexOf('"x"') + 2;
          editor.revealRange({ start: at, end: at });
        });
        await page.keyboard.type('abcdefgh', { delay: 120 });
        await page.waitForTimeout(1500);
        expect({ host, ...(await settled(page)) }).toMatchObject({ host, text: 'name: xabcdefgh\n', store: { a: 'name: xabcdefgh\n' } });
    }
      expect(page.errors).toEqual([]);
    });
  });

  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test.describe(`${name}: a parent listing each edit back late`, () => {
      test.skip(!built, 'js packages not built: run `npm run build` in js/');

      for (const seed of [1, 2, 3]) {
        test(`echoes in any order never lose a keystroke, and setValue resets a document (seed ${seed})`, async ({ page }) => {
          await openDocuments(page, name, `?random&seed=${seed}`);
          const before = (await main(page)).text;
          // Typos fixed (texts sent twice), three bursts: the parent's renders
          // take several late edits at once.
          for (const burst of ['# abcdefghij', 'klmnopq', 'rstuvw']) {
            await page.keyboard.type(burst, { delay: 15 });
            await page.keyboard.press('Backspace');
            await page.keyboard.type('XY', { delay: 15 });
          }
          const typed = before + '# abcdefghiXYklmnopXYrstuvXY';
          expect((await main(page)).text).toBe(typed);
          // Every echo in (0-200 ms), the pause's check (250 ms), its text
          // listed back (0-200 ms): the parent holds what was typed.
          await page.waitForTimeout(800);
          expect(await main(page)).toEqual({ text: typed, parent: typed });
          // Back to the text it was loaded with, a text the user passed through.
          expect(await page.evaluate((text) => window.documentsHandle.current.setValue('main', text), before)).toBe(true);
          await page.waitForTimeout(600);
          expect(await main(page)).toEqual({ text: before, parent: before });
        });
      }

      test('edits listed back in every grouping and order: the parent ends on the document', async ({ page }) => {
        for (const groups of RELEASES) {
          await openDocuments(page, name, '?manual');
          const before = (await main(page)).text;
          await page.keyboard.type('ab', { delay: 30 });
          await page.keyboard.press('Backspace');
          await page.keyboard.type('c', { delay: 30 });
          await releaseHeld(page, groups);
          const typed = before + 'ac';
          expect({ groups, ...(await main(page)) }).toEqual({ groups, text: typed, parent: typed });
        }
      });
    });
  }
});

// A group of its own (its own helpers).
test.describe(() => {
  // A text the host gives of its own replaces the
  // document, and the texts the editor sent before it belong to the text it
  // replaced: going back to one of them (a file picker back on the first
  // file) applies, with `echoes: true` (ConfigEditor, ConfigDocuments) and in
  // the framework components. Ranges given with a skipped echo are in the
  // echoed text: they are mapped to the document, so a lock holds, typing is
  // kept and an update never throws. An update's `value` is the host's word,
  // the constructor's text included (a reset). A custom language's unknown
  // token kind is warned about.
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

  /** A framework page whose parent the test plays: `window.bind(value,
   * onChange, props)` renders the component with them (Vue's `v-model` props
   * for Vue). */
  async function openFramework(page, name) {
    listen(page);
    // `?random`: Svelte's one-way `value` (the test's parent hands it back).
    await page.goto(example(name) + '?random');
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').waitFor();
    await page.evaluate((vue) => {
      window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
      window.bind = (value, onChange, props = {}) =>
        window.setExtraProps(vue ? { modelValue: value, 'onUpdate:modelValue': onChange, ...props } : { value, onChange, ...props });
    }, name.startsWith('vue'));
  }

  const shownText = (page) => page.evaluate(() => document.querySelector('.config-editor__input').value);

  // --- a file picker over one editor. -----------------------------------

  // How the parent stores each edit of the file shown and renders it back:
  // - debounce: stored at once, rendered 300 ms after the last edit (a switch
  //   renders at once);
  // - random: stored and rendered 0-200 ms late (a fetch per edit), into the
  //   file it was made in;
  // - fetch: stored 150 ms late, rendered only while that file is shown.
  const PARENTS = ['debounce', 'random', 'fetch'];

  /** The parent's part, run in the page: `window.files`, `window.current`,
   * `stored(text)` for each edit, `window.switchTo(name)`; `render()` shows
   * `window.files[window.current]`. */
  function parentScript(parent) {
    return `
    window.files = { A: 'a: 1\\n', B: 'b: 2\\n' };
    window.current = 'A';
    let timer = null;
    window.stored = (text) => {
      const file = window.current;
      if (${JSON.stringify(parent)} === 'debounce') {
        window.files[file] = text;
        clearTimeout(timer);
        timer = setTimeout(() => window.render(), 300);
        return;
      }
      const random = ${JSON.stringify(parent)} === 'random';
      setTimeout(() => {
        window.files[file] = text;
        if (random || window.current === file) window.render();
      }, random ? Math.random() * 200 : 150);
    };
    window.switchTo = (name) => {
      window.current = name;
      clearTimeout(timer);
      window.render();
    };
  `;
  }

  /** Types `key` at the end of the first line. */
  async function typeOnFirstLine(page, key) {
    await page.locator('.config-editor__input').focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    await page.keyboard.type(key);
  }

  /** Types in A, switches to B, types there, back to A, back to B: what the
   * editor showed on each file and what the parent holds. */
  async function pickFiles(page, gap) {
    await typeOnFirstLine(page, '9');
    await page.waitForTimeout(gap);
    await page.evaluate(() => window.switchTo('B'));
    await page.waitForTimeout(700);
    const onB = await shownText(page);
    await typeOnFirstLine(page, '7');
    await page.waitForTimeout(gap);
    await page.evaluate(() => window.switchTo('A'));
    await page.waitForTimeout(900);
    const backOnA = await shownText(page);
    await page.evaluate(() => window.switchTo('B'));
    await page.waitForTimeout(900);
    const backOnB = await shownText(page);
    return { onB, backOnA, backOnB, files: await page.evaluate(() => window.files) };
  }

  const PICKED = { onB: 'b: 2\n', backOnA: 'a: 19\n', backOnB: 'b: 27\n', files: { A: 'a: 19\n', B: 'b: 27\n' } };

  // --- ranges given with a skipped echo. --------------------------------

  const START = `name: web\nid: srv-42\nnote: ${'x'.repeat(30)}\n`;

  const TYPED = `# a long comment line here\nname: web\nid: srv-42\nnote: ${'x'.repeat(10)}\n`;

  /** The host's ranges for `text`: `id: srv-42` locked, a note on `note:`'s
   * line (to the text's end, past its last break). */
  const RANGES = `
  window.ranges = (text) => {
    const id = text.indexOf('id: ');
    const note = text.lastIndexOf('note: ');
    return {
      readOnlyRanges: id < 0 ? [] : [{ start: id, end: text.indexOf('\\n', id) }],
      hostDiagnostics: note < 0 ? [] : [{ range: { start: note, end: text.length - 1 }, message: 'note checked', severity: 'info' }],
    };
  };
  `;

  /** Types a comment line at the top and at once `ZZ` after the locked
   * value, then deletes 20 characters back from the note's end, each echo
   * coming back 300 ms late with its ranges. */
  async function typeAroundRanges(page) {
    await page.locator('.config-editor__input').focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('# a long comment line here\n', { delay: 15 });
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.type('ZZ', { delay: 15 });
    await page.waitForTimeout(800);
    await page.keyboard.press('Control+End');
    await page.keyboard.press('ArrowLeft');
    for (let index = 0; index < 20; index += 1) {
      await page.keyboard.press('Backspace');
      await page.waitForTimeout(15);
    }
    await page.waitForTimeout(800);
  }


  test.describe('a file picker over one editor', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const host of ['editor', 'one document']) {
    test(`${host} with echoes: going back to a file applies its text, and nothing reports another file's text`, async ({ page }) => {
        test.slow();
        await open(page);
        const failures = [];
        for (const parent of PARENTS) {
          for (const gap of [50, 600]) {
            await page.evaluate(
              async ([host, script]) => {
                new Function(script)();
                const element = window.mountPoint();
                if (host === 'editor') {
                  const editor = new window.module.ConfigEditor(element, {
                    value: window.files.A,
                    echoes: true,
                    onChange: (text) => window.stored(text),
                  });
                  window.render = () => editor.update({ value: window.files[window.current] });
                } else {
                  const list = () => [{ id: 'main', fileName: 'main.yaml', value: window.files[window.current] }];
                  const view = new window.module.ConfigDocuments(element, {
                    documents: list(),
                    echoes: true,
                    onChange: (_, text) => window.stored(text),
                  });
                  window.render = () => view.update({ documents: list() });
                }
                await window.pause(400);
              },
              [host, parentScript(parent)],
            );
            const picked = await pickFiles(page, gap);
            if (JSON.stringify(picked) !== JSON.stringify(PICKED)) failures.push({ parent, gap, ...picked });
          }
        }
        expect(failures).toEqual([]);
        expect(page.errors).toEqual([]);
      });
    }

    for (const name of FRAMEWORK_PAGES) {
      for (const parent of PARENTS) {
        test(`${name}, a ${parent} parent: going back to a file applies its text`, async ({ page }) => {
          const failures = [];
          for (const gap of [50, 600]) {
            await openFramework(page, name);
            await page.evaluate(async (script) => {
              new Function(script)();
              window.render = () => window.bind(window.files[window.current], window.stored);
              window.render();
              await window.pause(300);
            }, parentScript(parent));
            const picked = await pickFiles(page, gap);
            if (JSON.stringify(picked) !== JSON.stringify(PICKED)) failures.push({ gap, ...picked });
          }
          expect(failures).toEqual([]);
          expect(page.errors).toEqual([]);
        });
      }
    }
  });

  test.describe('ranges given with a skipped echo', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const host of ['editor', 'one document']) {
    test(`${host} with echoes: the lock holds, typing is kept, and no update throws`, async ({ page }) => {
        await open(page);
        await page.evaluate(
          async ([host, start, ranges]) => {
            new Function(ranges)();
            const element = window.mountPoint();
            window.held = start;
            if (host === 'editor') {
              const editor = new window.module.ConfigEditor(element, {
                value: start,
                echoes: true,
                ...window.ranges(start),
                onChange: (text) =>
                  setTimeout(() => {
                    window.held = text;
                    editor.update({ value: text, ...window.ranges(text) });
                  }, 300),
              });
              window.shown = () => editor.value;
            } else {
              const list = () => [{ id: 'a', fileName: 'a.yaml', value: window.held, ...window.ranges(window.held) }];
              const view = new window.module.ConfigDocuments(element, {
                documents: list(),
                echoes: true,
                onChange: (_, text) =>
                  setTimeout(() => {
                    window.held = text;
                    view.update({ documents: list() });
                  }, 300),
              });
              window.shown = () => view.editor('a').value;
            }
            await window.pause(400);
          },
          [host, START, RANGES],
        );
        await typeAroundRanges(page);
        expect(await page.evaluate(() => ({ text: window.shown(), held: window.held }))).toEqual({ text: TYPED, held: TYPED });
        expect(page.errors).toEqual([]);
      });
    }

    for (const name of FRAMEWORK_PAGES) {
      test(`${name}, a parent 300 ms late: the lock holds, typing is kept, and no update fails`, async ({ page }) => {
        await openFramework(page, name);
        await page.evaluate(
          async ([start, ranges]) => {
            new Function(ranges)();
            window.held = start;
            const onChange = (text) =>
              setTimeout(() => {
                window.held = text;
                window.bind(text, onChange, window.ranges(text));
              }, 300);
            window.bind(start, onChange, window.ranges(start));
            await window.pause(400);
          },
          [START, RANGES],
        );
        await typeAroundRanges(page);
        expect({ text: await shownText(page), held: await page.evaluate(() => window.held) }).toEqual({ text: TYPED, held: TYPED });
        expect(page.errors).toEqual([]);
      });
    }

    test('rangesFor: ranges checked for an older text are checked in it and moved to the document', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const older = 'name: web\nport: 80\n';
      const editor = new window.module.ConfigEditor(window.mountPoint(), { value: '# added\nname: web\n' });
      const port = older.indexOf('port');
        const thrown = (apply) => {
          try {
            apply();
            return null;
          } catch (error) {
            return String(error.message);
          }
        };
        // Valid for the older text, past the document's end: no throw.
        const valid = thrown(() =>
          editor.update({
            rangesFor: older,
            readOnlyRanges: [{ start: 0, end: 4 }],
            hostDiagnostics: [{ range: { start: port, end: older.length - 1 }, message: 'port checked' }],
        }),
        );
        await window.pause(200);
        return {
          valid,
          past: thrown(() => editor.update({ rangesFor: older, readOnlyRanges: [{ start: 0, end: older.length + 1 }] })),
          notText: thrown(() => editor.update({ rangesFor: true })),
        };
      });
      expect(result.valid).toBeNull();
      expect(result.past).toContain("past the text's end (19)");
    expect(result.notText).toContain('rangesFor');
    expect(page.errors).toEqual([]);
      expect(page.warnings).toEqual([]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // The handle's `setValue` wins over echoes
  // still on their way in every framework (React's render right after it
  // listed the text it replaced, which rolled it back); a `replaceValue`
  // leaves the language another document is edited as; a host reordering the
  // tabs keeps the active one in view; keys pressed right after a focus move
  // land where it moved (sent back to back, as a fast typist's).
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

  // --- setValue against echoes still on their way. --------------------

  /** The orders the held echoes come back in: all at once backwards or in
   * order, or one at a time backwards. */
  const ORDERS = {
    backwards: (count) => [[...Array(count).keys()].reverse()],
    'in order': (count) => [[...Array(count).keys()]],
    'one by one backwards': (count) => [...Array(count).keys()].reverse().map((index) => [index]),
  };

  /** The main document's first line in the editor, and in the parent's list. */
  const main = (page) =>
    page.evaluate(() => ({
      text: window.documentsHandle.current.editor('main').value.split('\n')[0],
      parent: window.currentFiles().find((file) => file.id === 'main').value.split('\n')[0],
    }));


  for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
    test.describe(`${name}: setValue while echoes are on their way`, () => {
      test.skip(!built, 'js packages not built: run `npm run build` in js/');

      for (const [order, groups] of Object.entries(ORDERS)) {
        test(`the text set stands, the echoes handed back ${order}`, async ({ page }) => {
          await open(page, name, '?manual');
          await page.click('[role="tab"][data-document-id="main"]');
          await page.keyboard.press('Enter');
          await page.keyboard.press('End');
          await page.keyboard.type('123', { delay: 60 });
          await page.waitForTimeout(300);
          expect(await page.evaluate(() => window.documentsHandle.current.setValue('main', 'listen: 1\n'))).toBe(true);
          await page.waitForTimeout(200);
          const held = await page.evaluate(() => window.heldEchoes());
          for (const group of groups(held)) {
            await page.evaluate((group) => window.releaseEchoes(group), group);
            await page.waitForTimeout(100);
          }
          // What the editor sends again (at once, or after the pause), until
          // nothing more comes.
          for (let released = held; ; ) {
            await page.waitForTimeout(400);
            const now = await page.evaluate(() => window.heldEchoes());
            if (now === released) break;
            expect(now - held).toBeLessThan(6);
            for (; released < now; released += 1) {
              await page.evaluate((index) => window.releaseEchoes([index]), released);
              await page.waitForTimeout(50);
            }
          }
          expect(await main(page)).toEqual({ text: 'listen: 1', parent: 'listen: 1' });
          expect(page.errors).toEqual([]);
        });
      }
    });
  }
});

// A group of its own (its own helpers).
test.describe(() => {
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  function listen(page) {
    page.errors = [];
    page.on('console', (message) => {
      if (message.type() === 'error') page.errors.push(message.text());
    });
    page.on('pageerror', (error) => page.errors.push(String(error)));
  }

  async function open(page) {
    listen(page);
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.editor?.destroy();
      window.documentsView?.destroy();
      window.module = await import('../config-editor/index.js');
      window.tools = await import('../config-editor/tools.js');
      window.mountPoint = () => {
        window.mounted?.destroy();
        document.querySelectorAll('[data-host]').forEach((element) => element.remove());
        const element = document.createElement('div');
        element.dataset.host = '';
        element.style.height = '400px';
        document.body.prepend(element);
        return element;
      };
    });
  }

  const input = (page) => page.locator('[data-host] .config-editor__input:visible');

  /** The caret at the end of the `nth` `marker` in the text on screen (or at `offset` past its start). */
  async function caretAt(page, marker, offset = marker.length) {
    await input(page).evaluate(
      (element, [marker, offset]) => {
        const at = element.value.indexOf(marker);
        if (at < 0) throw new Error(`no ${marker} in ${element.value}`);
        element.focus();
        element.setSelectionRange(at + offset, at + offset);
      },
      [marker, offset],
    );
  }


  // --- in Edit-as, `update({ value })` with the text the host stored
  // from onChange, during the typing pause, keeps what was typed (the update
  // first sends the edit to onChange, then used to apply the older value), with
  // and without echoes; one call, and a host rendering on a timer while the
  // user types. -------------------------------------------------------------------

  for (const echoes of [false, true]) {
    test(`a stored text given back during the Edit-as pause keeps the typing (echoes ${echoes})`, async ({ page }) => {
      test.skip(!built, 'run `npm run build` in js/ first');
      await open(page);
      // One call, right after a key.
      await page.evaluate((echoes) => {
        window.store = { text: '{\n  "name": "a",\n  "port": 1\n}\n' };
        window.mounted = new module.ConfigEditor(mountPoint(), {
          language: 'json',
          editAs: 'yaml',
          value: store.text,
          echoes,
          onChange: (text) => {
            store.text = text;
          },
        });
      }, echoes);
      await caretAt(page, 'name: a');
      await page.keyboard.type('X');
      await page.evaluate(() => mounted.update({ value: store.text }));
      await expect(input(page)).toHaveValue('name: aX\nport: 1\n');
      expect(await page.evaluate(() => [mounted.value, store.text])).toEqual([
        '{\n  "name": "aX",\n  "port": 1\n}\n',
        '{\n  "name": "aX",\n  "port": 1\n}\n',
      ]);
      // A host rendering every 120 ms while the user types slowly.
      await page.evaluate((echoes) => {
        window.store = { text: '{\n  "name": "a",\n  "port": 1\n}\n' };
        window.mounted = new module.ConfigEditor(mountPoint(), {
          language: 'json',
          editAs: 'yaml',
          value: store.text,
          echoes,
          onChange: (text) => {
            store.text = text;
          },
        });
        window.timer = setInterval(() => mounted.update({ value: store.text, showFooter: true }), 120);
      }, echoes);
      await caretAt(page, 'name: a');
      for (const character of 'hello world') {
        await page.keyboard.type(character);
        await page.waitForTimeout(45);
      }
      await expect(input(page)).toHaveValue('name: ahello world\nport: 1\n');
      await expect.poll(() => page.evaluate(() => store.text)).toBe('{\n  "name": "ahello world",\n  "port": 1\n}\n');
      await page.evaluate(() => clearInterval(timer));
      expect(page.errors).toEqual([]);
    });
  }

  // --- with echoes, a store that writes each text another way (the
  // byte order mark dropped, LF for CRLF) hands back echoes the editor still
  // tells apart: typing in a BOM and CRLF document is never rolled back, and
  // the document keeps its own form. -----------------------------------------------

  for (const store of ['strip-bom', 'crlf-to-lf']) {
    test(`echoes written another way keep the typing (${store})`, async ({ page }) => {
      test.skip(!built, 'run `npm run build` in js/ first');
      await open(page);
      await page.evaluate((store) => {
        const rewrite =
          store === 'strip-bom' ? (text) => text.replace(/^﻿/, '') : (text) => text.replace(/\r\n/g, '\n');
        // Late, out of order: each echo after a delay that shrinks as typing goes on.
        let delay = 400;
        window.mounted = new module.ConfigEditor(mountPoint(), {
          value: '﻿a: 1\r\nb: 2\r\n',
          echoes: true,
          onChange: (text) => {
            const echo = rewrite(text);
            setTimeout(() => mounted.update({ value: echo }), delay);
            delay = Math.max(50, delay - 80);
          },
        });
      }, store);
      await caretAt(page, 'a: 1');
      for (const character of '23456') {
        await page.keyboard.type(character);
        await page.waitForTimeout(40);
      }
      await page.waitForTimeout(900);
      expect(await page.evaluate(() => mounted.value)).toBe('﻿a: 123456\r\nb: 2\r\n');
      expect(page.errors).toEqual([]);
    });
  }

  // --- The revision protocol: a host storing each `onChange` text with its
  // revision 0-200 ms late, keeping only the newest revision, and rendering
  // several stored texts at once (the document's own echo dropped in the
  // render) passes back `valueRevision`: no pause is needed, the host ends
  // on the document, and it hears each text at most once more. -------------

  test('a host passing revisions back, its renders dropping echoes, ends on the document unflooded', async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    await open(page);
    await page.evaluate(() => {
      window.store = { value: 'a: 1\n', revision: -1 };
      window.told = 0;
      let seed = 7;
      const random = (bound) => (seed = (seed * 1103515245 + 12345) % 2147483648) % bound;
      let render = null;
      window.mounted = new module.ConfigEditor(mountPoint(), {
        value: 'a: 1\n',
        echoes: true,
        onChange: (text, revision) => {
          window.told += 1;
          setTimeout(() => {
            // The store keeps the newest revision only.
            if (revision > window.store.revision) window.store = { value: text, revision };
            // Renders batch: one per 30 ms gives what the store holds then.
            render ??= setTimeout(() => {
              render = null;
              mounted.update({ value: window.store.value, valueRevision: window.store.revision });
            }, 30);
          }, random(200));
        },
      });
    });
    await caretAt(page, 'a: 1');
    await page.keyboard.type('234567', { delay: 25 });
    await page.keyboard.press('Backspace');
    await page.keyboard.type('XY', { delay: 25 });
    await page.waitForTimeout(800);
    const { value, store, told } = await page.evaluate(() => ({
      value: mounted.value,
      store: window.store.value,
      told: window.told,
    }));
    expect(value).toBe('a: 123456XY\n');
    expect(store).toBe(value);
    expect(told).toBeLessThanOrEqual(2 * 9);
    expect(page.errors).toEqual([]);
  });
});

// A v0.1.0 host (no `valueRevision`) handing every text back late, its
// editor hidden and shown again where the editor knows it shows the same
// document: a `ConfigEditor` made in the element one was destroyed in, a
// `ConfigDocuments` document listed again with its id. The new editor
// takes over the texts the gone one sent: their late answers keep what is
// typed in it, and with nothing typed there they bring the gone editor's
// typing back. Before, an answer still to come was the host's own text for
// the new editor and replaced the typing. A conditional render makes the
// new editor in a new element: with the same `echoKey` it takes the texts
// over just the same (`keyed editor`); with none (`editor elsewhere`) an
// answer still to come is the host's own text, as before.
test.describe('shown again where the editor was, with answers still to come', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  for (const host of ['editor', 'keyed editor', 'editor elsewhere', 'documents']) {
    for (const typed of [true, false]) {
      // Known: nothing tells the new editor the answers are the gone one's.
      const asBefore = host === 'editor elsewhere';
      const title = asBefore
        ? typed
          ? 'a late answer replaces the typing in the new editor'
          : "the gone editor's typing comes back"
        : typed
          ? 'the typing in the new editor is kept'
          : "the gone editor's typing comes back";
      test(`${host}: ${title}`, async ({ page }) => {
        await open(page);
        await page.evaluate((host) => {
          let element = window.mountPoint();
          window.held = [];
          window.store = { a: 'name: x\n', b: 'other: 1\n' };
          const list = (ids) => ids.map((id) => ({ id, fileName: `${id}.yaml`, value: window.store[id] }));
          const answer = (apply) => window.held.push(apply);
          // Hands back every text held, in order, until none is left; the
          // editor's text after each.
          window.answerAll = async () => {
            const seen = [];
            while (window.held.length) {
              window.held.shift()();
              seen.push(window.textOf());
              await window.pause(20);
              if (!window.held.length) await window.pause(400);
            }
            return seen;
          };
          if (host !== 'documents') {
            // A conditional render makes a new element for each editor.
            const where = () => {
              if (host === 'editor') return element;
              element.remove();
              element = window.mountPoint();
              return element;
            };
            const make = () =>
              new window.module.ConfigEditor(where(), {
                value: window.store.a,
                echoes: true,
                echoKey: host === 'keyed editor' ? 'main' : undefined,
                onChange: (text) =>
                  answer(() => {
                    window.store.a = text;
                    window.shown.update({ value: text });
                  }),
              });
            window.shown = make();
            window.textOf = () => window.shown.value;
            window.hideAndShow = () => {
              window.shown.destroy();
              window.shown = make();
            };
            return;
          }
          let ids = ['a', 'b'];
          const view = new window.module.ConfigDocuments(element, {
            documents: list(ids),
            echoes: true,
            onChange: (id, text) =>
              answer(() => {
                window.store[id] = text;
                view.update({ documents: list(ids) });
              }),
          });
          window.textOf = () => view.editor('a').value;
          window.hideAndShow = async () => {
            ids = ['b'];
            view.update({ documents: list(ids) });
            await window.pause(100);
            ids = ['a', 'b'];
            view.update({ documents: list(ids) });
            view.select('a');
          };
        }, host);
        const typeAtEnd = async (text) => {
          const input = page.locator('.config-editor__input:visible');
          await input.focus();
          await input.evaluate((element) => element.setSelectionRange(element.value.length, element.value.length));
          await page.keyboard.type(text, { delay: 20 });
        };
        await page.waitForTimeout(300);
        await typeAtEnd('ab');
        await page.evaluate(() => window.hideAndShow());
        await page.waitForTimeout(300);
        // The host's text when it was shown again: its answers are still to come.
        expect(await page.evaluate(() => window.textOf())).toBe('name: x\n');
        if (typed) await typeAtEnd('c');
        const seen = await page.evaluate(() => window.answerAll());
        const expected = typed ? 'name: x\nc' : 'name: x\nab';
        if (asBefore && typed) {
          // The first answer, the gone editor's, is the host's own text.
          expect(seen[0]).toBe('name: x\na');
          expect(page.errors).toEqual([]);
          return;
        }
        // Typed in the new editor, no answer ever replaces it, not even for a while.
        if (typed) expect(seen.filter((text) => text !== expected)).toEqual([]);
        expect(await page.evaluate(() => [window.textOf(), window.store.a])).toEqual([expected, expected]);
        expect(page.errors).toEqual([]);
      });
    }
  }
});
