import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// JSON pointers through the JS package as RFC 6901 reads them: segments
// verbatim (` a`, `01`, `a~1b`, `m~0n` are keys), only `0` or digits
// without a leading zero index a list; and a path the document does not
// have (a missing parent, list index or leaf) is written nowhere: `rangeOf`,
// `rangesOf`, an editor's `rangeOf` and `reveal` answer that, in every
// language, rather than another field's place.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(page) {
  await page.goto(example);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.tools = await import('../config-editor/tools.js');
  });
}

test('keys that look like indices or hold spaces, slashes and tildes are keys', async ({ page }) => {
  await open(page);
  const failures = await page.evaluate((languages) => {
    const t = window.tools;
    const value = {
      m: { ' a': 'sp', '01': 'zero-one', 1: 'one', 'a/b': 'slash', 'm~n': 'tilde' },
      list: ['v0', 'v1', 'v2', 'v3', 'v4', 'v5', 'v6', 'v7', 'v8', 'v9', 'v10'],
    };
    const fields = [
      ['/m/ a', 'sp'],
      ['/m/01', 'zero-one'],
      ['/m/1', 'one'],
      ['/m/a~1b', 'slash'],
      ['/m/m~0n', 'tilde'],
      ['/m/+1', null],
      ['/list/0', 'v0'],
      ['/list/10', 'v10'],
      ['/list/01', null],
      ['/list/ 1', null],
    ];
    const failures = [];
    for (const language of languages) {
      // XML names cannot hold these keys: only its list.
      const document = language === 'xml' ? { list: value.list } : value;
      const text = t.serialize(language, language === 'xml' ? { root: document } : document);
      const at = (pointer) => (language === 'xml' ? `/root${pointer}` : pointer);
      const wanted = fields.filter(([pointer]) => language !== 'xml' || pointer.startsWith('/list'));
      const many = t.rangesOf(
        language,
        text,
        wanted.map(([pointer]) => at(pointer)),
      );
      wanted.forEach(([pointer, expected], index) => {
        const range = t.rangeOf(language, text, at(pointer));
        const slice = range && text.slice(range.start, range.end).replace(/"/g, '');
        const ok =
          expected === null
            ? range === null
            : slice !== null && (slice.endsWith(expected) || slice.includes(`>${expected}<`)) && !slice.endsWith(`0${expected}`);
        if (!ok) failures.push(`${language} ${pointer}: expected ${expected}, found ${slice}`);
        if (JSON.stringify(many[index]) !== JSON.stringify(range)) {
          failures.push(`${language} ${pointer}: rangesOf ${JSON.stringify(many[index])}, rangeOf ${JSON.stringify(range)}`);
        }
      });
    }
    return failures;
  }, LANGUAGES);
  expect(failures).toEqual([]);
});

test('a path the document does not have is written nowhere', async ({ page }) => {
  await open(page);
  const failures = await page.evaluate((languages) => {
    const t = window.tools;
    const value = {
      svc_0: { host: 'a', port: 1 },
      svc_1: { host: 'b', port: 2 },
      svc_2: { host: 'c', port: 3 },
      l: ['x', 'y', 'z'],
      nested: { l: [{ host: 'd' }, { host: 'e' }, { host: 'f' }] },
    };
    const missing = ['/svc_3/host', '/svc_0_x/host', '/svc_0/nope', '/l/3', '/l/7', '/nested/l/3/host', '/nested/x/0/host', '/nope'];
    const failures = [];
    for (const language of languages) {
      const xml = language === 'xml';
      const text = t.serialize(language, xml ? { root: value } : value);
      const paths = missing.map((pointer) => (xml ? `/root${pointer}` : pointer));
      const many = t.rangesOf(language, text, paths);
      paths.forEach((path, index) => {
        const one = t.rangeOf(language, text, path);
        if (one !== null) failures.push(`${language} ${path}: rangeOf ${text.slice(one.start, one.end)}`);
        if (many[index] !== null) failures.push(`${language} ${path}: rangesOf ${JSON.stringify(many[index])}`);
      });
    }
    return failures;
  }, LANGUAGES);
  expect(failures).toEqual([]);
});

test("an editor's rangeOf and reveal find no missing path", async ({ page }) => {
  await open(page);
  const found = await page.evaluate(async () => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const host = document.createElement('div');
    document.body.prepend(host);
    const editor = new ConfigEditor(host, {
      language: 'yaml',
      value: 'svc_0:\n  host: a\nsvc_1:\n  host: b\nm:\n  "01": x\n  "1": y\n',
    });
    const text = host.querySelector('.config-editor__input').value;
    const slice = (range) => range && text.slice(range.start, range.end);
    const result = {
      missing: editor.rangeOf('/svc_3/host'),
      revealed: editor.reveal('/svc_3/host'),
      zeroOne: slice(editor.rangeOf('/m/01')),
      one: slice(editor.rangeOf('/m/1')),
      segments: slice(editor.rangeOf(['m', '01'])),
    };
    editor.destroy();
    host.remove();
    return result;
  });
  expect(found).toEqual({ missing: null, revealed: false, zeroOne: '"01": x', one: '"1": y', segments: '"01": x' });
});
