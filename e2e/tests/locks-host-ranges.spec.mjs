import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// Locks given by the host as ranges: given in the mounting task, after a re-mount, for a text two edits
// or many lines behind, given again unchanged.

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
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.saved = [];
    window.mountPoint = () => {
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '300px';
      document.body.prepend(element);
      return element;
    };
  });
}

const input = (page) => page.locator('.config-editor__input:visible').first();

const shownText = (page) => input(page).inputValue();

/** Types `key` right after the first `marker` of the text on screen. */
async function typeAfter(page, marker, key) {
  await input(page).evaluate((element, marker) => {
    const at = element.value.indexOf(marker);
    if (at < 0) throw new Error(`not on screen: ${marker}`);
    element.focus();
    element.setSelectionRange(at + marker.length, at + marker.length);
  }, marker);
  await page.keyboard.type(key);
}

// --- locks given by `update` in the task that mounts the editor apply
// as if given at construction (`ConfigEditor`, `ConfigDocuments`, a
// component's re-mount). ------------------------------------------------------

const VALUE = 'a: 11\nb: 22\nc: 33\n';

const LINE_ONE = '[{ startLine: 1, startColumn: 1, endLine: 1, endColumn: 6 }]';

const FIRST_FIVE = '[{ start: 0, end: 5 }]';

/** Types into the locked `a: 11` and into `b: 22`, then saves: the lock
 * refuses the first, the second is kept, and no saved text has the first. */
async function expectLineOneLocked(page) {
  await typeAfter(page, 'a: 1', '9');
  await typeAfter(page, 'b: 2', '7');
  await page.keyboard.press('Control+s');
  await page.waitForTimeout(100);
  const text = await shownText(page);
  const saved = await page.evaluate(() => window.saved);
  expect({ text, lockedSaved: saved.some((value) => value.includes('a: 191')) }).toEqual({
    text: expect.stringMatching(/^a: 11\nb: 272\n/),
    lockedSaved: false,
  });
}

// What the host runs right after `new ConfigEditor(…, { value: VALUE })`.
const EDITOR_CASES = {
  'ranges by line and column': `editor.update({ readOnlyRanges: ${LINE_ONE} })`,
  'ranges by index': `editor.update({ readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges with the same value': `editor.update({ value: ${JSON.stringify(VALUE)}, readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges with a new value': `editor.update({ value: ${JSON.stringify(VALUE + 'd: 44\n')}, readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges for another text': `editor.update({ rangesFor: ${JSON.stringify('# x\n' + VALUE)}, readOnlyRanges: [{ start: 4, end: 9 }] })`,
  'ranges for the document': `editor.update({ rangesFor: ${JSON.stringify(VALUE)}, readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges after a theme': `editor.update({ theme: 'nord' }); editor.update({ readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges after paths': `editor.update({ readOnlyPaths: ['/c'] }); editor.update({ readOnlyRanges: ${FIRST_FIVE} })`,
  'ranges replacing ranges': `editor.update({ readOnlyRanges: [{ start: 6, end: 11 }] }); editor.update({ readOnlyRanges: ${FIRST_FIVE} })`,
  'a value, then ranges': `editor.update({ value: ${JSON.stringify(VALUE)} }); editor.update({ readOnlyRanges: ${FIRST_FIVE} })`,
};

/** A framework page whose parent the test plays: `window.bind(value,
 * onChange, props)` renders the component with them. */
async function openFramework(page, name) {
  listen(page);
  // `?random`: Svelte's one-way `value` (the test's parent hands it back).
  await page.goto(example(name) + '?random');
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  await page.evaluate((vue) => {
    window.bind = (value, onChange, props = {}) =>
      window.setExtraProps(vue ? { modelValue: value, 'onUpdate:modelValue': onChange, ...props } : { value, onChange, ...props });
  }, name.startsWith('vue'));
}

// A prop the component is built from: changing it mounts a new editor.
const REMOUNTS = {
  schema: { schema: '{"type":"object"}' },
  fileName: { fileName: 'other.yaml' },
  'language spelling': { language: 'yml' },
};

// --- ranges given for a text several separate edits away are moved
// exactly. ----------------------------------------------------------------------

/** Whether typing at the end of each of `lines` (one-based) is refused;
 * a key that is taken is undone. */
async function refusedOn(page, lines) {
  const refused = [];
  for (const line of lines) {
    const before = await shownText(page);
    await input(page).evaluate((element, line) => {
      const rows = element.value.split('\n');
      const at = rows.slice(0, line).reduce((sum, row) => sum + row.length + 1, 0) - 1;
      element.focus();
      element.setSelectionRange(at, at);
    }, line);
    await page.keyboard.type('Z');
    const after = await shownText(page);
    refused.push(after === before);
    if (after !== before) await page.keyboard.press('Control+z');
  }
  return refused;
}

// One section per finding below.
test.describe('locks given in the task that mounts the editor', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, code] of Object.entries(EDITOR_CASES)) {
    test(`ConfigEditor, ${name}, right after new: the lock holds and nothing locked is saved`, async ({ page }) => {
      await open(page);
      await page.evaluate(
        ([value, code]) => {
          const editor = new window.module.ConfigEditor(window.mountPoint(), {
            value,
            onSave: (text) => window.saved.push(text),
          });
          window.editor = editor;
          new Function('editor', code)(editor);
        },
        [VALUE, code],
      );
      await page.waitForTimeout(300);
      await expectLineOneLocked(page);
      expect(page.errors).toEqual([]);
    });
  }

  test('ConfigEditor, ranges given from the first onProblems: the lock holds', async ({ page }) => {
    await open(page);
    await page.evaluate((value) => {
      let editor = null;
      let first = true;
      editor = new window.module.ConfigEditor(window.mountPoint(), {
        value,
        onSave: (text) => window.saved.push(text),
        onProblems: () => {
          if (!first) return;
          first = false;
          // From inside the constructor there is no editor yet: then the
          // ranges come right after it returns, in the same task.
          if (editor) editor.update({ readOnlyRanges: [{ start: 0, end: 5 }] });
          else queueMicrotask(() => editor.update({ readOnlyRanges: [{ start: 0, end: 5 }] }));
        },
      });
      window.editor = editor;
    }, VALUE);
    await page.waitForTimeout(300);
    await expectLineOneLocked(page);
    expect(page.errors).toEqual([]);
  });

  // How the host shows document `b` and gives it its ranges, in one task.
  const DOCUMENTS_CASES = {
    'select, then ranges': 'view.select("b"); view.update({ documents: locked });',
    'activeId, then ranges': 'view.update({ activeId: "b" }); view.update({ documents: locked });',
    'activeId and ranges in one update': 'view.update({ activeId: "b", documents: locked });',
    'ranges, then select': 'view.update({ documents: locked }); view.select("b");',
    'new view on b, then ranges': 'view.update({ documents: locked });',
  };
  for (const [name, code] of Object.entries(DOCUMENTS_CASES)) {
    test(`ConfigDocuments, ${name}: the lock holds and nothing locked is saved`, async ({ page }) => {
      await open(page);
      await page.evaluate(
        async ([value, code, name]) => {
          const plain = [
            { id: 'a', fileName: 'a.yaml', value: 'x: 1\n' },
            { id: 'b', fileName: 'b.yaml', value },
          ];
          const locked = [plain[0], { ...plain[1], readOnlyRanges: [{ start: 0, end: 5 }] }];
          const fresh = name.startsWith('new view');
          const view = new window.module.ConfigDocuments(window.mountPoint(), {
            documents: plain,
            activeId: fresh ? 'b' : 'a',
            onSave: (id, text) => window.saved.push(text),
          });
          window.documentsView = view;
          if (!fresh) await window.pause(100);
          new Function('view', 'locked', code)(view, locked);
        },
        [VALUE, code, name],
      );
      await page.waitForTimeout(300);
      await expectLineOneLocked(page);
      expect(page.errors).toEqual([]);
    });
  }
});

test.describe('a component re-mount keeps its locks', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const name of FRAMEWORK_PAGES) {
    for (const [remount, props] of Object.entries(REMOUNTS)) {
      test(`${name}, a re-mount by ${remount} past the bound value: the lock holds`, async ({ page }) => {
        await openFramework(page, name);
        await page.evaluate(async (value) => {
          window.saved = [];
          // The parent never hands an edit back: the bound value lags.
          window.props = { readOnlyRanges: [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }], fileName: 'a.yaml', language: 'yaml' };
          window.bind(value, () => {}, window.props);
          await new Promise((resolve) => setTimeout(resolve, 300));
        }, VALUE);
        await typeAfter(page, 'c: 3', '5');
        await page.evaluate(
          async ([value, props]) => {
            window.bind(value, () => {}, { ...window.props, ...props });
            await new Promise((resolve) => setTimeout(resolve, 400));
          },
          [VALUE, props],
        );
        expect(await shownText(page)).toBe('a: 11\nb: 22\nc: 353\n');
        await typeAfter(page, 'a: 1', '9');
        await typeAfter(page, 'b: 2', '7');
        expect(await shownText(page)).toBe('a: 11\nb: 272\nc: 353\n');
        expect(page.errors).toEqual([]);
      });
    }
  }
});

test.describe('ranges for a text two edits away', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const breaks of ['LF', 'CRLF']) {
    test(`${breaks}: a lock and a diagnostic between two edits land on their own lines`, async ({ page }) => {
      await open(page);
      let checked = '';
      for (let line = 1; line <= 40; line++) checked += `key${line}: ${line * 11}\n`;
      if (breaks === 'CRLF') checked = checked.replace(/\n/g, '\r\n');
      await page.evaluate((value) => {
        window.editor = new window.module.ConfigEditor(window.mountPoint(), { value });
      }, checked);
      // The user types on lines 2 and 30 while the host checks `checked`.
      await typeAfter(page, 'key2: 22', '0');
      await typeAfter(page, 'key30: 330', '0');
      await page.evaluate((checked) => {
        window.editor.update({
          rangesFor: checked,
          readOnlyRanges: [{ startLine: 10, startColumn: 1, endLine: 10, endColumn: 11 }],
          hostDiagnostics: [{ range: { startLine: 20, startColumn: 1, endColumn: 6 }, message: 'host warning', severity: 'warning' }],
        });
      }, checked);
      await page.waitForTimeout(300);
      expect(await page.locator('.config-editor__diagnostic-marker-host').count()).toBe(1);
      expect(await refusedOn(page, [3, 9, 10, 11, 20, 29])).toEqual([false, false, true, false, false, false]);
      expect(page.errors).toEqual([]);
    });
  }
});

// A group of its own (its own helpers).
test.describe(() => {
  // One section per finding below.
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
        document.querySelectorAll('[data-host]').forEach((element) => element.remove());
        const element = document.createElement('div');
        element.dataset.host = '';
        element.style.height = '400px';
        document.body.prepend(element);
        return element;
      };
    });
  }


  // --- the async-check flow after Replace All over 300 lines: the
  // host's locks and problems for the text it checked stay on their own
  // lines (past ~512 diff steps everything between the first and last change
  // was one changed stretch, locked and underlined whole). ----------------------

  test.describe('ranges for a text many lines behind', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Replace All over 300 lines, then ranges for the text before it: only the named fields are locked', async ({ page }) => {
      await open(page);
      await page.evaluate(() => {
        let text = 'services:\n';
      for (let index = 0; index < 300; index++) {
          text += `  svc${index}:\n    image: nginx:1.0\n    port: ${8000 + index}\n`;
        }
        window.checked = text;
        window.editor = new window.module.ConfigEditor(window.mountPoint(), { value: text, showFooter: true });
      });
      await page.waitForTimeout(300);
      const input = page.locator('[data-host] .config-editor__input');
    await input.click();
      await page.keyboard.press('Control+h');
    await page.keyboard.type('nginx:1.0');
    await page.locator('[data-host] .config-editor__replace-input').click();
    await page.keyboard.type('nginx:1.1');
    await page.keyboard.press('Control+Alt+Enter');
    await page.keyboard.press('Escape');
      expect(await page.evaluate(() => (window.editor.value.match(/nginx:1\.1/g) || []).length)).toBe(300);
      await page.evaluate(() => {
        const lines = window.checked.split('\n');
        const line = (text) => lines.indexOf(text) + 1;
        const port = (number) => ({ startLine: line(`    port: ${number}`), startColumn: 5, endColumn: 15 });
        window.editor.update({
          rangesFor: window.checked,
          readOnlyRanges: [port(8005), port(8100)],
          hostDiagnostics: [{ range: port(8007), message: 'port taken' }],
      });
      });
      await page.waitForTimeout(300);
      const locked = await page.locator('[data-host] .config-editor__locked').allTextContents();
      expect(locked).toEqual(['port: 8005']);
    const marked = await page.locator('[data-host] .config-editor__diagnostic-marker').count();
    expect(marked).toBe(1);
      // A port between the two locks types.
      await input.evaluate((element) => {
        const at = element.value.indexOf('port: 8001') + 10;
      element.focus();
        element.setSelectionRange(at, at);
      });
      await page.keyboard.type('9');
      expect(await page.evaluate(() => window.editor.value.includes('port: 80019'))).toBe(true);
      // And one past the far lock.
      await input.evaluate((element) => {
        const at = element.value.indexOf('port: 8200') + 10;
      element.setSelectionRange(at, at);
      });
      await page.keyboard.type('9');
      expect(await page.evaluate(() => window.editor.value.includes('port: 82009'))).toBe(true);
      expect(page.errors).toEqual([]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // One section per finding below.
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

  // --- ranges the host gives again unchanged (a ConfigEditor host
  // passing all its options again with the `value` onChange gave; the README's
  // ConfigDocuments `{ ...document, ...change }` list) keep where the editor
  // moved them: the lock stays on its field and its value saves unchanged, in
  // every language, for index, line/column and `rangeOf` ranges, after edits
  // above, on the line before, and below the locked line. ------------------------

  const LOCKED = {
    yaml: {
      fileName: 'm.yaml',
      text: 'name: a\nhost: h\nport: 80\n',
      path: '/host',
      locked: 'host: h',
      above: ['name: a', '\nx: 1'],
      before: ['name: a', 'bc'],
      below: ['port: 80', '\nz: 9'],
    },
    json: {
      fileName: 'm.json',
      text: '{\n  "name": "a",\n  "host": "h",\n  "port": 80\n}\n',
      path: '/host',
      locked: '"host": "h"',
      above: ['"name": "a",', '\n"x": 1,'],
      before: ['"name": "a', 'bc'],
      below: ['"port": 80', ', "z": 9'],
    },
    toml: {
      fileName: 'm.toml',
      text: 'name = "a"\nhost = "h"\nport = 80\n',
      path: '/host',
      locked: 'host = "h"',
      above: ['name = "a"', '\nx = 1'],
      before: ['name = "a', 'bc'],
      below: ['port = 80', '\nz = 9'],
    },
    hcl: {
      fileName: 'm.tf',
      text: 'name = "a"\nhost = "h"\nport = 80\n',
      path: '/host',
      locked: 'host = "h"',
      above: ['name = "a"', '\nx = 1'],
      before: ['name = "a', 'bc'],
      below: ['port = 80', '\nz = 9'],
    },
    xml: {
      fileName: 'm.xml',
      text: '<c>\n  <name>a</name>\n  <host>h</host>\n  <port>80</port>\n</c>\n',
      path: '/c/host',
      locked: '<host>h</host>',
      above: ['<name>a</name>', '\n<x>1</x>'],
      before: ['<name>a', 'bc'],
      below: ['<port>80</port>', '<z>9</z>'],
    },
  };

  /** Mounts `language`'s document locked by a range in `style`, its host giving every option again (`pattern`). */
  async function mountLocked(page, language, style, pattern) {
    await page.evaluate(
      ([language, style, pattern, sample]) => {
        const { text, path, fileName } = sample;
        const found = tools.rangeOf(language, text, path);
        const range =
          style === 'indices'
            ? { start: found.start, end: found.end }
            : style === 'lines'
              ? { startLine: found.startLine, startColumn: found.startColumn, endLine: found.endLine, endColumn: found.endColumn }
              : found;
        window.saved = [];
        if (pattern === 'documents') {
          let files = [{ id: 'm', fileName, value: text, readOnlyRanges: [range] }];
          const update = (id, change) => {
            files = files.map((file) => (file.id === id ? { ...file, ...change } : file));
            mounted.update({ documents: files });
          };
          window.mounted = new module.ConfigDocuments(mountPoint(), {
            documents: files,
            onChange: (id, value) => update(id, { value }),
            onSave: (id, value) => saved.push(value),
          });
          window.current = () => mounted.editor('m').value;
          window.saveNow = () => mounted.save('m');
        } else {
          const element = mountPoint();
          const props = { language, value: text, readOnlyRanges: [range], onSave: (value) => saved.push(value) };
          props.onChange = (value) => {
            props.value = value;
            queueMicrotask(() => mounted.update({ ...props }));
          };
          window.mounted = new module.ConfigEditor(element, props);
          window.current = () => mounted.value;
          window.saveNow = () => mounted.save();
        }
      },
      [language, style, pattern, LOCKED[language]],
    );
  }


  for (const pattern of ['editor', 'documents']) {
    test(`ranges given again unchanged keep the lock on its field (${pattern})`, async ({ page }) => {
      test.skip(!built, 'run `npm run build` in js/ first');
      test.setTimeout(240_000);
      await open(page);
      const failures = [];
      for (const [language, sample] of Object.entries(LOCKED)) {
        for (const style of ['indices', 'lines', 'rangeOf']) {
          for (const edit of ['above', 'before', 'below']) {
            const name = `${language} ${style} ${edit}`;
            await mountLocked(page, language, style, pattern);
            const [marker, typed] = sample[edit];
            await caretAt(page, marker);
            for (const part of typed.split(/(\n)/)) {
              if (part === '\n') await page.keyboard.press('Enter');
              else if (part) await page.keyboard.type(part);
            }
            await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
            const edited = await page.evaluate(() => current());
            // Typing into the locked value is refused.
            await caretAt(page, sample.locked, sample.locked.lastIndexOf('h') + 1);
            await page.keyboard.type('ACKED');
            await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
            const after = await page.evaluate(() => current());
            if (after !== edited) failures.push(`${name}: the locked value took typing: ${JSON.stringify(after)}`);
            if (!edited.includes(sample.locked)) failures.push(`${name}: the locked text changed: ${JSON.stringify(edited)}`);
            const saved = await page.evaluate(() => {
              saveNow();
              return saved.at(-1) ?? null;
            });
            if (saved !== null && (!saved.includes(sample.locked) || saved.includes('ACKED'))) {
              failures.push(`${name}: saved ${JSON.stringify(saved)}`);
            }
          }
        }
      }
      expect(failures).toEqual([]);
      expect(page.errors).toEqual([]);
    });
  }

  // --- the shipped example's "Lock line 1", then Ctrl+Home, Enter,
  // ArrowUp and a comment typed: the new line is free and the locked line
  // stays locked (the line break used to join the lock, and every key after
  // it was refused). -------------------------------------------------------------

  test('a line added above the shipped example\'s locked line is free', async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    listen(page);
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('#lock-first-line').check();
    const box = page.locator('.config-editor__input').first();
    const before = await box.inputValue();
    const first = before.split('\n')[0];
    await box.click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.type('# added above');
    await expect(box).toHaveValue(`# added above\n${before}`);
    // The original first line is still locked.
    await box.evaluate((element, first) => {
      const at = element.value.indexOf(first) + first.length;
      element.focus();
      element.setSelectionRange(at, at);
    }, first);
    await page.keyboard.type('Z');
    await expect(box).toHaveValue(`# added above\n${before}`);
    // And the user's own line takes Backspace.
    await box.evaluate((element) => {
      element.focus();
      element.setSelectionRange(13, 13);
    });
    await page.keyboard.press('Backspace');
    await expect(box).toHaveValue(`# added abov\n${before}`);
    expect(page.errors).toEqual([]);
  });
});
