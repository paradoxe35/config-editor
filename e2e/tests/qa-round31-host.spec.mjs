import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 31 (host API), one section per finding below.

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

// --- HO31-1: a text with a byte order mark through every headless tool, in
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

test.describe('QA round 31: headless tools and a byte order mark', () => {
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

// --- HO31-2: the async-check flow after Replace All over 300 lines: the
// host's locks and problems for the text it checked stay on their own
// lines (past ~512 diff steps everything between the first and last change
// was one changed stretch, locked and underlined whole). ----------------------

test.describe('QA round 31: ranges for a text many lines behind', () => {
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
