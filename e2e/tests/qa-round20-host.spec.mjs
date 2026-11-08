import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 20 (host): registered languages. Read-only fields hold in a
// language that gives no spans (found on a flat language's lines, else
// checked by value); a language that cannot write documents is never
// edited as another, either way; a document's language comes from its
// file name, registered extensions included; Ctrl+/ takes a registered
// language's string over lines whole; and invalid input is refused with
// the reason.

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
