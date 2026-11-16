import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor } from './helpers.mjs';

// The headless tools: parse, serialize, validate, rangeOf, rangesOf and pathAt, line breaks, infinities,
// NaN and 64-bit integers.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const input = (page) => page.locator('#qa .config-editor__input');

const value = (page) => input(page).inputValue();

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

test.describe('infinities and NaN', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('stay numbers through parse, convert, serialize and validate', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const api = await import('../config-editor/pkg/config_editor.js');
      const caught = (run) => {
        try {
          run();
          return null;
        } catch (error) {
          return error.message;
        }
      };
      const schema = JSON.stringify({ properties: { rate: { type: 'number' }, odd: { type: 'number' } } });
      return {
        toml: api.parse('toml', 'rate = inf\nlow = -inf\nodd = nan\n'),
        yaml: api.parse('yaml', 'rate: .inf\n'),
        toYaml: api.convert('toml', 'rate = inf\nodd = nan\n', 'yaml'),
        toJson: api.convert('yaml', 'rate: .inf\n', 'json'),
        written: api.serialize('toml', { rate: Infinity, odd: NaN }),
        json: caught(() => api.serialize('json', { rate: Infinity })),
        valid: api.validate('toml', 'rate = inf\nodd = nan\n', schema),
      };
    });
    expect(result.toml.rate).toBe(Infinity);
    expect(result.toml.low).toBe(-Infinity);
    expect(Number.isNaN(result.toml.odd)).toBe(true);
    expect(result.yaml.rate).toBe(Infinity);
    expect(result.toYaml).toEqual({ text: 'rate: .inf\nodd: .nan\n', losses: [] });
    expect(result.toJson.losses).toEqual(['/rate']);
    expect(result.toJson.text).not.toContain('null');
    expect(result.written).toBe('rate = inf\nodd = nan\n');
    expect(result.json).toMatch(/JSON cannot write this value.*\/rate/);
    expect(result.valid).toEqual([]);
  });
});

test.describe('a registered language with a naive writer', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('writes infinities, NaN and huge integers from other languages as their text', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const t = window.tools;
      return {
        toml: t.convert('toml', 'rate = inf\nlow = -inf\nodd = nan\nbig = 9223372036854775807\n', 'Bare21'),
        json: t.convert('json', '{"big": 18446744073709551615, "s": "\\ufdd0inf"}', 'Bare21'),
      };
    });
    expect(result.toml.text).toBe('rate=inf\nlow=-inf\nodd=nan\nbig=9223372036854775807\n');
    expect(result.toml.losses).toEqual(expect.arrayContaining(['/rate', '/low', '/odd']));
    expect(result.json.text).toBe('big=18446744073709551615\ns=﷐inf\n');
  });
});

test.describe('host inputs and data', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('serialize writes only what reads back the same; bad inputs throw with their index', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').waitFor();
    const result = await page.evaluate(async () => {
      const api = await import('../config-editor/pkg/config_editor.js');
      const caught = (run) => {
        try {
          run();
          return null;
        } catch (error) {
          return error.message;
        }
      };
      const editor = window.editor;
      return {
        xmlOne: caught(() => api.serialize('xml', { a: [1] })),
        xmlEmpty: caught(() => api.serialize('xml', { a: [] })),
        date: caught(() => api.serialize('yaml', { when: new Date(0) })),
        map: caught(() => api.serialize('json', { m: new Map() })),
        big: api.serialize('json', { n: 1e16 }),
        noSlash: caught(() => api.rangeOf('yaml', 'web:\n  http: 80\n', 'web/http')),
        paths: caught(() => editor.update({ readOnlyPaths: ['/listen', 'nope'] })),
        backwards: caught(() => editor.update({ readOnlyRanges: [{ start: 5, end: 2 }] })),
        past: caught(() => editor.update({ readOnlyRanges: [{ startLine: 999, startColumn: 1 }] })),
        both: caught(() => editor.update({ hostDiagnostics: [{ path: '/listen', range: { start: 0, end: 1 }, message: 'x' }] })),
        docs: caught(() => editor.update({ hostDiagnostics: [{ path: '/listen', message: 'x', docsUrl: 'javascript:alert(1)' }] })),
        badPath: caught(() => editor.update({ hostDiagnostics: [{ message: 'ok' }, { path: 'listen', message: 'x' }] })),
      };
    });
    expect(result.xmlOne).toMatch(/XML cannot write this value.*\/a/);
    expect(result.xmlEmpty).toMatch(/XML cannot write this value.*\/a/);
    expect(result.date).toMatch(/Date, which is not plain data/);
    expect(result.map).toMatch(/Map, which is not plain data/);
    expect(result.big).toBe('{\n  "n": 10000000000000000\n}\n');
    expect(result.noSlash).toMatch(/starts with "\/"/);
    expect(result.paths).toMatch(/readOnlyPaths\[1\]: a JSON pointer starts with "\/"/);
    expect(result.backwards).toMatch(/readOnlyRanges\[0\]: end \(2\) is before start \(5\)/);
    expect(result.past).toMatch(/readOnlyRanges\[0\]: start line 999 is past/);
    expect(result.both).toMatch(/hostDiagnostics\[0\]: give a path or a range, not both/);
    expect(result.docs).toMatch(/hostDiagnostics\[0\]: docsUrl must be an http\(s\) URL/);
    expect(result.badPath).toMatch(/hostDiagnostics\[1\]: path: a JSON pointer starts with "\/"/);
  });
});

test.describe('a string spelled like the reserved infinity form', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  test('stays a string through parse, serialize and convert', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(async () => {
      const tools = await import('../config-editor/pkg/config_editor.js');
      const forged = '\ufdd0inf';
      const parsed = tools.parse('json', JSON.stringify({ s: forged, n: 1 }));
      const yaml = tools.serialize('yaml', { s: forged, rate: Infinity });
      const converted = tools.convert('json', JSON.stringify({ s: forged }), 'toml');
      return {
        parsed: parsed.s === forged,
        yaml,
        back: tools.parse('yaml', yaml),
        converted: converted.losses,
        toml: tools.parse('toml', converted.text).s === forged,
      };
    });
    expect(seen.parsed).toBe(true);
    expect(seen.back.s).toBe('\ufdd0inf');
    expect(seen.back.rate).toBe(Infinity);
    expect(seen.converted).toEqual([]);
    expect(seen.toml).toBe(true);
  });
});

test.describe('the vanilla example', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');
  test('the headless tools read a YAML comment line ending in a lone CR', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const seen = await page.evaluate(async () => {
      const tools = await import('../config-editor/pkg/config_editor.js');
      const text = '# top\rname: edge\rn: 1\n';
      return {
        parsed: tools.parse('yaml', text),
        converted: tools.parse('json', tools.convert('yaml', text, 'json').text),
        problems: tools.validate('yaml', text, '{"properties":{"n":{"type":"string"}}}').length,
      };
    });
    expect(seen.parsed).toEqual({ name: 'edge', n: 1 });
    expect(seen.converted).toEqual({ name: 'edge', n: 1 });
    expect(seen.problems).toBe(1);
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

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 22 (host API). `rangesOf` gives what `rangeOf` gives for every
  // path, and every range `rangeOf` gives is taken back as a host problem's
  // range and a read-only range, whichever of LF, CRLF and a lone CR end the
  // lines, in every language, in the host's view and through Edit as; and the
  // host LOWs: `colors` merge token by token, a throwing custom-language
  // callback is logged once, and `serialize` drops `undefined` members alike
  // in every language.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];

  const ENDINGS = { lf: '\n', crlf: '\r\n', cr: '\r' };

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


  test.describe('ranges, whichever breaks end the lines', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('rangesOf gives rangeOf for every path', async ({ page }) => {
      await open(page);
      const failures = await page.evaluate(
        ([languages, endings]) => {
          const t = window.tools;
          const failures = [];
          for (const language of languages) {
            for (const { text: lf, paths } of window.documents(language)) {
              for (const [name, ending] of Object.entries(endings)) {
                const text = lf.replace(/\n/g, ending);
                const many = t.rangesOf(language, text, paths);
                paths.forEach((path, index) => {
                  const one = t.rangeOf(language, text, path);
                  if (JSON.stringify(many[index]) !== JSON.stringify(one)) {
                    failures.push(`${language} ${name} ${path}: ${JSON.stringify(many[index])} vs ${JSON.stringify(one)}`);
                  }
                });
              }
            }
          }
          return failures;
        },
        [LANGUAGES, ENDINGS],
      );
      expect(failures).toEqual([]);
    });

    for (const view of ['host', 'editAs']) {
      test(`every rangeOf is taken as a host range and a read-only range (${view})`, async ({ page }) => {
        await open(page);
        const result = await page.evaluate(
          async ([languages, endings, view]) => {
            const t = window.tools;
            const failures = [];
            let checked = 0;
            const host = document.createElement('div');
            document.body.prepend(host);
            for (const language of languages) {
              for (const { text: lf, paths } of window.documents(language)) {
                for (const [name, ending] of Object.entries(endings)) {
                  const text = lf.replace(/\n/g, ending);
                  // Through Edit as, a text its language does not read (a lone
                  // CR in TOML or HCL) cannot be shown in another one.
                  if (view === 'editAs') {
                    try {
                      t.parse(language, text);
                    } catch {
                      continue;
                    }
                  }
                  const editAs = view === 'editAs' ? (language === 'yaml' ? 'json' : 'yaml') : undefined;
                  const editor = new window.module.ConfigEditor(host, { language, value: text, editAs });
                  for (const path of paths) {
                    // A host range is in the host's text: the editor's own
                    // `rangeOf` there, `tools.rangeOf` behind Edit as.
                    const range = view === 'host' ? editor.rangeOf(path) : t.rangeOf(language, text, path);
                    if (!range) continue;
                    const { start, end, ...lines } = range;
                    for (const [form, given] of [['both', range], ['indices', { start, end }], ['lines', lines]]) {
                      const at = `${language} ${name} ${path} ${form}`;
                      try {
                        editor.update({ hostDiagnostics: [{ range: given, message: 'host says' }] });
                        editor.update({ readOnlyRanges: [given] });
                        checked += 1;
                      } catch (error) {
                        failures.push(`${at}: ${error.message}`);
                      }
                    }
                  }
                  editor.destroy();
                }
              }
            }
            host.remove();
            return { failures, checked };
          },
          [LANGUAGES, ENDINGS, view],
        );
        expect(result.failures).toEqual([]);
        expect(result.checked).toBeGreaterThan(300);
      });
    }

    test('a host problem from rangeOf after a lone CR is marked on its line', async ({ page }) => {
      await open(page);
      const text = 'a: 1\rweb:\r  port: 80\r  host: x\r';
      await page.evaluate((text) => {
        const host = document.createElement('div');
        host.id = 'qa';
        document.body.prepend(host);
        window.qa = new window.module.ConfigEditor(host, { value: text });
        const range = window.qa.rangeOf('/web/host');
        window.qa.update({ hostDiagnostics: [{ range, message: 'host says' }], readOnlyRanges: [range] });
      }, text);
      await expect(page.locator('#qa [aria-label*=" on line 4"]').first()).toBeAttached();
    });
  });

  test.describe('host API details', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('serialize leaves out undefined members and refuses undefined items, in every language', async ({ page }) => {
      await open(page);
      const result = await page.evaluate((languages) => {
        const t = window.tools;
        const outcome = (run) => {
          try {
            return run();
          } catch (error) {
            return `throws: ${error.message}`;
          }
        };
        return Object.fromEntries(
          languages.map((language) => [
            language,
            [
              outcome(() => t.serialize(language, { r: { a: undefined, b: 1 } }) === t.serialize(language, { r: { b: 1 } })),
              outcome(() => t.serialize(language, { r: { a: [1, undefined, 3] } })),
              // eslint-disable-next-line no-sparse-arrays
              outcome(() => t.serialize(language, { r: { a: [1, , 3] } })),
            ],
          ]),
        );
      }, LANGUAGES);
      for (const language of LANGUAGES) {
        expect(result[language][0], language).toBe(true);
        expect(result[language][1], language).toMatch(/^throws: cannot write this value/);
        expect(result[language][2], language).toMatch(/^throws: cannot write this value/);
      }
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 23 (host API): list items holding a block (YAML `- name: web`,
  // a second HCL block, a TOML `[[array]]` and its entries) are found by
  // path in every language, and a host problem by such a path is marked on
  // the item; a nested `undefined` in `colors` and `strings` leaves the
  // entry as it was, as a top-level `undefined` leaves a setting.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  async function open(page) {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.module = await import('../config-editor/index.js');
      window.tools = await import('../config-editor/tools.js');
    });
  }


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('list items holding a block are found by path, and pathAt gives them back', async ({ page }) => {
      await open(page);
      const found = await page.evaluate(() => {
        const cases = [
          ['yaml', 'l:\n  - n: 1\n  - n: 2\n', ['/l/0', '/l/1']],
          ['yaml', 'spec:\n  containers:\n    - name: web\n    - name: side\n', ['/spec/containers/1']],
          ['hcl', 'b {\n  x = 1\n}\nb {\n  x = 2\n}\n', ['/b/1']],
          ['hcl', 'd "l" {\n  y = 1\n}\nd "l" {\n  y = 2\n}\n', ['/d/l/1']],
          ['toml', '[[l]]\nn = 1\n\n[[l]]\nn = 2\n', ['/l', '/l/1']],
          ['toml', '[t]\n[[t.l]]\nn = 1\n', ['/t/l']],
        ];
        const out = [];
        for (const [language, text, paths] of cases) {
          for (const ending of ['\n', '\r\n']) {
            const written = text.replaceAll('\n', ending);
            for (const path of paths) {
              const range = window.tools.rangeOf(language, written, path);
              const back = range && window.tools.pathAt(language, written, range.start)?.pointer;
              out.push(`${language} ${JSON.stringify(ending)} ${path}: ${range ? `line ${range.startLine}` : 'none'} -> ${back}`);
            }
          }
        }
        return out;
      });
      const lines = { '/l/0': 2, '/l/1': 3 };
      expect(found).toEqual([
        ...['"\\n"', '"\\r\\n"'].flatMap((ending) => ['/l/0', '/l/1'].map((path) => `yaml ${ending} ${path}: line ${lines[path]} -> ${path}`)),
        ...['"\\n"', '"\\r\\n"'].map((ending) => `yaml ${ending} /spec/containers/1: line 4 -> /spec/containers/1`),
        ...['"\\n"', '"\\r\\n"'].map((ending) => `hcl ${ending} /b/1: line 4 -> /b/1`),
        ...['"\\n"', '"\\r\\n"'].map((ending) => `hcl ${ending} /d/l/1: line 4 -> /d/l/1`),
        ...['"\\n"', '"\\r\\n"'].flatMap((ending) => [`toml ${ending} /l: line 1 -> /l/0`, `toml ${ending} /l/1: line 4 -> /l/1`]),
        ...['"\\n"', '"\\r\\n"'].map((ending) => `toml ${ending} /t/l: line 2 -> /t/l/0`),
      ]);
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

  const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('an index is found only on a list, in every language', async ({ page }) => {
      await open(page);
      const found = await page.evaluate(() => {
        const value = { name: 'web', o: { k: 1 }, l: [{ a: 1 }, { a: 2 }] };
        const out = {};
        for (const language of ['yaml', 'json', 'toml', 'hcl', 'xml']) {
          const prefix = language === 'xml' ? '/root' : '';
          const text = window.tools.serialize(language, language === 'xml' ? { root: value } : value);
          out[language] = Object.fromEntries(
            ['/name/0', '/o/0/0', '/o/k/0/0', '/l/0', '/l/0/0', '/l/1/0', '/l/1/a', '/0'].map((pointer) => {
              const at = pointer === '/0' ? pointer : `${prefix}${pointer}`;
              return [pointer, window.tools.rangeOf(language, text, at) !== null];
            }),
          );
        }
        return out;
      });
      for (const language of LANGUAGES) {
        // XML reads an element written once as a one-item list too.
        const lone = language === 'xml';
        expect(found[language], language).toEqual({
          '/name/0': lone,
          '/o/0/0': false,
          '/o/k/0/0': false,
          '/l/0': true,
          '/l/0/0': false,
          '/l/1/0': false,
          '/l/1/a': true,
          '/0': false,
        });
      }
    });
    test('the caret in a block-scalar key is on that key', async ({ page }) => {
      await open(page);
      const paths = await page.evaluate(() => {
        const text = '? |\n  multi\n  line\n: v\n';
        return ['multi', 'line', ': v'].map((at) => window.tools.pathAt('yaml', text, text.indexOf(at) + 1)?.pointer);
      });
      expect(paths).toEqual(['/multi\nline\n', '/multi\nline\n', '/multi\nline\n']);
    });
  });
});
