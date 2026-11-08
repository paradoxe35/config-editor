import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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

test.describe('host LOWs', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('colors merge token by token, null removing one or all', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const host = document.createElement('div');
      document.body.prepend(host);
      const editor = new window.module.ConfigEditor(host, { value: 'a: 1\n', colors: { bg: '#102030', key: '#ff0000' } });
      const out = [];
      editor.update({ colors: { key: '#00ff00' } });
      out.push(editor.settings.colors);
      editor.update({ colors: { bg: null } });
      out.push(editor.settings.colors);
      editor.update({ colors: { bg: '#102030' } });
      editor.update({ colors: null });
      out.push(editor.settings.colors);
      editor.destroy();
      return out;
    });
    expect(result).toEqual([{ bg: '#102030', key: '#00ff00' }, { key: '#00ff00' }, {}]);
  });

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
      expect(result[language][1], language).toMatch(/^throws: value is not JSON data/);
      expect(result[language][2], language).toMatch(/^throws: value is not JSON data/);
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
