import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 28 (host API). By default every `value` a host gives applies: a
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

test.describe('QA round 28: host API', () => {
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

/** Types the QA's sequence: letters, backspaces, more letters, an undo. */
async function typeSequence(page) {
  await page.keyboard.type('abcdefghij', { delay: 25 });
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.type('XY', { delay: 25 });
  await page.keyboard.press('Control+z');
}

test.describe('QA round 28: hosts that echo', () => {
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
