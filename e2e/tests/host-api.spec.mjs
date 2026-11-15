import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The host API: options read alike, defaults, colors and strings merged, settings errors, callbacks
// after destroy() and given later.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const input = (page) => page.locator('#qa .config-editor__input');

const value = (page) => input(page).inputValue();

/** Puts the caret `offset` units into the first `marker`. */
async function caretAt(page, marker, offset = 0) {
  const text = await value(page);
  const at = text.indexOf(marker);
  expect(at, `${JSON.stringify(marker)} in ${JSON.stringify(text)}`).toBeGreaterThanOrEqual(0);
  await input(page).evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, at + offset);
}

const schema = readFileSync(join(root, 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'protected.schema.json'), 'utf8');

/** A new editor in `#qa` with `options`, recording what reaches the host. */
async function mount(page, options) {
  await page.evaluate(
    async ([options, schema]) => {
      const { ConfigEditor } = await import('../config-editor/index.js');
      window.editor?.destroy();
      window.qa?.destroy();
      let host = document.getElementById('qa');
      if (!host) {
        host = document.createElement('div');
        host.id = 'qa';
        document.body.prepend(host);
      }
      window.sent = [];
      window.qa = new ConfigEditor(host, {
        ...options,
        schema: options.schema ? schema : undefined,
        onChange: (text) => window.sent.push(text),
        onSave: (text) => window.sent.push(text),
      });
    },
    [options, schema],
  );
  await input(page).waitFor();
}

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
}

test.describe('host inputs and data', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
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
});

test.describe('callbacks after destroy()', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // Which callback destroys the editor, and how to make it fire.
  for (const destroyIn of ['onChange', 'onDirtyChange', 'onChangeSummary']) {
    test(`ConfigEditor: none fires after destroy() inside ${destroyIn}`, async ({ page }) => {
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(async (destroyIn) => {
        const { ConfigEditor } = await import('../config-editor/index.js');
        window.editor?.destroy();
        const host = document.createElement('div');
        host.id = 'qa';
        document.body.prepend(host);
        window.after = [];
        window.destroyed = false;
        // Armed once the typing starts (some fire at mount).
        window.armed = false;
        const record = (name) => () => {
          if (window.destroyed) window.after.push(name);
          if (name === destroyIn && window.armed && !window.destroyed) {
            window.destroyed = true;
            window.qa.destroy();
          }
        };
        const names = ['onChange', 'onChangeSummary', 'onDirtyChange', 'onPathChange', 'onSave', 'onEditAsChange', 'onBaselineConflict'];
        window.qa = new ConfigEditor(host, {
          language: 'yaml',
          value: 'name: edge\nlisten: 7835\n',
          ...Object.fromEntries(names.map((name) => [name, record(name)])),
        });
      }, destroyIn);
      await caretAt(page, 'edge', 4);
      await page.evaluate(() => (window.armed = true));
      await page.keyboard.type('12');
      await page.waitForTimeout(600);
      expect(await page.evaluate(() => window.destroyed)).toBe(true);
      expect(await page.evaluate(() => window.after)).toEqual([]);
      expect(await page.locator('#qa .config-editor').count()).toBe(0);
    });
  }

  test('ConfigDocuments: none fires after destroy() inside onChange', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      const { ConfigDocuments } = await import('../config-editor/index.js');
      window.editor?.destroy();
      const host = document.createElement('div');
      host.id = 'qa';
      document.body.prepend(host);
      window.after = [];
      window.destroyed = false;
      const record = (name) => () => {
        if (window.destroyed) window.after.push(name);
        if (name === 'onChange' && !window.destroyed) {
          window.destroyed = true;
          window.qa.destroy();
        }
      };
      window.qa = new ConfigDocuments(host, {
        documents: [{ id: 'a', fileName: 'a.yaml', value: 'name: edge\n' }],
        ...Object.fromEntries(['onChange', 'onDirtyChange', 'onSave', 'onSelect'].map((name) => [name, record(name)])),
      });
    });
    await caretAt(page, 'edge', 4);
    await page.keyboard.type('12');
    await page.waitForTimeout(600);
    expect(await page.evaluate(() => window.destroyed)).toBe(true);
    expect(await page.evaluate(() => window.after)).toEqual([]);
  });
});

test.describe('settings errors in JavaScript terms', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a wrong value names its JavaScript type, an unknown color token its nearest and where they are listed', async ({ page }) => {
    await open(page);
    await mount(page, { language: 'yaml', value: 'a: 1\n' });
    const messages = await page.evaluate(() =>
      [{ colors: { key: null } }, { colors: { key: 5 } }, { strings: { noProblems: 2.5 } }, { colors: { keys: 'red' } }].map((options) => {
        try {
          window.qa.update(options);
          return 'accepted';
        } catch (error) {
          return error.message;
        }
      }),
    );
    expect(messages).toEqual([
      // `null` resets one token to the theme's (colors merge token by token).
      'accepted',
      'colors.key must be a CSS color string (got number 5)',
      'strings.noProblems must be a string (got number 2.5)',
      expect.stringMatching(/unknown color token "keys" \(did you mean "key"\?\); the tokens are listed by ThemeColorToken/),
    ]);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
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


  test.describe('host API details', () => {
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
  });
});

// A group of its own (its own helpers).
test.describe(() => {
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
    test('a nested undefined leaves a color token or a text as it was; null resets it', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(() => {
        const host = document.createElement('div');
        document.body.prepend(host);
        const editor = new window.module.ConfigEditor(host, {
          value: 'a: 1\n',
          colors: { key: '#ff0000' },
          strings: { noProblems: 'Keine Fehler' },
        });
        const out = [];
        editor.update({ colors: { key: undefined }, strings: { noProblems: undefined } });
        out.push([editor.settings.colors, editor.settings.strings.noProblems]);
        editor.update({ colors: { key: null }, strings: { noProblems: null } });
        out.push([editor.settings.colors, editor.settings.strings.noProblems]);
        editor.destroy();
        // A document's own settings over the view's, alike.
        const view = new window.module.ConfigDocuments(host, {
          documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings: { colors: { key: undefined } } }],
          colors: { key: '#00ff00' },
        });
        out.push(view.editor('a')?.settings.colors ?? null);
        view.destroy();
        host.remove();
        return out;
      });
      expect(result[0]).toEqual([{ key: '#ff0000' }, 'Keine Fehler']);
      expect(result[1][0]).toEqual({});
      expect(result[1][1]).not.toBe('Keine Fehler');
      expect(result[2]).toEqual({ key: '#00ff00' });
    });
  });
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('a constructor takes a nested undefined or null in colors and strings as its default', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.prepend(host);
      const settings = {
        colors: { key: undefined, string: '#ff0000', number: null },
        strings: { noProblems: undefined, tabList: 'Dateien' },
      };
      const out = [];
      const editor = new module.ConfigEditor(host, { value: 'a: 1\n', ...settings });
      out.push([editor.settings.colors, editor.settings.strings.tabList, editor.settings.strings.noProblems]);
      editor.destroy();
      const view = new module.ConfigDocuments(host, {
        documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings }],
        ...settings,
      });
      out.push([view.settings.colors, view.settings.strings.tabList, view.settings.strings.noProblems]);
      view.destroy();
      host.remove();
      return out;
    });
    for (const [colors, tabList, noProblems] of result) {
      expect(colors).toEqual({ string: '#ff0000' });
      expect(tabList).toBe('Dateien');
      expect(typeof noProblems).toBe('string');
    }
  });
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  // The constructor took a `null` value that `update` refused; a document
  // could have the id "", and a documents view kept a schema that does not
  // compile where a ConfigEditor throws.
  test('options are read alike: a null text, an empty id and a bad schema throw', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.prepend(host);
      const attempt = (run) => {
        try {
          const made = run();
          made?.destroy?.();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const view = (documents) => new module.ConfigDocuments(host, { documents });
      const doc = { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' };
      const out = {
        editorValue: attempt(() => new module.ConfigEditor(host, { value: null })),
        editorBaseline: attempt(() => new module.ConfigEditor(host, { value: 'a: 1\n', baseline: null })),
        editorSchema: attempt(() => new module.ConfigEditor(host, { schema: '{bad' })),
        emptyId: attempt(() => view([{ ...doc, id: '' }])),
        documentsSchema: attempt(() => view([{ ...doc, schema: '{bad' }])),
      };
      // An update with a bad schema throws and changes nothing; the same
      // schema text again keeps the document's editor.
      const schema = JSON.stringify({ type: 'object', properties: { a: { type: 'integer' } } });
      const shown = view([{ ...doc, schema }]);
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
      const input = host.querySelector('.config-editor__input');
      out.updateSchema = attempt(() => shown.update({ documents: [{ ...doc, schema: '{bad' }] }));
      shown.update({ documents: [{ ...doc, value: 'a: 2\n', schema }] });
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
      out.kept = host.querySelector('.config-editor__input') === input;
      out.value = shown.editor('a').value;
      shown.destroy();
      host.remove();
      return out;
    });
    expect(result.editorValue).toContain('value must be a string');
    expect(result.editorBaseline).toContain('baseline must be a string');
    expect(result.editorSchema).not.toBe('no error');
    expect(result.emptyId).toContain('documents[0].id must not be empty');
    expect(result.documentsSchema).toContain('documents[0].schema');
    expect(result.updateSchema).toContain('documents[0].schema');
    expect(result.kept).toBe(true);
    expect(result.value).toBe('a: 2\n');
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
    test('a state callback added later hears the state at once, and both views tell the language on mount', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const mounted = [];
        new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\n', onEditAsChange: (language) => mounted.push(language) });
        const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: [\n' });
        await window.pause(400);
        const heard = [];
        editor.update({
          onSyncChange: (inSync) => heard.push(['sync', inSync]),
          onProblems: (problems) => heard.push(['problems', problems.length > 0]),
          onEditAsChange: (language) => heard.push(['language', language]),
        });
        const editorHeard = [...heard];
        const view = new window.module.ConfigDocuments(window.mountPoint(), { documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: [\n' }] });
        await window.pause(400);
        const documentsHeard = [];
        view.update({
          onSyncChange: (id, inSync) => documentsHeard.push(['sync', id, inSync]),
          onProblems: (id, problems) => documentsHeard.push(['problems', id, problems.length > 0]),
          onEditAsChange: (id, language) => documentsHeard.push(['language', id, language]),
        });
        // Given again: nothing new to hear.
        const again = documentsHeard.length;
        view.update({ onSyncChange: (id, inSync) => documentsHeard.push(['again', id, inSync]) });
        return { mounted, editorHeard, documentsHeard, again: documentsHeard.length - again };
      });
      expect(result.mounted).toEqual(['yaml']);
      expect(result.editorHeard).toEqual([
        ['language', 'yaml'],
        ['sync', true],
        ['problems', true],
      ]);
      expect(result.documentsHeard).toEqual([
        ['language', 'a', 'yaml'],
        ['sync', 'a', true],
        ['problems', 'a', true],
      ]);
      expect(result.again).toBe(0);
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
    page.on('console', (message) => {
      if (message.type() === 'error') page.errors.push(message.text());
    });
    page.on('pageerror', (error) => page.errors.push(String(error)));
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.editor?.destroy();
      window.module = await import('../config-editor/index.js');
      window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
      window.mountPoint = () => {
        const element = document.createElement('div');
        document.body.prepend(element);
        return element;
      };
    });
  }


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('destroy() works from a callback a method of the same instance fired', async ({ page }) => {
      await open(page);
      const results = await page.evaluate(async () => {
        const documents = () => [
          { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', baseline: 'a: 0\n' },
          { id: 'b', fileName: 'b.yaml', value: 'b: 1\n' },
        ];
        const cases = [
          ['ConfigDocuments saveAll() -> onSave', 'documents', 'onSave', (view) => view.saveAll()],
          ['ConfigDocuments save(id) -> onSave', 'documents', 'onSave', (view) => view.save('a')],
          ['ConfigDocuments select() -> onSelect', 'documents', 'onSelect', (view) => view.select('b')],
          ['ConfigDocuments update({ activeId }) -> onSelect', 'documents', 'onSelect', (view) => view.update({ activeId: 'b' })],
          ['ConfigDocuments update({ documents }) -> onSelect', 'documents', 'onSelect', (view) => view.update({ documents: [documents()[1]] })],
          ['ConfigDocuments editAs(id) -> onEditAsChange', 'documents', 'onEditAsChange', (view) => view.editAs('a', 'json')],
          ['ConfigEditor save() -> onSave', 'editor', 'onSave', (editor) => editor.save()],
          ['ConfigEditor update({ editAs }) -> onEditAsChange', 'editor', 'onEditAsChange', (editor) => editor.update({ editAs: 'json' })],
          ['ConfigEditor update({ baseline }) -> onDirtyChange', 'editor', 'onDirtyChange', (editor) => editor.update({ baseline: 'a: 1\n' })],
        ];
        const results = [];
        for (const [name, kind, callback, act] of cases) {
          const element = window.mountPoint();
          let instance;
          let armed = false;
          let fired = false;
          let error = null;
          const destroy = () => {
            if (!armed || fired) return;
            fired = true;
            try {
              instance.destroy();
            } catch (thrown) {
              error = String(thrown.message ?? thrown);
            }
          };
          instance =
            kind === 'documents'
              ? new window.module.ConfigDocuments(element, { documents: documents(), [callback]: destroy })
              : new window.module.ConfigEditor(element, { value: 'a: 1\n', baseline: 'a: 0\n', [callback]: destroy });
          await window.pause(300);
          armed = true;
          try {
            act(instance);
          } catch (thrown) {
            error = `${error ?? ''} act: ${thrown.message ?? thrown}`;
          }
          await window.pause(100);
          results.push({ name, fired, error, left: element.children.length });
          element.remove();
        }
        return results;
      });
      for (const result of results) expect(result).toEqual({ name: result.name, fired: true, error: null, left: 0 });
    });
    test('onDirtyChange and onChangeSummary given later hear the state at once', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\nb: 2\n', baseline: 'a: 0\nb: 2\n' });
        const view = new window.module.ConfigDocuments(window.mountPoint(), {
          documents: [
            { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', baseline: 'a: 0\n' },
            { id: 'b', fileName: 'b.yaml', value: 'b: 1\n' },
          ],
        });
        await window.pause(400);
        const editorHeard = [];
        editor.update({
          onDirtyChange: (dirty) => editorHeard.push(['dirty', dirty]),
          onChangeSummary: (summary) => editorHeard.push(['summary', summary]),
        });
        const documentsHeard = [];
        view.update({ onDirtyChange: (id, dirty) => documentsHeard.push(['dirty', id, dirty]) });
        return { editorHeard, documentsHeard };
      });
      expect(result.editorHeard).toEqual([
        ['dirty', true],
        ['summary', { added: 0, removed: 0, modified: 1, hunks: 1 }],
      ]);
      expect(result.documentsHeard).toEqual([
        ['dirty', 'a', true],
        ['dirty', 'b', false],
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


  // --- L30-5: an option's default is what a view built without it has.

  test.describe("an option's default is what a view left without it was built with", () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('update with the default of an option left out changes nothing; another value still throws', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(() => {
        const tried = (target, options) => {
          try {
            target.update(options);
            return 'ok';
          } catch (error) {
            return error.message.split(':')[0];
          }
        };
        const editor = new window.module.ConfigEditor(window.mountPoint(), { value: 'a: 1\n' });
        const view = new window.module.ConfigDocuments(document.createElement('div'), {
          documents: [{ id: 'a', fileName: 'a.yaml', value: '' }],
        });
        const built = new window.module.ConfigEditor(document.createElement('div'), { value: '', className: 'x', echoes: true });
        return {
          editor: [
            { echoes: false },
            { echoes: null },
            { className: null },
            { className: '' },
            { fileName: null },
            { fileName: '' },
            { fileName: 'untitled.yaml' },
          { schema: null },
            { secrets: null },
            { language: 'yaml' },
          ].map((options) => tried(editor, options)),
          documents: [{ echoes: false }, { label: null }, { className: null }, { className: '' }].map((options) => tried(view, options)),
          changed: [
            tried(editor, { echoes: true }),
            tried(editor, { className: 'y' }),
            tried(editor, { fileName: 'other.yaml' }),
          tried(view, { label: 'Files' }),
            tried(built, { className: null }),
            tried(built, { echoes: false }),
            tried(built, { className: 'x', echoes: true }),
          ],
        };
      });
      expect(result).toEqual({
        editor: Array(10).fill('ok'),
        documents: Array(4).fill('ok'),
        changed: [
          'echoes cannot change on a mounted ConfigEditor',
        'className cannot change on a mounted ConfigEditor',
        'fileName cannot change on a mounted ConfigEditor',
        'label cannot change on a mounted ConfigDocuments',
        'className cannot change on a mounted ConfigEditor',
        'echoes cannot change on a mounted ConfigEditor',
        'ok',
        ],
    });
      expect(page.errors).toEqual([]);
    });
  });
});
