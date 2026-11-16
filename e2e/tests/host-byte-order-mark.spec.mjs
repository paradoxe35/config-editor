import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// A leading byte order mark: kept through edits and saves, never a column for tools, ranges and locks.

const saved = (page) => page.locator('.demo__output-text').textContent();

const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Puts the caret just after `needle`.
async function caretAfter(e, needle) {
  await e.input.evaluate((element, find) => {
    const start = element.value.indexOf(find);
    if (start < 0) throw new Error(`no ${find}`);
    element.focus();
    element.setSelectionRange(start + find.length, start + find.length);
  }, needle);
}

const BOM = '\u{FEFF}';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

/** Mounts a fresh ConfigEditor with `options` on the vanilla page. */
async function open(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const parent = document.querySelector('.config-editor').parentElement;
    window.editor.destroy();
    window.saved = null;
    window.problems = [];
    window.editor = new ConfigEditor(parent, {
      ...options,
      onSave: (text) => (window.saved = text),
      onProblems: (problems) => (window.problems = problems),
    });
  }, options);
  await page.locator('.config-editor__input').waitFor();
}

const input = (page) => page.locator('.config-editor__input');

const host = (page) => page.evaluate(() => window.editor.value);

/** Puts the caret at the start of the `nth` match of `needle` on screen. */
async function caretAt(page, needle, nth = 0) {
  await input(page).evaluate((element, [needle, nth]) => {
    let at = -1;
    for (let index = 0; index <= nth; index++) at = element.value.indexOf(needle, at + 1);
    if (at < 0) throw new Error(`no ${needle}`);
    element.focus();
    element.setSelectionRange(at, at);
  }, [needle, nth]);
}

async function press(page, ...keys) {
  for (const key of keys) await page.keyboard.press(key);
}

async function editAs(page, language) {
  await page.evaluate((language) => window.editor.update({ editAs: language }), language);
  await page.waitForTimeout(200);
}

/** The host's value once the typing pause is over. */
async function settled(page) {
  await page.waitForTimeout(700);
  return host(page);
}

for (const [language, host, edited] of [
  ['yaml', `${BOM}a: 1 # c\nb: 2\n`, `${BOM}a: 10 # c\nb: 2\n`],
  ['toml', `${BOM}a = 1 # c\nb = 2\n`, `${BOM}a = 10 # c\nb = 2\n`],
]) {
  test.describe(`a ${language} host with a byte order mark`, () => {
    setupEditor(test, `?language=${language}`);

    test('keeps the mark and the comment when its first key is edited as JSON', async ({ page }) => {
      const e = editor(page);
      await e.paste(host);
      await expect.poll(() => saved(page)).toBe(host);
      await pick(page, 'json');
      await expect.poll(() => e.value()).toContain('"a": 1');
      await caretAfter(e, '"a": 1');
      await page.keyboard.type('0');
      await expect.poll(() => saved(page)).toBe(edited);
    });
  });
}

test.describe('editing', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  const marked = {
    yaml: '# head\nname: a\nport: 1\n',
    json: '{\n  "name": "a",\n  "port": 1\n}\n',
    toml: '# head\nname = "a"\nport = 1\n',
    hcl: '# head\nname = "a"\nport = 1\n',
    xml: '<?xml version="1.0"?>\n<c>\n  <name>a</name>\n</c>\n',
  };
  const firstLineEdits = {
    'Shift+Down Delete': (page) => press(page, 'Shift+ArrowDown', 'Delete'),
    'Ctrl+X': (page) => press(page, 'Control+x'),
    'Ctrl+A and a paste': async (page) => {
      await press(page, 'Control+a');
      await page.keyboard.insertText('x: 1\n');
    },
    'Ctrl+Shift+K': (page) => press(page, 'Control+Shift+K'),
    'typing at the start': (page) => page.keyboard.type('x'),
    'Ctrl+A Tab': (page) => press(page, 'Control+a', 'Tab'),
    'Tab on the line': (page) => press(page, 'Shift+End', 'Tab'),
    'Ctrl+]': (page) => press(page, 'Control+BracketRight'),
    'Alt+Down': (page) => press(page, 'Alt+ArrowDown'),
    'Shift+Alt+Up': (page) => press(page, 'Shift+Alt+ArrowUp'),
  };
  for (const [language, text] of Object.entries(marked)) {
    for (const crlf of [false, true]) {
      test(`a ${language} host's byte order mark outlasts every edit of its first line${crlf ? ' (CRLF)' : ''}`, async ({ page }) => {
        const value = BOM + (crlf ? text.replaceAll('\n', '\r\n') : text);
        const failures = [];
        for (const [name, edit] of Object.entries(firstLineEdits)) {
          await open(page, { language, value });
          expect(await input(page).inputValue()).not.toContain(BOM);
          await input(page).evaluate((element) => {
            element.focus();
            element.setSelectionRange(0, 0);
          });
          await edit(page);
          const saved = await settled(page);
          const marks = [...saved].filter((character) => character === BOM).length;
          if (!saved.startsWith(BOM) || marks !== 1) failures.push(`${name}: ${JSON.stringify(saved)}`);
        }
        expect(failures).toEqual([]);
      });
    }
  }
  test('a byte order mark outlasts a line moved over the first in another view', async ({ page }) => {
    await open(page, { language: 'yaml', value: `${BOM}name: a\nport: 1\n` });
    await editAs(page, 'toml');
    await caretAt(page, 'port');
    await press(page, 'Alt+ArrowUp');
    const saved = await settled(page);
    expect(saved).toBe(`${BOM}port: 1\nname: a\n`);
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

  // --- a byte order mark is a string index but no column, everywhere.

  // Documents with a byte order mark, one schema problem on line 1 each.
  const SCHEMA = JSON.stringify({ type: 'object', properties: { port: { type: 'integer' } } });

  const MARKED = {
    yaml: '﻿port: x\n',
    'yaml CRLF': '﻿port: x\r\na: 1\r\n',
    json: '﻿{"port": "x"}\n',
    toml: '﻿port = "x"\n',
    hcl: '﻿port = "x"\n',
  };


  test.describe('a byte order mark is no column', () => {
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
        document.querySelectorAll('[data-host]').forEach((element) => element.remove());
        const element = document.createElement('div');
        element.dataset.host = '';
        element.style.height = '400px';
        document.body.prepend(element);
        return element;
      };
    });
  }

  // --- a text with a byte order mark through every headless tool, in
  // every language, with astral, other non-ASCII and ASCII characters in each
  // kind of place (keys, values, element text, attributes, comments; the first
  // line too; documents that read and ones that do not): no tool crashes, and
  // each answers as for the text after the mark (lines and columns are the
  // document's, string indices the text's: one more). `schemaWarnings` reads
  // no document. ----------------------------------------------------------------

  const DOCUMENTS = {
    yaml: [
      'k@: v@ # c@\n',
      '# c@\nk@: v@\nlist:\n  - "q@"\n  - \'r@\'\nn: 1\nblock: |\n  t@\n',
      '{a@: b@, n: [1, "x@"]}\n',
      'k@: [v@\n',
    ],
    json: ['{"k@": "v@", "n": [1, "x@"]}', '{\n  "k@": "v@",\n  "o": {"a": "@"}\n}\n', '{"k@": }'],
    toml: ['"k@" = "v@" # c@\n', '# c@\nk = \'v@\'\n["t@"]\na = """\nx@"""\n[[l]]\nb = "y@"\n', 'k = "v@\n'],
    hcl: ['k = "v@" # c@\n', '// c@\nb "l@" {\n  a = "x@"\n  n = [1, "y@"]\n}\n', 'k = "v@\n'],
    xml: [
      '<a>@</a>\n',
      '<r k="v@"><e>t@</e><!-- c@ --></r>\n',
      '<?xml version="1.0"?>\n<r>\n  <a>@</a>\n  <b x="@">y@</b>\n  <!-- @ -->\n</r>\n',
      '<r><e@>t@</e@></r>\n',
      '<r><e>t@</x></r>\n',
    ],
  };

  const SCHEMA = JSON.stringify({
    type: 'object',
    properties: { n: { type: 'string' }, list: { type: 'array', items: { type: 'number' } } },
    additionalProperties: { type: 'number' },
  });


  test.describe('headless tools and a byte order mark', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('every tool reads a marked text as the text after its mark, in every language', async ({ page }) => {
      await open(page);
      const failures = await page.evaluate(
        ([documents, schema]) => {
          const T = window.tools;
          const failures = [];
          const answer = (run) => {
            try {
              return JSON.stringify(run(), (_, value) => (typeof value === 'bigint' ? `${value}n` : value));
            } catch (error) {
              return `${error instanceof WebAssembly.RuntimeError ? 'CRASH' : 'throws'} ${error.message}`;
            }
          };
          const pointers = (value, at, out) => {
            out.push(at);
            if (value && typeof value === 'object') {
              for (const [key, child] of Object.entries(value)) {
                pointers(child, `${at}/${key.replaceAll('~', '~0').replaceAll('/', '~1')}`, out);
            }
            }
            return out;
          };
          // A range of the plain text, as found in the marked one.
          const placed = (range) => range && { ...range, start: range.start + 1, end: range.end + 1 };
          for (const [language, list] of Object.entries(documents)) {
            for (const template of list) {
              for (const character of ['x', 'é', '😀', '😀é😀']) {
                const plain = template.replaceAll('@', character);
                const marked = `﻿${plain}`;
                const same = (tool, without, withMark) => {
                  if (without !== withMark || withMark.startsWith('CRASH')) {
                    failures.push(`${language} ${tool} ${JSON.stringify(plain)}: ${without} | ${withMark}`);
                  }
                };
                const both = (tool, run) => same(tool, answer(() => run(plain)), answer(() => run(marked)));
                both('validate', (text) => T.validate(language, text));
                both('validate with a schema', (text) => T.validate(language, text, schema));
                both('parse', (text) => T.parse(language, text));
                for (const target of Object.keys(documents)) {
                  both(`convert to ${target}`, (text) => T.convert(language, text, target));
                  both(`convert to ${target} with a schema`, (text) => T.convert(language, text, target, schema));
                }
                let all = [];
                try {
                  all = pointers(T.parse(language, plain), '', []);
                } catch {}
                same(
                  'rangeOf',
                  answer(() => all.map((path) => placed(T.rangeOf(language, plain, path)))),
                  answer(() => all.map((path) => T.rangeOf(language, marked, path))),
                );
                same(
                  'rangesOf',
                  answer(() => T.rangesOf(language, plain, all).map(placed)),
                  answer(() => T.rangesOf(language, marked, all)),
                );
                const indices = [...Array(plain.length + 1).keys()];
                same(
                  'pathAt',
                  answer(() => indices.map((index) => T.pathAt(language, plain, index))),
                  answer(() => indices.map((index) => T.pathAt(language, marked, index + 1))),
                );
              }
            }
          }
          return failures;
        },
        [DOCUMENTS, SCHEMA],
      );
      expect(failures).toEqual([]);
      expect(page.errors).toEqual([]);
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // A leading byte order mark is no part of a
  // document, shown or never shown. `save(id)`, `saveAll()` and
  // `editor(id).save()` of a never-shown document answer as they do once it
  // is shown, in every language, whatever characters it holds (a marked XML
  // document with an emoji once crashed the view; a marked JSON or HCL one
  // was refused), and the text sent keeps its mark. `saveAll()` sends every
  // other document past one it refuses.
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const DOCUMENTS = pathToFileURL(join(js, 'examples', 'documents.html')).href;

  const BOM = '﻿';

  // [name, file, text]: every one reads; with a mark in front it still does.
  const CASES = [
    ['YAML', 'a.yaml', 'a: 😀日本é\n'],
    ['YAML key', 'a.yaml', '😀日本é: 1\n'],
    ['JSON', 'a.json', '{"a": "😀日本é"}\n'],
    ['JSON key', 'a.json', '{"😀日本é": 1}\n'],
    ['TOML', 'a.toml', 'a = "😀日本é"\n'],
    ['HCL', 'a.tf', 'a = "😀日本é"\n'],
    ['XML element text', 'a.xml', '<r><a>😀</a></r>\n'],
    ['XML element name', 'a.xml', '<r><日本>1</日本></r>\n'],
    ['XML attribute', 'a.xml', '<r a="é"/>\n'],
    ['XML declaration', 'a.xml', '<?xml version="1.0" encoding="utf-8"?>\n<r><a>😀</a></r>\n'],
  ];

  /** Mounts `documents` in a fresh view, recording what `onSave` hears. */
  async function mount(page, documents) {
    await page.evaluate(async (documents) => {
      const CE = await import(new URL('../config-editor/index.js', location.href).href);
      window.documentsView?.destroy();
      document.getElementById('probe')?.remove();
      const host = document.createElement('div');
      host.id = 'probe';
      host.style.height = '300px';
      document.body.prepend(host);
      window.sent = [];
      window.documentsView = new CE.ConfigDocuments(host, {
        documents,
        onSave: (id, value) => window.sent.push([id, value]),
      });
    }, documents);
  }

  /** What `save('x')`, `saveAll()` and `editor('x').save()` answer and send. */
  async function saves(page, file, value, shown) {
    await mount(page, [
      { id: 'first', fileName: 'first.yaml', value: 'k: 1\n' },
      { id: 'x', fileName: file, value, baseline: 'changed\n' },
    ]);
    if (shown) {
      await page.evaluate(() => window.documentsView.select('x'));
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    }
    return page.evaluate(() => {
      const view = window.documentsView;
      const one = view.save('x');
      const all = view.saveAll();
      const editor = view.editor('x').save();
      return { one, all, editor, sent: window.sent };
    });
  }


  test.describe('a byte order mark on a document shown or never shown', () => {
    test.skip(!built, 'the npm package is not built: cd js && npm run build');
    test.beforeEach(async ({ page }) => {
      page.errors = [];
      page.on('pageerror', (error) => page.errors.push(String(error)));
      await page.goto(DOCUMENTS);
      await page.waitForFunction(() => document.body.dataset.ready === 'true');
    });
    for (const [name, file, text] of CASES) {
      test(`${name}: saved alike, the mark kept`, async ({ page }) => {
        for (const value of [text, BOM + text]) {
          const unshown = await saves(page, file, value, false);
          const shown = await saves(page, file, value, true);
          expect(unshown, `never shown ${JSON.stringify(value)}`).toEqual({
            one: true,
            all: ['x'],
            editor: true,
            sent: [
              ['x', value],
              ['x', value],
              ['x', value],
            ],
          });
          expect(shown, `shown ${JSON.stringify(value)}`).toEqual(unshown);
        }
        expect(page.errors).toEqual([]);
      });
    }
  });
});
