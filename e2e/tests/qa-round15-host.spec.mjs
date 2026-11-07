import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// QA round 15: read-only blocks never grow, infinities stay numbers,
// placeholders are real only in their own field, serialize writes only what
// reads back the same, host inputs are checked, and a drag from a secret
// chip takes it whole.

const saved = (page) => page.locator('.demo__output-text');
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

// Where to type what, in the protected fixture of each language: a key
// inside the read-only `limits` block, typed key by key.
const EXTEND = {
  yaml: { after: '  burst: 20', keys: ['Enter'], typed: 'extra: 5' },
  json: { after: '"burst": 20', keys: [], typed: ',\n"extra": 5' },
  toml: { after: 'burst = 20\n', keys: [], typed: 'extra = 5\n' },
  hcl: { after: '  burst = 20\n', keys: [], typed: '  extra = 5\n' },
  xml: { after: '  <burst>20</burst>\n', keys: [], typed: '  <extra>5</extra>\n' },
};

/** Puts the caret at UTF-16 offset `at`. */
async function caretTo(page, at) {
  await editor(page).input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, at);
}

for (const [language, { after, keys, typed }] of Object.entries(EXTEND)) {
  test.describe(`${language}: a read-only block`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test('takes no new key, typed one key at a time', async ({ page }) => {
      const before = await saved(page).textContent();
      const text = await editor(page).value();
      await caretTo(page, text.indexOf(after) + after.length);
      for (const key of keys) await page.keyboard.press(key);
      await page.keyboard.type(typed);
      await page.waitForTimeout(300);
      expect(await editor(page).value()).not.toContain('extra');
      await expect(saved(page)).toHaveText(before);
    });
  });
}

test.describe('toml: a read-only table', () => {
  setupEditor(test, '?schema=protected&language=toml');

  test('takes no sub-table or dotted key from outside it', async ({ page }) => {
    for (const [at, typed] of [
      ['end', '[limits.sub]\nx = 1\n'],
      ['start', 'limits.extra = 1\n'],
    ]) {
      const text = await editor(page).value();
      await caretTo(page, at === 'end' ? text.length : 0);
      await page.keyboard.type(typed);
      await page.waitForTimeout(300);
      // A keystroke that would make the key name the locked table is
      // refused: what is left names another one.
      const now = await editor(page).value();
      expect(now).not.toContain('[limits.sub]');
      expect(now).not.toContain('limits.extra');
      await expect(saved(page)).not.toContainText('[limits.sub]');
      await expect(saved(page)).not.toContainText('limits.extra');
    }
  });
});

test.describe('yaml: a duplicate of a read-only key', () => {
  setupEditor(test, '?schema=protected');

  test('is refused at the end of the document', async ({ page }) => {
    const text = await editor(page).value();
    await caretTo(page, text.length);
    await page.keyboard.type('limits:\n  max: 99\nid: other\n');
    await page.waitForTimeout(300);
    // The keystroke making each a second key is refused.
    for (const text of [await editor(page).value(), await saved(page).textContent()]) {
      expect(text.match(/^limits:/gm)).toHaveLength(1);
      expect(text.match(/^id:/gm)).toHaveLength(1);
    }
  });
});

test.describe('secret placeholders in another field', () => {
  setupEditor(test, '?schema=protected');

  test("the key's token pasted into the pin is plain text and flagged", async ({ page }) => {
    const KEY = '«secret 3f2a»';
    const PIN = '«secret 0b1c»';
    const text = await editor(page).value();
    const pin = text.indexOf(PIN);
    // Select the pin's token whole and put the key's over it, as a paste.
    await editor(page).input.evaluate((element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    }, [pin, pin + PIN.length]);
    await page.keyboard.insertText(KEY);
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toContain(`pin: ${KEY}`);
    const chips = page.locator('.config-editor__code-line .config-editor__secret');
    await expect(chips).toHaveText([KEY]);
    await expect(
      page.locator('.config-editor__diagnostic-marker-warning[data-message*="stands for the secret of /tls/key"]'),
    ).toHaveCount(1);
  });

  test('a drag starting on a chip takes it whole, either way', async ({ page }) => {
    const KEY = '«secret 3f2a»';
    const start = (await editor(page).value()).indexOf(KEY);
    const end = start + KEY.length;
    // Pressed in the middle of the chip as drawn, as a user aims at it.
    const chip = await page.locator('.config-editor__code-line .config-editor__secret').first().boundingBox();
    const y = chip.y + chip.height / 2;
    for (const [x, check] of [
      [chip.x - 25, ([from, until]) => from < start && until === end],
      [chip.x + chip.width + 25, ([from, until]) => from === start && until >= end],
    ]) {
      await caretTo(page, 0);
      await page.mouse.move(chip.x + chip.width / 2, y);
      await page.mouse.down();
      await page.mouse.move(x, y, { steps: 5 });
      await page.mouse.up();
      await page.waitForTimeout(100);
      const selection = await editor(page).input.evaluate((element) => [element.selectionStart, element.selectionEnd]);
      expect(check(selection), `drag to x ${x}: selection ${selection} around the chip ${start}..${end}`).toBe(true);
    }
  });
});

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
        paths: caught(() => editor.setReadOnlyPaths(['/listen', 'nope'])),
        backwards: caught(() => editor.setReadOnlyRanges([{ start: 5, end: 2 }])),
        past: caught(() => editor.setReadOnlyRanges([{ startLine: 999, startColumn: 1 }])),
        both: caught(() => editor.setHostDiagnostics([{ path: '/listen', range: { start: 0, end: 1 }, message: 'x' }])),
        docs: caught(() => editor.setHostDiagnostics([{ path: '/listen', message: 'x', docsUrl: 'javascript:alert(1)' }])),
        badPath: caught(() => editor.setHostDiagnostics([{ message: 'ok' }, { path: 'listen', message: 'x' }])),
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

  test('a placeholder pattern that does not compile or matches empty text throws', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const errors = await page.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
      return ['(', '.*'].map((placeholder) => {
        const element = document.createElement('div');
        document.body.append(element);
        try {
          new ConfigEditor(element, { language: 'yaml', value: 'a: 1\n', secrets: { placeholder } }).destroy();
          return null;
        } catch (error) {
          return error.message;
        } finally {
          element.remove();
        }
      });
    });
    expect(errors[0]).toMatch(/secrets\.placeholder: "\(" is not a regular expression/);
    expect(errors[1]).toMatch(/secrets\.placeholder: "\.\*" matches empty text/);
  });

  for (const name of ['documents.html', 'documents-react.html']) {
    test(`${name}: editor(id) of an unknown document is null`, async ({ page }) => {
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const result = await page.evaluate(() => {
        const view = window.documentsView ?? window.documentsHandle?.current;
        return view ? view.editor('no-such-document') : 'no view';
      });
      expect(result).toBeNull();
    });
  }
});
