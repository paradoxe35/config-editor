import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Languages registered by the host: read-only fields without spans, a language that cannot write
// documents, languages by file name, Ctrl+/, callbacks that throw or return nothing, token kinds, bad input.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const input = (page) => page.locator('#qa .config-editor__input');

const value = (page) => input(page).inputValue();

async function caretAt(page, marker, offset = 0, length = 0) {
  const text = await value(page);
  const at = text.indexOf(marker);
  expect(at, `${JSON.stringify(marker)} in the text`).toBeGreaterThanOrEqual(0);
  await input(page).evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [at + offset, at + offset + length],
  );
}

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  // The env languages: `KEY=value` lines, numbers read as numbers. Flat
  // (separators) and bare (nothing but parse and serialize), neither with
  // spans; and one with `parse` alone.
  await page.evaluate(async () => {
    const module = await import('../config-editor/index.js');
    window.module = module;
    const parse = (text) => {
      const value = {};
      text.split('\n').forEach((line, index) => {
        if (!line.trim() || line.trim().startsWith('#')) return;
        const at = line.indexOf('=');
        if (at < 0) throw { message: 'expected KEY=value', line: index + 1, column: 1 };
        const raw = line.slice(at + 1).trim();
        value[line.slice(0, at).trim()] = /^-?\d+$/.test(raw) ? Number(raw) : raw;
      });
      return value;
    };
    const serialize = (value) =>
      Object.entries(value)
        .map(([key, item]) => `${key}=${item}`)
        .join('\n') + '\n';
    window.envParse = parse;
    module.registerLanguage({ label: 'EnvFlat', extensions: ['qaenv'], parse, serialize, separators: ['='], comments: { lineComment: '#' } });
    module.registerLanguage({ label: 'EnvBare', parse, serialize });
    module.registerLanguage({ label: 'EnvReadOnly', extensions: ['envro'], parse });
  });
}

/** A new editor in `#qa`, recording what reaches the host. */
async function mount(page, options) {
  await page.evaluate((options) => {
    window.editor?.destroy();
    window.qa?.destroy();
    let host = document.getElementById('qa');
    if (!host) {
      host = document.createElement('div');
      host.id = 'qa';
      document.body.prepend(host);
    }
    window.sent = [];
    window.qa = new window.module.ConfigEditor(host, {
      ...options,
      onChange: (text) => window.sent.push(text),
      onSave: (text) => window.sent.push(text),
    });
  }, options);
  await input(page).waitFor();
}

const SCHEMA = JSON.stringify({
  type: 'object',
  properties: { ID: { type: 'string', readOnly: true }, PORT: { type: 'integer' }, NAME: { type: 'string' } },
});

const TEXT = '# c\nID=srv-1\nNAME=a\nPORT=80\n';

// Each edit QA made on the locked `ID` (schema) and `PORT` (host path).
const LOCKED_EDITS = {
  'type in a locked value': async (page) => {
    await caretAt(page, 'srv-1', 3);
    await page.keyboard.type('9');
  },
  'type in a host-locked value': async (page) => {
    await caretAt(page, 'PORT=80', 6);
    await page.keyboard.type('9');
  },
  'delete a locked line': async (page) => {
    await caretAt(page, 'ID=srv-1\n', 0, 9);
    await page.keyboard.press('Delete');
  },
  'rename a locked key': async (page) => {
    await caretAt(page, 'ID=', 0, 2);
    await page.keyboard.type('XD');
  },
  'a duplicate overriding it': async (page) => {
    await caretAt(page, 'PORT=80\n', 8);
    await page.keyboard.insertText('ID=evil\n');
  },
  'replace everything': async (page) => {
    await caretAt(page, '# c', 0);
    await page.keyboard.press('ControlOrMeta+a');
    await page.keyboard.insertText('ID=srv-2\nPORT=81\n');
  },
};

// QA round 20 (host): registered languages. Read-only fields hold in a
// language that gives no spans (found on a flat language's lines, else
// checked by value); a language that cannot write documents is never
// edited as another, either way; a document's language comes from its
// file name, registered extensions included; Ctrl+/ takes a registered
// language's string over lines whole; and invalid input is refused with
// the reason.
test.describe('read-only fields in a registered language without spans', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const language of ['envflat', 'envbare']) {
    for (const [name, edit] of Object.entries(LOCKED_EDITS)) {
      test(`${language}: ${name} is refused`, async ({ page }) => {
        await open(page);
        await mount(page, { language, value: TEXT, schema: SCHEMA, readOnlyPaths: ['/PORT'] });
        await edit(page);
        await expect.poll(() => value(page)).toBe(TEXT);
        const changed = await page.evaluate(() => {
          window.qa.save();
          return [...window.sent, window.qa.value].filter((text) => {
            try {
              const read = window.envParse(text);
              return read.ID !== 'srv-1' || read.PORT !== 80;
            } catch {
              return false;
            }
          });
        });
        expect(changed).toEqual([]);
      });
    }

    test(`${language}: edits leaving the locked values as they were go through`, async ({ page }) => {
      await open(page);
      await mount(page, { language, value: TEXT, schema: SCHEMA, readOnlyPaths: ['/PORT'] });
      await caretAt(page, 'NAME=a', 6);
      await page.keyboard.type('bc');
      await caretAt(page, '# c', 3);
      await page.keyboard.type('omment');
      await expect.poll(() => value(page)).toBe('# comment\nID=srv-1\nNAME=abc\nPORT=80\n');
    });
  }

  test('a flat language without spans shows the lock in the gutter', async ({ page }) => {
    await open(page);
    await mount(page, { language: 'envflat', value: TEXT, schema: SCHEMA });
    await expect(page.locator('#qa .config-editor__lock-marker')).toHaveCount(1);
  });
});

test.describe('a language that cannot write documents', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('is never edited as another language, nor another as it', async ({ page }) => {
    await open(page);
    await mount(page, { language: 'envreadonly', value: 'A=1\n' });
    const messages = await page.evaluate(() => {
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return {
        fromUpdate: attempt(() => window.qa.update({ editAs: 'yaml' })),
        fromOption: attempt(() => new window.module.ConfigEditor(document.createElement('div'), { language: 'envreadonly', editAs: 'json' })),
        intoUpdate: attempt(() => new window.module.ConfigEditor(document.createElement('div'), { language: 'yaml' }).update({ editAs: 'envreadonly' })),
        ownLanguage: attempt(() => window.qa.update({ editAs: 'envreadonly' })),
      };
    });
    expect(messages.fromUpdate).toBe('editAs: EnvReadOnly cannot write documents');
    expect(messages.fromOption).toBe('editAs: EnvReadOnly cannot write documents');
    expect(messages.intoUpdate).toBe('editAs: EnvReadOnly cannot write documents');
    expect(messages.ownLanguage).toBe('no error');
    await expect(input(page)).toHaveValue('A=1\n');
    await expect(page.locator('#qa .config-editor__edit-as-notice')).toBeHidden();
  });
});

test.describe('a document opens in the language its file name says', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('registered extensions included; an unclaimed one warns and opens in the default', async ({ page }) => {
    await open(page);
    const warnings = [];
    page.on('console', (message) => {
      if (message.type() === 'warning') warnings.push(message.text());
    });
    const languages = await page.evaluate(() => {
      document.getElementById('editor').replaceChildren();
      const saved = {};
      const view = new window.module.ConfigDocuments(document.getElementById('editor'), {
        documents: [
          { id: 'env', fileName: 'app.qaenv', value: 'A=1\n' },
          { id: 'odd', fileName: 'notes.xyz', value: 'a: 1\n' },
          { id: 'toml', fileName: 'conf.d/limits.toml', value: 'a = 1\n' },
        ],
        onSave: (id, _text, language) => {
          saved[id] = language;
        },
      });
      for (const id of ['env', 'odd', 'toml']) view.editor(id).save();
      return saved;
    });
    expect(languages).toEqual({ env: 'envflat', odd: 'yaml', toml: 'toml' });
    expect(warnings.filter((text) => text.includes('notes.xyz'))).toHaveLength(1);
    expect(warnings.filter((text) => text.includes('app.qaenv'))).toHaveLength(0);
  });
});

test.describe("Ctrl+/ in a registered language's string over lines", () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('comments the whole string, never a line inside it', async ({ page }) => {
    await open(page);
    await page.evaluate(() => {
      window.module.registerLanguage({
        label: 'Tiny',
        parse: () => ({}),
        serialize: () => '',
        comments: { lineComment: '#' },
        separators: ['='],
        multilineRanges: (text) => [...text.matchAll(/'''[\s\S]*?(?:'''|$)/g)].map((match) => ({ start: match.index, end: match.index + match[0].length, kind: 'string' })),
      });
    });
    const text = "a = 1\nname = '''multi\nhost = x\nend'''\nb = 2\n";
    for (const marker of ['host = x', "end'''", "name = '''"]) {
      await mount(page, { language: 'tiny', value: text });
      await caretAt(page, marker, 2);
      await page.keyboard.press('ControlOrMeta+/');
      await expect.poll(() => value(page), marker).toBe("a = 1\n# name = '''multi\n# host = x\n# end'''\nb = 2\n");
    }
  });
});

test.describe('invalid input is refused with the reason', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('pointer escapes other than ~0 and ~1, and an empty source', async ({ page }) => {
    await open(page);
    await mount(page, { language: 'yaml', value: 'a: 1\n' });
    const messages = await page.evaluate(() => {
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const diagnostic = (extra) => window.qa.update({ hostDiagnostics: [{ severity: 'error', message: 'x', ...extra }] });
      return {
        readOnlyPath: attempt(() => window.qa.update({ readOnlyPaths: ['/a~2'] })),
        trailingTilde: attempt(() => window.qa.update({ readOnlyPaths: ['/a~'] })),
        diagnosticPath: attempt(() => diagnostic({ path: '/a~2' })),
        emptySource: attempt(() => diagnostic({ path: '/a', source: '' })),
        escapes: attempt(() => window.qa.update({ readOnlyPaths: ['/a~0b~1c'] })),
        source: attempt(() => diagnostic({ path: '/a', source: 'server' })),
      };
    });
    expect(messages.readOnlyPath).toContain('readOnlyPaths[0]: a JSON pointer escapes only ~0 (~) and ~1 (/)');
    expect(messages.trailingTilde).toContain('escapes only ~0 (~) and ~1 (/)');
    expect(messages.diagnosticPath).toContain('escapes only ~0 (~) and ~1 (/)');
    expect(messages.emptySource).toContain('source must name who reported the problem');
    expect(messages.escapes).toBe('no error');
    expect(messages.source).toBe('no error');
  });

  test('a key insertion whose caret splits a character is warned and left out', async ({ page }) => {
    await open(page);
    const warnings = [];
    page.on('console', (message) => {
      if (message.type() === 'warning') warnings.push(message.text());
    });
    await page.evaluate(() => {
      window.module.registerLanguage({
        label: 'Emoji',
        parse: () => ({}),
        separators: ['='],
        insertKey: (key) => ({ text: `${key}=😀`, caret: key.length + 2 }),
      });
    });
    await mount(page, { language: 'emoji', value: '', schema: SCHEMA });
    await input(page).focus();
    await page.keyboard.type('NA');
    await page.keyboard.press('Control+Space');
    await page.keyboard.press('Enter');
    await expect.poll(() => warnings.some((text) => text.includes("insertKey() must return") && text.includes('between characters'))).toBe(true);
    const text = await value(page);
    expect([...text].every((character) => character.codePointAt(0) < 0xd800 || character.codePointAt(0) > 0xdfff)).toBe(true);
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
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.module = await import('../config-editor/index.js');
      window.tools = await import('../config-editor/tools.js');
      const data = [
        {
          web: { hosts: ['a', 'b', { x: 1, 'we/ird~k': 2 }], listen: 80, 'k é😀': 'v😀' },
          list: [{ n: 1 }, { n: 2, m: [1, 2] }],
          empty: {},
          s: 'multi\nline',
        },
        { a: { b: { c: { d: true } } }, z: 'x' },
      ];
      const pointers = (value, at = '') => {
        const out = [at];
        if (value && typeof value === 'object') {
          for (const [key, item] of Object.entries(value)) {
            out.push(...pointers(item, `${at}/${key.replace(/~/g, '~0').replace(/\//g, '~1')}`));
          }
        }
        return out;
      };
      // Each language's documents (those it can write: XML has no `/` in a
      // name), LF-ended, and every path of each.
      window.documents = (language) =>
        data.flatMap((value) => {
          const document = language === 'xml' ? { root: value } : value;
          let text;
          try {
            text = window.tools.serialize(language, document);
          } catch {
            return [];
          }
          const paths = pointers(window.tools.parse(language, text));
          return [{ text, paths: [...paths, ...paths.map((path) => `${path}/nope`)] }];
        });
    });
  }


  test.describe('host API details', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('a throwing custom-language callback is logged once', async ({ page }) => {
      await open(page);
      await page.evaluate(async () => {
        const parse = (text) =>
          Object.fromEntries(
            text
              .split('\n')
              .filter((line) => line.includes('='))
              .map((line) => line.split('=')),
          );
        window.module.registerLanguage({
          label: 'Thrower22',
          separators: ['='],
          parse,
          highlight() {
            throw new Error('highlight boom');
          },
          multilineRanges() {
            throw new Error('multiline boom');
          },
          keySpan() {
            throw new Error('keySpan boom');
          },
        });
        const host = document.createElement('div');
        host.id = 'qa';
        document.body.prepend(host);
        window.qa = new window.module.ConfigEditor(host, {
          language: 'thrower22',
          value: 'A=1\nB=2\n',
          schema: JSON.stringify({ properties: { A: { type: 'integer' } } }),
        });
        for (let round = 0; round < 4; round += 1) {
          window.qa.update({ value: `A=${round}\nB=${round}\n` });
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
      });
      for (const callback of ['highlight', 'multilineRanges', 'keySpan']) {
        const logged = page.errors.filter((text) => text.includes(`Thrower22's ${callback}() threw`));
        expect(logged, callback).toHaveLength(1);
      }
    });
    test('validate reports a custom parse() that returns nothing', async ({ page }) => {
      await open(page);
      const problems = await page.evaluate(() => {
        window.module.registerLanguage({ label: 'Nothing22', parse: () => undefined });
        return window.tools.validate('nothing22', 'x').map((problem) => problem.message);
      });
      expect(problems).toEqual(['parse() returned nothing']);
    });
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


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('a custom language span callback returning null or undefined falls back to the flat default', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(() => {
        const parse = (text) =>
          Object.fromEntries(
            text
              .split('\n')
              .filter((line) => line.includes('='))
              .map((line) => line.split('=')),
          );
        const out = {};
        for (const [label, given] of [
          ['Stub26u', () => undefined],
          ['Stub26n', () => null],
          ['Stub26e', () => {}],
        ]) {
          window.module.registerLanguage({ label, separators: ['='], parse, keySpan: given, valueSpan: given });
          const name = label.toLowerCase();
          const text = '# c\nlisten=x\nname=web\n';
          const range = window.tools.rangeOf(name, text, '/name');
          const problems = window.tools.validate(name, text, JSON.stringify({ properties: { listen: { type: 'integer' } } }));
          out[label] = { range: range && text.slice(range.start, range.end), line: problems[0]?.line };
        }
        // A span given is used as it is.
        window.module.registerLanguage({
          label: 'Given26',
          separators: ['='],
          parse,
          keySpan: () => ({ line: 1, startColumn: 1, endColumn: 2 }),
        });
        const given = window.tools.rangeOf('given26', 'a=1\nb=2\n', '/b');
        out.given = given && [given.startLine, given.startColumn];
        return out;
      });
      for (const label of ['Stub26u', 'Stub26n', 'Stub26e']) {
        expect(result[label], label).toEqual({ range: 'name=web', line: 2 });
      }
      expect(result.given).toEqual([1, 1]);
    });
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


  // --- L29-4: a custom language's unknown token kind. --------------------------

  test.describe('custom language token kinds', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('an unknown kind from highlight is warned about once, and read as a value', async ({ page }) => {
      await open(page);
      const classes = await page.evaluate(async () => {
        const name = window.module.registerLanguage({
          label: 'Kinds',
          extensions: ['kinds'],
          parse: (text) => Object.fromEntries(text.split('\n').filter(Boolean).map((line) => line.split('='))),
          serialize: (value) => Object.entries(value).map(([key, item]) => `${key}=${item}\n`).join(''),
          highlight: (line) => [{ start: 0, end: line.length, kind: line.startsWith('A') ? 'nonsense' : 'value' }],
        });
        const editor = new window.module.ConfigEditor(window.mountPoint(), { language: name, value: 'A=1\nA=2\nB=3\n' });
      await window.pause(300);
        editor.revealRange({ start: 3, end: 3 });
        await window.pause(100);
        return [...document.querySelectorAll('[data-host] .config-editor__highlight span')]
        .filter((span) => span.children.length === 0 && span.textContent.startsWith('A'))
          .map((span) => span.className);
      });
      await page.locator('[data-host] .config-editor__input').focus();
    await page.keyboard.type('x');
      await page.waitForTimeout(200);
      const warned = page.warnings.filter((text) => text.includes("Kinds's highlight()"));
    expect(warned).toEqual([
        'config-editor: Kinds\'s highlight() gave a token of kind "nonsense", not one of "key", "string", "value", "literal", "comment", "punctuation", "reference" or "interpolation"; read as "value"',
    ]);
    expect(classes.length).toBeGreaterThan(0);
      expect(new Set(classes)).toEqual(new Set(['config-editor__value']));
    expect(page.errors).toEqual([]);
    });
  });
});
