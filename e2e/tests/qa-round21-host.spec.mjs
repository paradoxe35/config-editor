import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 21 (host API, locks, headless). A read-only field in a language
// without spans holds through broken text, and a save waits for a text that
// parses; a refusal takes back only what it is about; a host update while
// editing as another language sends no onChange; YAML tags are valid YAML
// and Edit as keeps them; several YAML documents say so in plain words;
// every position counts LF, CRLF and lone CR alike; infinities, NaN and
// 64-bit integers come through `parse` exactly; and the small host LOWs.

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
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    const parse = (text) => {
      const value = {};
      text.split('\n').forEach((line, index) => {
        if (!line.trim() || line.trim().startsWith('#')) return;
        const at = line.indexOf('=');
        if (at < 0) throw { message: 'expected KEY=value', line: index + 1, column: 1 };
        value[line.slice(0, at).trim()] = line.slice(at + 1).trim();
      });
      return value;
    };
    window.envParse = parse;
    const serialize = (value) =>
      Object.entries(value)
        .map(([key, item]) => `${key}=${item}`)
        .join('\n') + '\n';
    window.module.registerLanguage({ label: 'Bare21', parse, serialize });
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
    window.saved = [];
    window.qa = new window.module.ConfigEditor(host, {
      showFooter: true,
      ...options,
      onChange: (text) => window.sent.push(text),
      onSave: (text) => window.saved.push(text),
    });
  }, options);
  await input(page).waitFor();
}

const TEXT = '# c\nid=abc\nlisten=7835\nname=x\n';

test.describe('a read-only field in a language without spans, through broken text', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  const cases = {
    'break another line, then edit the locked value': async (page) => {
      await caretAt(page, 'name=x', 4, 1);
      await page.keyboard.press('Delete');
      await caretAt(page, 'listen=7835', 11);
      await page.keyboard.type('99');
    },
    'replace the locked line so the text no longer parses': async (page) => {
      await caretAt(page, 'listen=7835', 0, 11);
      await page.keyboard.type('z');
    },
    'select everything and type': async (page) => {
      await caretAt(page, '# c', 0);
      await page.keyboard.press('ControlOrMeta+a');
      await page.keyboard.type('x');
    },
  };
  for (const [name, edit] of Object.entries(cases)) {
    test(`${name}: the value holds and nothing sent carries another`, async ({ page }) => {
      await open(page);
      await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
      await edit(page);
      await expect.poll(() => value(page)).toContain('listen=7835\n');
      const carried = await page.evaluate(() => {
        window.qa.save();
        return [...window.sent, ...window.saved].filter((text) => !text.includes('listen=7835\n'));
      });
      expect(carried).toEqual([]);
    });
  }

  test('a save waits while the text does not parse, saying which field', async ({ page }) => {
    await open(page);
    await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
    await caretAt(page, 'name=x', 4, 1);
    await page.keyboard.press('Delete');
    expect(await page.evaluate(() => window.qa.save())).toBe(false);
    await expect(page.locator('#qa .config-editor__protect-tip')).toContainText('/listen');
    expect(await page.evaluate(() => window.saved)).toEqual([]);
    await caretAt(page, 'namex', 4);
    await page.keyboard.type('=');
    expect(await page.evaluate(() => window.qa.save())).toBe(true);
  });

  test('a refusal takes back only what it refuses', async ({ page }) => {
    // Typed on a free line, the `=` that would join it to nothing locked
    // goes through; on the locked line, the first key is refused and the
    // user's own text on other lines stays.
    await open(page);
    await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
    await caretAt(page, 'name=x', 6);
    await page.keyboard.type('abc');
    await caretAt(page, 'listen=7835', 0, 11);
    await page.keyboard.type('z');
    await expect.poll(() => value(page)).toBe('# c\nid=abc\nlisten=7835\nname=xabc\n');
  });
});

test.describe('a host update while editing as another language', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const editAs of ['json', 'toml', 'hcl', 'xml']) {
    test(`${editAs}: an update in the constructor's task sends no onChange`, async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async (editAs) => {
        const changes = [];
        const host = document.createElement('div');
        document.body.append(host);
        const editor = new window.module.ConfigEditor(host, { value: '# c\na: 1\n', editAs, onChange: (text) => changes.push(text) });
        editor.update({ value: '# c2\na: 5\nb: x\n' });
        await new Promise((resolve) => setTimeout(resolve, 700));
        const out = { changes, value: editor.value };
        editor.destroy();
        return out;
      }, editAs);
      expect(result).toEqual({ changes: [], value: '# c2\na: 5\nb: x\n' });
    });
  }
});

const TAGGED = "# stack\nres: !GetAtt [r, Arn]\nname: !Sub 'x-${y}'\nport: 1\ninc: !include\n  k: v\nkey: !secret pw\nrefs: !reference [.base, script]\n";

test.describe('YAML tags and several documents', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('tags are valid YAML, read as the values under them, kept through Edit as', async ({ page }) => {
    await open(page);
    const headless = await page.evaluate((text) => ({ problems: window.tools.validate('yaml', text), data: window.tools.parse('yaml', text) }), TAGGED);
    expect(headless.problems).toEqual([]);
    expect(headless.data).toEqual({ res: ['r', 'Arn'], name: 'x-${y}', port: 1, inc: { k: 'v' }, key: 'pw', refs: ['.base', 'script'] });
    await mount(page, { language: 'yaml', value: TAGGED, editAs: 'json' });
    await expect.poll(() => value(page)).toContain('"port": 1');
    await caretAt(page, '"port": 1', 8, 1);
    await page.keyboard.type('2');
    expect(await page.evaluate(() => window.qa.save())).toBe(true);
    expect(await page.evaluate(() => window.saved)).toEqual([TAGGED.replace('port: 1', 'port: 2')]);
  });

  test('several documents: a plain message, and Edit as refused with it', async ({ page }) => {
    await open(page);
    const problems = await page.evaluate(() => window.tools.validate('yaml', 'a: 1\n---\nb: 2\n'));
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ line: 2, column: 1 });
    expect(problems[0].message).toContain('more than one YAML document');
    expect(problems[0].message).not.toMatch(/deserializ|serde/);
    await mount(page, { language: 'yaml', value: 'a: 1\n---\nb: 2\n', editAs: 'json' });
    const messages = () =>
      page.locator('#qa .config-editor__diagnostic-marker').evaluateAll((markers) => markers.map((marker) => marker.dataset.message));
    await expect.poll(messages).toContainEqual(expect.stringMatching(/^Cannot switch to JSON yet: .*more than one YAML document/));
    await expect(input(page)).toHaveValue('a: 1\n---\nb: 2\n');
  });
});

test.describe('headless answers', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('positions count LF, CRLF and a lone CR alike', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const t = window.tools;
      const samples = {
        yaml: [['a: 1', 'b: 2', 'n: 1', ''], '/n', '{"properties":{"n":{"type":"string"}}}'],
        json: [['{', '"a": 1,', '"n": 1', '}'], '/n', '{"properties":{"n":{"type":"string"}}}'],
        xml: [['<r>', '<a>1</a>', '<n>x</n>', '</r>'], '/r/n', '{"properties":{"r":{"properties":{"n":{"type":"integer"}}}}}'],
      };
      const out = {};
      for (const [language, [lines, pointer, schema]] of Object.entries(samples)) {
        for (const [name, ending] of [['lf', '\n'], ['crlf', '\r\n'], ['cr', '\r']]) {
          const text = lines.join(ending);
          const range = t.rangeOf(language, text, pointer);
          out[`${language} ${name}`] = {
            problems: t.validate(language, text, schema).map(({ line, column }) => [line, column]),
            range: range && [range.startLine, range.startColumn, range.endLine, range.endColumn, text.slice(range.start, range.end)],
            path: range && t.pathAt(language, text, range.start)?.pointer,
          };
        }
      }
      return out;
    });
    for (const language of ['yaml', 'json', 'xml']) {
      const lf = result[`${language} lf`];
      expect(lf.range, language).not.toBeNull();
      for (const ending of ['crlf', 'cr']) {
        expect(result[`${language} ${ending}`], `${language} ${ending}`).toEqual(lf);
      }
    }
  });

  test('infinities, NaN and 64-bit integers come through parse and back exactly', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const t = window.tools;
      const yaml = t.parse('yaml', 'd: .inf\ne: -.inf\nf: .nan\nbig: 9007199254740993\n');
      const toml = t.parse('toml', 'c = inf\nd = -inf\ne = nan\nbig = -9223372036854775808\n');
      return {
        yaml: [yaml.d, yaml.e, Number.isNaN(yaml.f), typeof yaml.big, String(yaml.big)],
        toml: [toml.c, toml.d, Number.isNaN(toml.e), typeof toml.big, String(toml.big)],
        yamlBack: t.serialize('yaml', yaml),
        tomlBack: t.serialize('toml', toml),
        small: typeof t.parse('json', '{"a": 9007199254740991}').a,
      };
    });
    expect(result.yaml).toEqual([Infinity, -Infinity, true, 'bigint', '9007199254740993']);
    expect(result.toml).toEqual([Infinity, -Infinity, true, 'bigint', '-9223372036854775808']);
    expect(result.yamlBack).toBe('d: .inf\ne: -.inf\nf: .nan\nbig: 9007199254740993\n');
    expect(result.tomlBack).toBe('c = inf\nd = -inf\ne = nan\nbig = -9223372036854775808\n');
    expect(result.small).toBe('number');
  });
});

test.describe('host LOWs', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('the API before loadConfigEditor() says to await it', async ({ page }) => {
    await open(page);
    const messages = await page.evaluate(async () => {
      // A fresh copy of the module, never loaded.
      const fresh = await import('../config-editor/pkg/config_editor.js?unloaded');
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message);
        }
      };
      return [
        attempt(() => new fresh.ConfigEditor(document.createElement('div'), {})),
        attempt(() => fresh.parse('yaml', 'a: 1\n')),
        attempt(() => fresh.languages()),
      ];
    });
    for (const message of messages) expect(message).toContain('await loadConfigEditor()');
  });

  test('pointers, language, strings, ranges and documents', async ({ page }) => {
    await open(page);
    const warnings = [];
    page.on('console', (message) => {
      if (message.type() === 'warning') warnings.push(message.text());
    });
    await mount(page, { language: 'yaml', value: '"": empty\na: 1\n' });
    const result = await page.evaluate(async () => {
      const attempt = (run) => {
        try {
          return run();
        } catch (error) {
          return `throws: ${error.message}`;
        }
      };
      const out = {
        toolsSlash: window.tools.rangeOf('yaml', '"": empty\na: 1\n', '/'),
        editorSlash: window.qa.rangeOf('/'),
        sameLanguage: attempt(() => window.qa.update({ language: 'yaml', readOnly: true }) ?? 'ok'),
        otherLanguage: attempt(() => window.qa.update({ language: 'json' })),
        pastLine: attempt(() => window.qa.update({ hostDiagnostics: [{ message: 'm', range: { startLine: 2, startColumn: 50 } }], readOnlyRanges: [{ startLine: 1, startColumn: 1, endLine: 1, endColumn: 99 }] }) ?? 'ok'),
      };
      window.qa.update({ readOnly: false, strings: { errorsOne: '{cnt} err' } });
      await new Promise((resolve) => setTimeout(resolve, 100));
      window.qa.update({ readOnlyPaths: ['/'] });
      return out;
    });
    expect(result.toolsSlash).toMatchObject({ startLine: 1, endLine: 1 });
    expect(result.editorSlash).toMatchObject({ startLine: 1, endLine: 1 });
    expect(result.sameLanguage).toBe('ok');
    expect(result.otherLanguage).toContain('language cannot change on a mounted ConfigEditor');
    expect(result.pastLine).toBe('ok');
    expect(warnings.filter((text) => text.includes('column 50 is past the end of line 2'))).toHaveLength(1);
    expect(warnings.filter((text) => text.includes('unknown placeholder {cnt}'))).toHaveLength(1);
    // `/` locks the key "" alone: the rest is free.
    await caretAt(page, 'a: 1', 3, 1);
    await page.keyboard.type('2');
    await expect.poll(() => value(page)).toBe('"": empty\na: 2\n');
    await caretAt(page, 'empty', 0, 5);
    await page.keyboard.type('x');
    await expect.poll(() => value(page)).toBe('"": empty\na: 2\n');

    warnings.length = 0;
    await page.evaluate(() => {
      window.qa.destroy();
      const view = new window.module.ConfigDocuments(document.getElementById('qa'), {
        documents: [
          { id: 'x', fileName: 'a.yaml', value: 'a: 1\n' },
          { id: 'x', fileName: 'b.yaml', value: 'b: 1\n' },
          { id: 'd', fileName: 'Dockerfile', value: 'FROM x\n' },
        ],
      });
      view.update({ documents: [{ id: 'x', fileName: 'a.yaml', value: 'a: 1\n' }, { id: 'x', fileName: 'b.yaml', value: 'b: 1\n' }] });
      view.destroy();
    });
    expect(warnings.filter((text) => text.includes('"x" is listed twice'))).toHaveLength(1);
    const docker = warnings.filter((text) => text.includes('Dockerfile'));
    expect(docker).toHaveLength(1);
    expect(docker[0]).toContain('the file name "Dockerfile" (it has no extension)');
    expect(docker[0]).not.toContain('extension of');
  });

  test('vanilla example: the Edit-as select shows the language on screen', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    // TOML has no null: the switch waits, the YAML stays on screen.
    await page.evaluate(() => window.editor.update({ value: 'a: null\n' }));
    await page.selectOption('#edit-as', 'toml');
    await expect(page.locator('#edit-as')).toHaveValue('');
    await page.selectOption('#edit-as', 'json');
    await expect(page.locator('#edit-as')).toHaveValue('json');
  });
});
