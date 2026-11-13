import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 30 (host API), one section per finding below.

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

// --- H30-1: locks given by `update` in the task that mounts the editor apply
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

test.describe('QA round 30: locks given in the task that mounts the editor', () => {
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

test.describe('QA round 30: a component re-mount keeps its locks', () => {
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

// --- H30-2: ranges given for a text several separate edits away are moved
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

test.describe('QA round 30: ranges for a text two edits away', () => {
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

// --- L30-3: `update({ value, baseline })` never reports the document dirty.

test.describe('QA round 30: a file loaded with its baseline is never dirty', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // How the host loads `b: 2` over `a: 1`.
  const LOADS = {
    'value and baseline': { value: 'b: 2\n', baseline: 'b: 2\n' },
    'baseline and value': { baseline: 'b: 2\n', value: 'b: 2\n' },
    CRLF: { value: 'b: 2\r\n', baseline: 'b: 2\r\n' },
    'editing as JSON': { value: 'b: 2\n', baseline: 'b: 2\n', editAs: 'json' },
  };
  for (const [name, load] of Object.entries(LOADS)) {
    test(`ConfigEditor, ${name}: clean in the same task, and no dirty flag or change count meanwhile`, async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async (load) => {
        const heard = { dirty: [], changed: [] };
        const { editAs, ...update } = load;
        const editor = new window.module.ConfigEditor(window.mountPoint(), {
          value: 'a: 1\n',
          editAs,
          onDirtyChange: (dirty) => heard.dirty.push(dirty),
          onChangeSummary: (summary) => heard.changed.push(summary.added + summary.removed + summary.modified),
        });
        window.editor = editor;
        await window.pause(200);
        editor.update(update);
        const now = { dirty: editor.isDirty, diff: editor.unifiedDiff() };
        await window.pause(600);
        return { now, heard };
      }, load);
      expect(result).toEqual({ now: { dirty: false, diff: '' }, heard: { dirty: [false], changed: [0] } });
      expect(page.errors).toEqual([]);
    });
  }

  test('ConfigDocuments: a document loaded with its baseline is clean at once, and onDirtyChange stays quiet', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const heard = [];
      const view = new window.module.ConfigDocuments(window.mountPoint(), {
        documents: [
          { id: 'm', fileName: 'm.yaml', value: 'a: 1\n' },
          { id: 'h', fileName: 'h.yaml', value: 'a: 1\n' },
        ],
        onDirtyChange: (id, dirty) => heard.push(`${id} ${dirty}`),
      });
      window.documentsView = view;
      await window.pause(200);
      view.update({
        documents: [
          { id: 'm', fileName: 'm.yaml', value: 'b: 2\n', baseline: 'b: 2\n' },
          { id: 'h', fileName: 'h.yaml', value: 'b: 2\n', baseline: 'b: 2\n' },
        ],
      });
      const now = { m: view.isDirty('m'), editor: view.editor('m').isDirty };
      await window.pause(600);
      return { now, heard: heard.filter((entry) => entry.endsWith('true')) };
    });
    expect(result).toEqual({ now: { m: false, editor: false }, heard: [] });
    expect(page.errors).toEqual([]);
  });
});

// --- L30-4: a byte order mark is a string index but no column, everywhere.

// Documents with a byte order mark, one schema problem on line 1 each.
const SCHEMA = JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } });
const MARKED = {
  yaml: '﻿port: x\n',
  'yaml CRLF': '﻿port: x\r\na: 1\r\n',
  json: '﻿{"port": "x"}\n',
  toml: '﻿port = "x"\n',
  hcl: '﻿port = "x"\n',
};

test.describe('QA round 30: a byte order mark is no column', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, text] of Object.entries(MARKED)) {
    const language = name.split(' ')[0];
    test(`${name}: validate, rangeOf and host ranges by index agree with the editor`, async ({ page }) => {
      await open(page);
      const result = await page.evaluate(
        async ([language, text, schema]) => {
          const tools = await import('../config-editor/tools.js');
          const strip = (problems) => problems.map(({ line, column, endColumn, message }) => ({ line, column, endColumn, message }));
          let own = null;
          const editor = new window.module.ConfigEditor(window.mountPoint(), {
            language,
            value: text,
            schema,
            onProblems: (problems) => (own = problems),
          });
          window.editor = editor;
          await window.pause(500);
          const checked = tools.validate(language, text, schema);
          const shown = editor.rangeOf('/port');
          const found = tools.rangeOf(language, text, '/port');
          const lines = ({ startLine, startColumn, endLine, endColumn }) => ({ startLine, startColumn, endLine, endColumn });
          // The problems a host passes on from `validate` are the editor's own.
          const markers = document.querySelectorAll('.config-editor__diagnostic-marker').length;
          editor.update({
            hostDiagnostics: checked.map((problem) => ({
              range: { startLine: problem.line, startColumn: problem.column, endColumn: problem.endColumn },
              message: problem.message,
            })),
          });
          await window.pause(500);
          return {
            problems: strip(checked),
            own: strip(own),
            lines: lines(found),
            shownLines: lines(shown),
            slices: [text.slice(found.start, found.end), editor.text.slice(shown.start, shown.end)],
            markers: [markers, document.querySelectorAll('.config-editor__diagnostic-marker').length],
          };
        },
        [language, text, SCHEMA],
      );
      expect(result.problems.length).toBe(1);
      expect(result.problems).toEqual(result.own);
      expect(result.lines).toEqual(result.shownLines);
      expect(result.slices[0]).toBe(result.slices[1]);
      expect(result.markers[1]).toBe(result.markers[0]);
      expect(page.errors).toEqual([]);
    });
  }

  for (const rangesFor of ['none', 'the text itself']) {
    test(`a lock by string index in a marked text (rangesFor: ${rangesFor}) is on the character it names`, async ({ page }) => {
      await open(page);
      const text = '﻿a: 1\nb: 2\n';
      await page.evaluate(
        async ([text, rangesFor]) => {
          window.editor = new window.module.ConfigEditor(window.mountPoint(), { value: text });
          await window.pause(200);
          // The `1`, not the line break after it.
          const at = text.indexOf('1');
          window.editor.update({ readOnlyRanges: [{ start: at, end: at + 1 }], ...(rangesFor === 'none' ? {} : { rangesFor: text }) });
        },
        [text, rangesFor],
      );
      await page.waitForTimeout(300);
      await typeAfter(page, 'a: 1', '');
      await page.keyboard.press('Backspace');
      expect(await shownText(page)).toBe('a: 1\nb: 2\n');
      await typeAfter(page, 'b: 2', '');
      await page.keyboard.press('Backspace');
      expect(await shownText(page)).toBe('a: 1\nb: \n');
      expect(page.errors).toEqual([]);
    });
  }
});

// --- L30-5: an option's default is what a view built without it has.

test.describe("QA round 30: an option's default is what a view left without it was built with", () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('update with the default of an option left out changes nothing; another value still throws', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const tried = (target, options) => {
        try {
          target.update(options);
          return 'ok';
        } catch (error) {
          return error.message.split(':')[0];
        }
      };
      const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\n' });
      const view = new window.module.ConfigDocuments(document.createElement('div'), {
        documents: [{ id: 'a', fileName: 'a.yaml', value: '' }],
      });
      const built = new window.module.ConfigEditor(document.createElement('div'), { value: '', className: 'x', echoes: true });
      return {
        editor: [
          { echoes: false },
          { echoes: null },
          { className: null },
          { className: '' },
          { fileName: null },
          { fileName: '' },
          { fileName: 'untitled.yaml' },
          { schema: null },
          { secrets: null },
          { language: 'yaml' },
        ].map((options) => tried(editor, options)),
        documents: [{ echoes: false }, { label: null }, { className: null }, { className: '' }].map((options) => tried(view, options)),
        changed: [
          tried(editor, { echoes: true }),
          tried(editor, { className: 'y' }),
          tried(editor, { fileName: 'other.yaml' }),
          tried(view, { label: 'Files' }),
          tried(built, { className: null }),
          tried(built, { echoes: false }),
          tried(built, { className: 'x', echoes: true }),
        ],
      };
    });
    expect(result).toEqual({
      editor: Array(10).fill('ok'),
      documents: Array(4).fill('ok'),
      changed: [
        'echoes cannot change on a mounted ConfigEditor',
        'className cannot change on a mounted ConfigEditor',
        'fileName cannot change on a mounted ConfigEditor',
        'label cannot change on a mounted ConfigDocuments',
        'className cannot change on a mounted ConfigEditor',
        'echoes cannot change on a mounted ConfigEditor',
        'ok',
      ],
    });
    expect(page.errors).toEqual([]);
  });
});
