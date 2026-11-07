import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES, pointAt } from './helpers.mjs';

// The Rust API brought to the JS packages: the component's on/off props as
// options (vanilla) and props (React, Vue, Svelte), the handle's `text` and
// `foldedLines`, and the headless functions, for every language. Run
// against the built packages (`npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const PAGES = {
  'vanilla.html': (options) => `?options=${encodeURIComponent(JSON.stringify(options))}`,
  ...Object.fromEntries(FRAMEWORK_PAGES.map((name) => [name, (options) => `?props=${encodeURIComponent(JSON.stringify(options))}`])),
};

async function open(page, name, options = {}) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => ['error', 'warning'].includes(message.type()) && errors.push(message.text()));
  await page.goto(example(name) + PAGES[name](options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (page) => page.locator('.config-editor__input');

/** Types `text` on a new last line. */
async function typeAtEnd(page, text) {
  await input(page).click();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(text);
}

/** Nothing appears within half a second. */
async function staysAbsent(locator) {
  await locator.page().waitForTimeout(500);
  await expect(locator).toHaveCount(0);
}

/** How many times `onSave` was called. */
function saveCount(page, name) {
  return name === 'vanilla.html'
    ? page.evaluate(() => window.saves.length)
    : page.locator('#saves').getAttribute('data-count').then(Number);
}

for (const name of Object.keys(PAGES)) {
  test.describe(`${name}: the component's switches`, () => {
    test('nothing warns about the new options', async ({ page }) => {
      const errors = await open(page, name, {
        showDiff: true, showDiagnostics: true, completions: true, autoTriggerCompletions: true,
        hoverDocs: true, highlighting: true, autoClosing: true, styled: true, saveShortcut: true,
      });
      expect(errors).toEqual([]);
    });

    test('showDiff: false shows no gutter markers after an edit', async ({ page }) => {
      await open(page, name, { showDiff: false });
      await typeAtEnd(page, 'limits:');
      await expect(page.locator('#saved')).toContainText('limits:');
      await staysAbsent(page.locator('.config-editor__diff-marker'));
    });

    test('showDiff defaults to true', async ({ page }) => {
      await open(page, name);
      await typeAtEnd(page, 'limits:');
      await expect(page.locator('.config-editor__diff-marker').first()).toBeVisible();
    });

    test('showDiagnostics: false marks no problems', async ({ page }) => {
      await open(page, name, { showDiagnostics: false });
      await typeAtEnd(page, 'bogus_key: 1');
      await staysAbsent(page.locator('.config-editor__diagnostic-marker'));
    });

    test('completions: false offers nothing, even on Ctrl+Space', async ({ page }) => {
      await open(page, name, { completions: false });
      await typeAtEnd(page, 'lim');
      await page.keyboard.press('Control+Space');
      await staysAbsent(page.locator('.config-editor__completion-key'));
    });

    test('autoTriggerCompletions: false waits for Ctrl+Space', async ({ page }) => {
      await open(page, name, { autoTriggerCompletions: false });
      await typeAtEnd(page, 'lim');
      await staysAbsent(page.locator('.config-editor__completion-key'));
      await page.keyboard.press('Control+Space');
      await expect.poll(() => page.locator('.config-editor__completion-key').allTextContents()).toContain('limits');
    });

    test('hoverDocs: false shows no card on a key; the default does', async ({ page }) => {
      for (const [options, count] of [[{}, 1], [{ hoverDocs: false }, 0]]) {
        await open(page, name, options);
        const at = await pointAt(page, 1, 2);
        await page.mouse.move(at.x, at.y, { steps: 4 });
        if (count) await expect(page.locator('.config-editor__hover')).toHaveCount(1);
        else await staysAbsent(page.locator('.config-editor__hover'));
      }
    });

    test('highlighting: false colors nothing', async ({ page }) => {
      await open(page, name, { highlighting: false });
      await expect(page.locator('.config-editor__code-line').first()).toBeAttached();
      await expect(page.locator('.config-editor__key')).toHaveCount(0);
    });

    test('autoClosing: false types a bracket alone', async ({ page }) => {
      await open(page, name, { autoClosing: false });
      await typeAtEnd(page, 'x: [');
      await expect(input(page)).toHaveValue(/x: \[$/);
    });

    test('styled: false injects no stylesheet', async ({ page }) => {
      await open(page, name, { styled: false });
      await expect(page.locator('.config-editor style')).toHaveCount(0);
      const css = await page.evaluate(async () =>
        (window.packageApi ?? (await import('../config-editor/pkg/config_editor.js'))).presetCss(),
      );
      expect(css).toContain('.config-editor');
      expect(css).toContain('--config-editor-bg');
    });

    test('saveShortcut: false leaves Ctrl+S alone until switched on live', async ({ page }) => {
      await open(page, name, { saveShortcut: false });
      await typeAtEnd(page, 'x: 1');
      await page.keyboard.press('Control+s');
      await page.waitForTimeout(300);
      expect(await saveCount(page, name)).toBe(0);
      await page.evaluate((vanilla) => {
        if (vanilla) window.editor.setSettings({ ...window.editor.settings, saveShortcut: true });
        else window.setExtraProps({ saveShortcut: true });
      }, name === 'vanilla.html');
      await input(page).focus();
      await page.keyboard.press('Control+s');
      await expect.poll(() => saveCount(page, name)).toBe(1);
    });

    test('text is the text on screen and foldedLines the folds', async ({ page }) => {
      await open(page, name);
      const read = (property) =>
        page.evaluate((property) => (window.editor ?? window.editorHandle.current)[property], property);
      expect(await read('foldedLines')).toEqual([]);
      await page.evaluate(() => (window.editor ?? window.editorHandle.current).setValue('web:\n  http: 80\nlimits:\n  a: 1\n'));
      await page.evaluate(() => (window.editor ?? window.editorHandle.current).foldAll());
      expect(await read('foldedLines')).toEqual([{ startLine: 1, endLine: 2 }, { startLine: 3, endLine: 4 }]);
      await page.locator('#edit-as').selectOption('json');
      await expect.poll(() => read('text')).toContain('"http": 80');
      expect(await read('value')).toContain('http: 80');
    });
  });
}

test.describe('vanilla: the switches are checked like the other options', () => {
  test('a wrong type throws, setSettings checks its argument', async ({ page }) => {
    await open(page, 'vanilla.html');
    const messages = await page.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const caught = (run) => {
        try {
          run();
          return null;
        } catch (error) {
          return error.message;
        }
      };
      return {
        option: caught(() => new module.ConfigEditor(document.createElement('div'), { showDiff: 'no' })),
        setter: caught(() => window.editor.setSettings({ saveShortcut: 'yes' })),
      };
    });
    expect(messages).toEqual({
      option: 'showDiff must be a boolean (got string "no")',
      setter: 'saveShortcut must be a boolean (got string "yes")',
    });
  });
});

// One document per language: `web.http = 80` and `limits.a = 1`.
const DOCUMENTS = {
  yaml: 'web:\n  http: 80\nlimits:\n  a: 1\n',
  json: '{\n  "web": {\n    "http": 80\n  },\n  "limits": {\n    "a": 1\n  }\n}\n',
  toml: '[web]\nhttp = 80\n\n[limits]\na = 1\n',
  hcl: 'web {\n  http = 80\n}\n\nlimits {\n  a = 1\n}\n',
  xml: '<web>\n  <http>80</http>\n</web>\n<limits>\n  <a>1</a>\n</limits>\n',
};

test.describe('headless functions, every language', () => {
  for (const [language, text] of Object.entries(DOCUMENTS)) {
    test(`${language}: parse, serialize, rangeOf, pathAt, foldRanges`, async ({ page }) => {
      await open(page, 'vanilla.html');
      const result = await page.evaluate(
        async ({ language, text }) => {
          const api = await import('../config-editor/pkg/config_editor.js');
          const range = api.rangeOf(language, text, ['web', 'http']);
          const written = api.serialize(language, { web: { http: 80 }, limits: { a: 1 } });
          return {
            parsed: api.parse(language, text),
            roundTrip: api.parse(language, written),
            range: range && text.slice(range.start, range.end),
            rangeLine: range?.startLine,
            missing: api.rangeOf(language, text, '/web/nope'),
            path: api.pathAt(language, text, text.indexOf('80') + 1),
            pastEnd: api.pathAt(language, text, text.length + 1),
            folds: api.foldRanges(language, text),
            info: api.languageInfo(language),
          };
        },
        { language, text },
      );
      expect(result.parsed).toEqual({ web: { http: 80 }, limits: { a: 1 } });
      expect(result.roundTrip).toEqual(result.parsed);
      expect(result.range).toContain('http');
      expect(result.rangeLine).toBe(text.split('\n').findIndex((line) => line.includes('http')) + 1);
      expect(result.missing).toBeNull();
      expect(result.path).toEqual({ pointer: '/web/http', segments: ['web', 'http'] });
      expect(result.pastEnd).toBeNull();
      const webLine = text.split('\n').findIndex((line) => /\bweb\b/.test(line)) + 1;
      expect(result.folds.map((fold) => fold.startLine)).toContain(webLine);
      for (const fold of result.folds) expect(fold.endLine).toBeGreaterThan(fold.startLine);
      expect(result.info.name).toBe(language);
      expect(result.info.extensions).toContain(language);
      expect(result.info.defaultFileName).toBe(`untitled.${result.info.extensions[0]}`);
    });
  }

  for (const name of FRAMEWORK_PAGES) test(`the same answers as the editor, and from the ${name} package`, async ({ page }) => {
    await open(page, name);
    const result = await page.evaluate(() => {
      const api = window.packageApi;
      const handle = window.editorHandle.current;
      const text = handle.text;
      const index = text.indexOf('7835');
      return {
        range: [api.rangeOf('yaml', text, '/listen'), handle.rangeOf('/listen')],
        path: [api.pathAt('yaml', text, index), handle.pathAt(index)],
        summary: api.changeSummary('a: 1\nb: 2\nc: 3\n', 'a: 1\nb: 20\nc: 3\nd: 4\n'),
        file: [api.languageForFile('main.tf'), api.languageForFile('app.YML'), api.languageForFile('notes.txt')],
        info: api.languageInfo('yml'),
        theme: api.themeInfo('nord'),
        themes: api.themes().map((name) => api.themeInfo(name).label),
        css: api.presetCss().length > 1000,
      };
    });
    expect(result.range[0]).toEqual(result.range[1]);
    expect(result.path[0]).toEqual(result.path[1]);
    expect(result.summary).toEqual({ added: 1, removed: 0, modified: 1, hunks: 2 });
    expect(result.file).toEqual(['hcl', 'yaml', null]);
    expect(result.info).toEqual({ name: 'yaml', label: 'YAML', icon: expect.any(String), extensions: ['yaml', 'yml'], defaultFileName: 'untitled.yaml' });
    expect(result.theme).toEqual({ name: 'nord', label: 'Nord' });
    expect(result.themes).toContain('VS Code Dark');
    expect(result.css).toBe(true);
  });

  test('the README examples answer what the README says', async ({ page }) => {
    await open(page, 'vanilla.html');
    const result = await page.evaluate(async () => {
      const api = await import('../config-editor/pkg/config_editor.js');
      return [
        api.parse('toml', '[web]\nhttp = 80\n'),
        api.serialize('hcl', { web: { http: 80 } }),
        api.rangeOf('yaml', 'web:\n  http: 80\n', '/web/http'),
        api.foldRanges('json', '{\n  "web": {\n    "http": 80\n  }\n}'),
        api.languageForFile('deploy/main.tf'),
        api.languageInfo('yml'),
      ];
    });
    expect(result).toEqual([
      { web: { http: 80 } },
      'web {\n  http = 80\n}\n',
      { start: 7, end: 15, startLine: 2, startColumn: 3, endLine: 2, endColumn: 11 },
      [{ startLine: 1, endLine: 4 }, { startLine: 2, endLine: 3 }],
      'hcl',
      { name: 'yaml', label: 'YAML', icon: 'Y', extensions: ['yaml', 'yml'], defaultFileName: 'untitled.yaml' },
    ]);
  });

  test('registered languages answer too; errors are thrown with their message', async ({ page }) => {
    await open(page, 'vanilla.html');
    const result = await page.evaluate(async () => {
      const api = await import('../config-editor/pkg/config_editor.js');
      const caught = (run) => {
        try {
          run();
          return null;
        } catch (error) {
          return error instanceof Error ? error.message : `not an Error: ${error}`;
        }
      };
      return {
        file: api.languageForFile('.config/prod.env'),
        info: api.languageInfo('env'),
        parsed: api.parse('env', 'A=1\nB=x\n'),
        written: api.serialize('env', { A: 1 }),
        syntax: caught(() => api.parse('json', '{\n  "a": \n')),
        unsupported: caught(() => api.serialize('toml', { a: null })),
        notData: caught(() => api.serialize('yaml', { a: () => 1 })),
        unknown: caught(() => api.languageInfo('cobol')),
        badTheme: caught(() => api.themeInfo('neon')),
        badIndex: caught(() => api.pathAt('yaml', 'a: 1', -1)),
      };
    });
    expect(result.file).toBe('env');
    expect(result.info).toMatchObject({ name: 'env', label: 'ENV', icon: 'E', extensions: ['env'], defaultFileName: 'untitled.env' });
    expect(result.parsed).toEqual({ A: 1, B: 'x' });
    expect(result.written).toBe('A=1\n');
    expect(result.syntax).toMatch(/line \d+/);
    expect(result.unsupported).toMatch(/null/i);
    expect(result.notData).toContain('value is not JSON data');
    expect(result.unknown).toMatch(/cobol/);
    expect(result.badTheme).toMatch(/neon/);
    expect(result.badIndex).toBe('index must be a non-negative integer');
  });
});
