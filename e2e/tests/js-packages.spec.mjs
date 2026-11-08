import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { frameworkCases } from './framework-cases.mjs';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// The npm packages in js/: the vanilla `config-editor` (wasm-bindgen) and the
// framework wrappers, through their example pages (the React component's own
// cases are in framework-cases.mjs, shared with Vue and Svelte). Build them first
// with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');
const saved = (browserPage) => browserPage.locator('#saved');

for (const name of ['vanilla.html', ...FRAMEWORK_PAGES]) {
  test.describe(name, () => {
    test('typing reaches onChange, suggestions work, and nothing logs errors', async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('lim');
      await expect
        .poll(() => browserPage.locator('.config-editor__completion-key').allTextContents())
        .toContain('limits');
      await browserPage.keyboard.press('Escape');
      await browserPage.keyboard.type('its:');
      await expect(saved(browserPage)).toContainText('limits:');
      expect(errors).toEqual([]);
    });

    test('schema problems are marked', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('bogus_key: 1');
      await expect(browserPage.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    });

    test('edit as switches live and saves back in the host language', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.locator('#edit-as').selectOption('json');
      await expect(input(browserPage)).toHaveValue(/"listen": 7835/);
      await input(browserPage).evaluate((element) => {
        const at = element.value.indexOf('7835');
        element.focus();
        element.setSelectionRange(at, at + 4);
      });
      await browserPage.keyboard.type('9999');
      await expect(saved(browserPage)).toContainText('listen: 9999');
      await expect(saved(browserPage)).toContainText('# Burrow server configuration');
    });

    test('value, theme and read-only update live', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.locator('#replace').click();
      await expect(input(browserPage)).toHaveValue('listen: 9000\n');
      await browserPage.locator('#theme').selectOption('nord');
      await expect(browserPage.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
      await browserPage.locator('#read-only').check();
      await input(browserPage).click();
      await browserPage.keyboard.type('x');
      await expect(input(browserPage)).toHaveValue('listen: 9000\n');
    });
  });
}

test.describe('vanilla.html', () => {
  test('a language registered from JavaScript edits, validates and converts', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#edit-as').selectOption('env');
    await expect(input(browserPage)).toHaveValue(/listen=7835/);
    await expect(browserPage.locator('.config-editor__key').first()).toBeVisible();
    await input(browserPage).evaluate((element) => {
      const at = element.value.indexOf('false');
      element.focus();
      element.setSelectionRange(at, at + 5);
    });
    // `insecure_plaintext` is a boolean: schema validation, placed through
    // the JS language's key positions.
    await browserPage.keyboard.type('maybe');
    await expect(browserPage.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
  });

  test('destroy removes the editor', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#destroy').click();
    await expect(input(browserPage)).toHaveCount(0);
  });

  test('headless validation is exposed', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const problems = await browserPage.evaluate(() => window.configEditor.validate('json', '{"a": }'));
    expect(problems).toHaveLength(1);
    expect(problems[0].severity).toBe('error');
  });
});

test.describe('vanilla API robustness', () => {
  test('destroy right after typing is clean', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await input(browserPage).click();
    await browserPage.keyboard.type('bogus');
    await browserPage.evaluate(() => window.editor.destroy());
    await browserPage.waitForTimeout(600);
    expect(errors).toEqual([]);
  });

  test('bad arguments throw clear errors instead of crashing', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const messages = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return {
        element: attempt(() => new module.ConfigEditor(null, {})),
        theme: attempt(() => window.editor.update({ theme: 5 })),
        value: attempt(() => window.editor.update({ value: null })),
        validate: attempt(() => module.validate('yaml', 5)),
        extensions: attempt(() => module.registerLanguage({ label: 'Plain', parse: () => ({}) })),
        builtIn: attempt(() => module.registerLanguage({ label: 'YAML', parse: () => ({}) })),
      };
    });
    expect(messages.element).toContain('HTMLElement');
    expect(messages.theme).toContain('theme must be a string');
    expect(messages.value).toContain('value must be a string');
    expect(messages.validate).toContain('text must be a string');
    expect(messages.extensions).toBe('no error');
    expect(messages.builtIn).toContain('built-in');
  });

  test('an edit-as that cannot convert yet waits in the host language', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const state = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      const host = document.getElementById('editor');
      const changes = [];
      const editor = new module.ConfigEditor(host, {
        language: 'yaml',
        value: 'listen: 1\npublic_address: null\n',
        editAs: 'toml',
        onChange: (text) => changes.push(text),
      });
      window.waiting = { editor, changes };
      return editor.value;
    });
    expect(state).toContain('public_address: null');
    await expect(browserPage.locator('.config-editor__input')).toHaveValue(/public_address: null/);
    await expect(browserPage.locator('.config-editor__diagnostic-marker').first()).toHaveAttribute(
      'data-message',
      /Cannot switch to TOML yet/,
    );
    expect(errors).toEqual([]);
  });
});

test.describe('vanilla API: callbacks, teardown and custom languages', () => {
  const withModule = (browserPage, run) =>
    browserPage.evaluate(async (source) => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      const host = document.getElementById('editor');
      // eslint-disable-next-line no-new-func
      return new Function('module', 'host', `return (async () => { ${source} })()`)(module, host);
    }, run);

  test('destroy inside onChange unmounts cleanly', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await withModule(
      browserPage,
      `const editor = new module.ConfigEditor(host, { value: 'a: 1\\n', onChange: () => editor.destroy() });
       window.target = editor;`,
    );
    await input(browserPage).click();
    await browserPage.keyboard.type('x');
    await expect(input(browserPage)).toHaveCount(0);
    expect(await browserPage.evaluate(() => window.target.value)).toContain('x');
    await browserPage.waitForTimeout(400);
    expect(errors).toEqual([]);
  });

  test('destroy right after an edit-as edit still saves it', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await withModule(
      browserPage,
      `window.changes = [];
       window.target = new module.ConfigEditor(host, {
         value: 'listen: 1\\n', editAs: 'json', onChange: (text) => window.changes.push(text),
       });`,
    );
    await input(browserPage).evaluate((element) => {
      const at = element.value.indexOf('1');
      element.focus();
      element.setSelectionRange(at, at + 1);
    });
    await browserPage.keyboard.type('2');
    const state = await browserPage.evaluate(() => {
      window.target.destroy();
      return { changes: window.changes, value: window.target.value };
    });
    expect(state.changes.at(-1)).toBe('listen: 2\n');
    expect(state.value).toBe('listen: 2\n');
    expect(errors).toEqual([]);
  });

  test('options of the wrong type are refused, and the element is cleared', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const messages = await withModule(
      browserPage,
      `const attempt = (options) => { try { new module.ConfigEditor(host, options).destroy(); return 'no error'; } catch (error) { return String(error.message ?? error); } };
       host.innerHTML = '<p id="old">old</p>';
       const kept = new module.ConfigEditor(host, { value: 'a: 1\\n' });
       const cleared = !document.getElementById('old');
       kept.destroy();
       let readOnly;
       try { const editor = new module.ConfigEditor(host, {}); editor.update({ readOnly: 'yes' }); editor.destroy(); readOnly = 'no error'; } catch (error) { readOnly = String(error.message); }
       return {
         cleared,
         value: attempt({ value: 5 }),
         readOnly: attempt({ readOnly: 'yes' }),
         onChange: attempt({ onChange: 'save' }),
         setReadOnly: readOnly,
       };`,
    );
    expect(messages.cleared).toBe(true);
    expect(messages.value).toContain('value must be a string');
    expect(messages.readOnly).toContain('readOnly must be a boolean');
    expect(messages.onChange).toContain('onChange must be a function');
    expect(messages.setReadOnly).toContain('readOnly must be a boolean');
  });

  test('a custom language returning bad data is an error, not a crash', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const problems = await withModule(
      browserPage,
      `module.registerLanguage({
         label: 'Loop', extensions: [],
         parse: () => { const value = {}; value.self = value; return value; },
         highlight: () => null,
         serialize: () => { throw 'no writer'; },
       });
       window.target = new module.ConfigEditor(host, { language: 'loop', value: 'anything' });
       return module.validate('loop', 'anything');`,
    );
    expect(problems).toHaveLength(1);
    expect(problems[0].message).toMatch(/cyclic/);
    await expect(input(browserPage)).toHaveValue('anything');
    await input(browserPage).click();
    await browserPage.keyboard.type('!');
    await expect(input(browserPage)).toHaveValue('anything!');
    const converted = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      try {
        module.convert('json', '{}', 'loop');
        return 'no error';
      } catch (error) {
        return String(error.message ?? error);
      }
    });
    expect(converted).toContain('no writer');
    expect(errors).toEqual([]);
  });
});

test.describe('vanilla API: languages described by configuration', () => {
  test('comments, brackets and separators give a JS language editing behavior', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const refused = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      const attempt = (definition) => {
        try {
          module.registerLanguage({ label: 'Bad', parse: () => ({}), ...definition });
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const refusals = {
        comments: attempt({ comments: { lineComment: 5 } }),
        brackets: attempt({ brackets: [['{{', '}}']] }),
        separators: attempt({ separators: '=' }),
      };
      module.registerLanguage({
        label: 'INI',
        parse: (text) => Object.fromEntries(text.split('\n').filter((line) => line.includes('=')).map((line) => line.split('=').map((part) => part.trim()))),
        comments: { lineComment: [';', '#'] },
        brackets: [['[', ']']],
        separators: ['='],
      });
      window.target = new module.ConfigEditor(document.getElementById('editor'), { language: 'ini', value: 'name = demo\n' });
      return refusals;
    });
    expect(refused.comments).toContain('comments.lineComment');
    expect(refused.brackets).toContain('single characters');
    expect(refused.separators).toContain('array');
    // Ctrl+/ writes the first line comment.
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+Home');
    await browserPage.keyboard.press('Control+/');
    await expect(input(browserPage)).toHaveValue('; name = demo\n');
    // Enter between brackets opens an indented line.
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('hosts = [');
    await browserPage.keyboard.press('Enter');
    await expect(input(browserPage)).toHaveValue('; name = demo\nhosts = [\n  \n]');
    expect(errors).toEqual([]);
  });
});

test.describe('vanilla API: fourth-round findings', () => {
  test('an editor nobody references stays mounted through garbage collection', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      window.editor = undefined;
      // No reference kept, as in the README quick start.
      new module.ConfigEditor(document.getElementById('editor'), { value: 'listen: 1\n' });
    });
    const client = await browserPage.context().newCDPSession(browserPage);
    for (let round = 0; round < 3; round += 1) {
      await client.send('HeapProfiler.collectGarbage');
      await browserPage.waitForTimeout(200);
    }
    await expect(input(browserPage)).toHaveValue('listen: 1\n');
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('x');
    await expect(input(browserPage)).toHaveValue('listen: 1\nx');
    expect(errors).toEqual([]);
  });

  test('whole numbers from a JavaScript language stay integers', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const converted = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      module.registerLanguage({ label: 'Num', parse: () => ({ n: 5, list: [1, 2], ratio: 0.5 }) });
      return ['yaml', 'toml', 'json'].map((target) => module.convert('num', 'x', target).text);
    });
    expect(converted[0]).toBe('n: 5\nlist:\n  - 1\n  - 2\nratio: 0.5\n');
    expect(converted[1]).toContain('n = 5\n');
    expect(converted[2]).toContain('"n": 5');
  });

  test('an editAs applied at mount reports onEditAsChange', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const reported = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      const languages = [];
      window.target = new module.ConfigEditor(document.getElementById('editor'), {
        value: 'listen: 1\n',
        editAs: 'json',
        onEditAsChange: (language) => languages.push(language),
      });
      await new Promise((resolve) => setTimeout(resolve, 100));
      return languages;
    });
    expect(reported).toEqual(['json']);
  });

  test('update checks its argument, callbacks and settings', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const messages = await browserPage.evaluate(() => {
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return {
        callbackType: attempt(() => window.editor.update({ onChange: 'x' })),
        notObject: attempt(() => window.editor.update(5)),
        array: attempt(() => window.editor.update([])),
        colorsNumber: attempt(() => window.editor.update({ colors: 5 })),
        colorsString: attempt(() => window.editor.update({ colors: 'red' })),
        unknownToken: attempt(() => window.editor.update({ colors: { keys: 'red' } })),
        removal: attempt(() => window.editor.update({ onChange: null })),
      };
    });
    expect(messages.callbackType).toContain('onChange must be a function or null');
    expect(messages.notObject).toContain('update takes an object of options');
    expect(messages.array).toContain('update takes an object of options');
    expect(messages.colorsNumber).toContain('colors must be an object');
    expect(messages.colorsString).toContain('colors must be an object');
    expect(messages.unknownToken).toContain('colors: unknown color token "keys"');
    expect(messages.removal).toBe('no error');
  });

  test('switching back to the host language during the typing pause saves the edit', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#edit-as').selectOption('json');
    await expect(input(browserPage)).toHaveValue(/"listen": 7835/);
    await input(browserPage).evaluate((element) => {
      const at = element.value.indexOf('7835') + 4;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await browserPage.keyboard.type('1');
    await browserPage.evaluate(() => window.editor.update({ editAs: null }));
    await expect(saved(browserPage)).toContainText('listen: 78351');
    expect(await browserPage.evaluate(() => window.editor.value)).toContain('listen: 78351');
  });
});

test.describe('vanilla API: update right after construction', () => {
  test('update({ editAs }) in the same task as the constructor switches the view', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.createElement('div');
      document.body.append(element);
      const reported = [];
      const editor = new module.ConfigEditor(element, { value: 'listen: 7835\n', onEditAsChange: (name) => reported.push(name) });
      editor.update({ editAs: 'json' });
      await new Promise((resolve) => setTimeout(resolve, 100));
      const shown = element.querySelector('textarea').value;
      const back = new module.ConfigEditor(document.body.appendChild(document.createElement('div')), { value: 'listen: 7835\n', editAs: 'json' });
      back.update({ editAs: null });
      await new Promise((resolve) => setTimeout(resolve, 100));
      return { shown, reported, back: back.value, backShown: document.body.lastChild.querySelector('textarea').value };
    });
    expect(result.shown).toContain('"listen": 7835');
    expect(result.reported).toEqual(['json']);
    expect(result.backShown).toBe('listen: 7835\n');
  });
});

test.describe('vanilla API: registerLanguage checks its fields', () => {
  test('an icon longer than two letters or extensions given as a string are refused', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const errors = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const attempt = (definition) => {
        try {
          module.registerLanguage({ parse: () => ({}), ...definition });
          return null;
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const results = [
        attempt({ label: 'IconLong', icon: 'ABCDEFG' }),
        attempt({ label: 'ExtString', extensions: 'env' }),
        attempt({ label: 'Fine', icon: 'Fi', extensions: ['.fine'] }),
        attempt({ label: 'IconEmoji', icon: '\u{1F600}' }),
        attempt({ label: 'ExtBuiltIn', extensions: ['yml'] }),
        attempt({ label: 'CommentsString', comments: '#' }),
        attempt({ label: 'SpanNumber', keySpan: 5 }),
        attempt({ label: 'SerializeString', serialize: 'x' }),
      ];
      // An extension names the language, as `yml` names YAML.
      let byExtension = null;
      try {
        byExtension = module.convert('fine', 'x', 'json').text;
      } catch (error) {
        byExtension = String(error.message ?? error);
      }
      return [...results, byExtension];
    });
    expect(errors[0]).toContain('icon');
    expect(errors[1]).toContain('extensions');
    expect(errors[2]).toBeNull();
    expect(errors[3]).toContain('icon');
    expect(errors[4]).toContain('built-in');
    expect(errors[5]).toContain('comments');
    expect(errors[6]).toContain('keySpan must be a function');
    expect(errors[7]).toContain('serialize must be a function');
    expect(errors[8]).not.toContain('unknown');
  });
});

test.describe('vanilla API: multi-line text inserted without a paste event', () => {
  test('is one change, fast, and undone in one step', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      let changes = 0;
      const editor = new module.ConfigEditor(element, { value: 'a: 1\n', onChange: () => (changes += 1) });
      await new Promise((resolve) => setTimeout(resolve, 0));
      const textarea = element.querySelector('textarea');
      textarea.focus();
      textarea.setSelectionRange(textarea.value.length, textarea.value.length);
      const lines = Array.from({ length: 200 }, (_, index) => `key_${index}: ${index}`).join('\n');
      const started = performance.now();
      document.execCommand('insertText', false, lines);
      await new Promise((resolve) => setTimeout(resolve, 0));
      const took = performance.now() - started;
      window.burstEditor = editor;
      return { changes, took, value: editor.value, expected: 'a: 1\n' + lines };
    });
    expect(result.value).toBe(result.expected);
    expect(result.changes).toBe(1);
    expect(result.took).toBeLessThan(1000);
    await browserPage.keyboard.press('Control+z');
    expect(await browserPage.evaluate(() => window.burstEditor.value)).toBe('a: 1\n');
  });
});

test.describe('vanilla API: input in the same task as an API call', () => {
  test('update({ value }), value and destroy see input the browser sent just before', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const mount = (options) => {
        const element = document.body.appendChild(document.createElement('div'));
        const changes = [];
        const editor = new module.ConfigEditor(element, { ...options, onChange: (text) => changes.push(text) });
        return { editor, textarea: element.querySelector('textarea'), changes };
      };
      // The textarea holds the document straight after the constructor.
      const first = mount({ value: 'a: 1\n' });
      const shownAtOnce = first.textarea.value;
      await new Promise((resolve) => setTimeout(resolve, 0));
      // Input then update({ value }) in one task: the reset wins.
      first.textarea.focus();
      first.textarea.setSelectionRange(3, 4);
      document.execCommand('insertText', false, '5');
      first.editor.update({ value: 'reset: 0\n' });
      const afterReset = first.editor.value;
      await new Promise((resolve) => setTimeout(resolve, 0));
      const afterResetLater = first.editor.value;
      // Input then value in one task: the value includes it.
      first.textarea.setSelectionRange(first.textarea.value.length, first.textarea.value.length);
      document.execCommand('insertText', false, 'x');
      const valueRightAway = first.editor.value;
      // Input then destroy in one task: the edit still reaches onChange.
      const second = mount({ value: 'b: 1\n' });
      await new Promise((resolve) => setTimeout(resolve, 0));
      second.textarea.focus();
      second.textarea.setSelectionRange(3, 4);
      document.execCommand('insertText', false, '7');
      second.editor.destroy();
      return { shownAtOnce, afterReset, afterResetLater, valueRightAway, destroyed: second.changes.at(-1), destroyedValue: second.editor.value };
    });
    expect(result.shownAtOnce).toBe('a: 1\n');
    expect(result.afterReset).toBe('reset: 0\n');
    expect(result.afterResetLater).toBe('reset: 0\n');
    expect(result.valueRightAway).toBe('reset: 0\nx');
    expect(result.destroyed).toBe('b: 7\n');
    expect(result.destroyedValue).toBe('b: 7\n');
  });
});

test.describe('vanilla API: misspelled keys are warned about', () => {
  test('update and registerLanguage name the key they ignore, with a hint', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const warnings = [];
    browserPage.on('console', (message) => {
      if (message.type() === 'warning') warnings.push(message.text());
    });
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      const editor = new module.ConfigEditor(element, { value: 'a: 1\n' });
      editor.update({ onChnge: () => {} });
      module.registerLanguage({ label: 'Typo', parse: () => ({}), seperators: ['='] });
    });
    await expect.poll(() => warnings.join('\n')).toContain('"onChnge" is ignored (did you mean "onChange"?)');
    expect(warnings.join('\n')).toContain('"seperators" is ignored (did you mean "separators"?)');
  });
});

test.describe('vanilla API: positions from a JavaScript language', () => {
  test('a span outside the text is no position, not a wrong one', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const problems = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      module.registerLanguage({
        label: 'Far',
        parse: () => ({ listen: 'not a port' }),
        keySpan: () => ({ line: 999, startColumn: 5, endColumn: 2 }),
        valueSpan: () => ({ line: 1, startColumn: 50, endColumn: 9 }),
      });
      const schema = JSON.stringify({ type: 'object', properties: { listen: { type: 'integer' } } });
      return module.validate('far', 'listen=x', schema);
    });
    expect(problems).toHaveLength(1);
    const [problem] = problems;
    // On line 1 and clamped onto it, or unplaced: never line 999.
    expect(problem.line === undefined || problem.line === 1).toBe(true);
    if (problem.line === 1) {
      expect(problem.column).toBeLessThanOrEqual(9);
      expect(problem.endColumn).toBeGreaterThanOrEqual(problem.column);
    }
  });
});

test.describe('vanilla API: mounting into an element that holds an editor', () => {
  test('replaces the old editor instead of leaking it', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const megabytes = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const wasm = await module.default();
      window.editor.destroy();
      const host = document.getElementById('editor');
      const value = Array.from({ length: 1500 }, (_, index) => `key${index}: ${index}`).join('\n');
      // A replaced editor's memory is free once the task that replaced it
      // ends (its last effects let go of it then): the first batches grow
      // memory to its peak, later ones reuse it.
      const batch = async () => {
        for (let round = 0; round < 20; round += 1) {
          // As in the README quick start: no reference kept, no destroy().
          new module.ConfigEditor(host, { value });
        }
        await new Promise((resolve) => setTimeout(resolve, 300));
        return wasm.memory.buffer.byteLength / 1024 / 1024;
      };
      await batch();
      const peak = await batch();
      await batch();
      return (await batch()) - peak;
    });
    expect(megabytes).toBeLessThan(0.5);
    await expect(input(browserPage)).toHaveCount(1);
    await expect(input(browserPage)).toHaveValue(/key1499: 1499$/);
  });
});

test.describe('round 5: package polish', () => {
  test('the toolbar shows the default file name', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.target = new module.ConfigEditor(document.getElementById('editor'), { language: 'toml', showToolbar: true });
    });
    await expect(browserPage.locator('.config-editor__toolbar')).toContainText('untitled.toml');
  });

  test('editAs into a language that cannot write documents is refused', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const messages = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      module.registerLanguage({ label: 'ReadOnlyLang', parse: () => ({}) });
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return {
        setter: attempt(() => window.editor.update({ editAs: 'readonlylang' })),
        option: attempt(() => new module.ConfigEditor(document.createElement('div'), { editAs: 'readonlylang' })),
      };
    });
    expect(messages.setter).toContain('cannot write documents');
    expect(messages.option).toContain('cannot write documents');
  });

  test('the TypeScript declarations hold only the public API', async () => {
    const { readFileSync } = await import('node:fs');
    const types = readFileSync(join(js, 'config-editor', 'pkg', 'config_editor.d.ts'), 'utf8');
    expect(types).not.toContain('ReadableStreamType');
    expect(types).not.toMatch(/__wbg_(?!init)/);
    expect(types).not.toContain('IntoUnderlying');
    expect(types).toContain('update(options: EditorUpdate): void;');
  });

  test('a failed load can be retried', async ({ page: browserPage }) => {
    await browserPage.goto(page('load-retry.html'));
    await expect(browserPage.locator('body')).toHaveAttribute('data-result', 'failed,loaded', { timeout: 20000 });
  });

  test('a label is a name in any script, lowercased', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const name = module.registerLanguage({ label: 'Énv', parse: () => ({}) });
      window.target = new module.ConfigEditor(document.createElement('div'), { language: 'ÉNV' });
      return name;
    });
    expect(result).toBe('énv');
  });

  test('highlight ranges are JavaScript string indices', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      module.registerLanguage({
        label: 'Idx',
        parse: () => ({}),
        highlight: (line) => {
          const equals = line.indexOf('=');
          return equals < 0 ? [] : [{ start: 0, end: equals, kind: 'key' }];
        },
      });
      window.target = new module.ConfigEditor(document.getElementById('editor'), { language: 'idx', value: 'café=1\n🙂x=2\n' });
    });
    const keys = await browserPage.locator('.config-editor__key').allTextContents();
    expect(keys).toEqual(['café', '🙂x']);
  });

  test('an unknown option is reported, with the likely meant one', async ({ page: browserPage }) => {
    const warnings = [];
    browserPage.on('console', (message) => message.type() === 'warning' && warnings.push(message.text()));
    await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.target = new module.ConfigEditor(document.createElement('div'), { readonly: true });
    });
    expect(warnings.join('\n')).toContain('unknown option "readonly" is ignored (did you mean "readOnly"?)');
  });
});

test.describe('round 11', () => {
  test('vanilla: input right after the constructor or update({ value }), in the same task, is kept', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      const changes = [];
      const editor = new module.ConfigEditor(element, { value: 'listen: 1\n', onChange: (text) => changes.push(text) });
      const textarea = element.querySelector('textarea');
      textarea.focus();
      textarea.setSelectionRange(10, 10);
      document.execCommand('insertText', false, 'Q');
      await new Promise((resolve) => setTimeout(resolve, 50));
      const afterConstructor = editor.value;
      editor.update({ value: 'x: 1\n' });
      textarea.setSelectionRange(5, 5);
      document.execCommand('insertText', false, 'R');
      await new Promise((resolve) => setTimeout(resolve, 50));
      return { afterConstructor, afterSetValue: editor.value, last: changes.at(-1) };
    });
    expect(result.afterConstructor).toBe('listen: 1\nQ');
    expect(result.afterSetValue).toBe('x: 1\nR');
    expect(result.last).toBe('x: 1\nR');
  });

  test('vanilla: registered languages cannot take each other’s names or extensions', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const parse = () => ({});
      const attempt = (definition) => {
        try {
          module.registerLanguage({ parse, ...definition });
          return 'ok';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return {
        first: attempt({ label: 'Aaa1', extensions: ['ccc1'] }),
        extensionNamesOther: attempt({ label: 'Bbb1', extensions: ['aaa1'] }),
        labelIsOthersExtension: attempt({ label: 'Ccc1' }),
        sharedExtension: attempt({ label: 'Ddd1', extensions: ['ccc1'] }),
        again: attempt({ label: 'Aaa1', extensions: ['ccc1'] }),
        emptyComment: attempt({ label: 'Eee1', comments: { lineComment: '' } }),
        separatorsString: attempt({ label: 'Fff1', separators: '=' }),
      };
    });
    expect(result.first).toBe('ok');
    expect(result.extensionNamesOther).toContain('aaa1');
    expect(result.labelIsOthersExtension).toContain('ccc1');
    expect(result.sharedExtension).toContain('ccc1');
    expect(result.again).toBe('ok');
    expect(result.emptyComment).toContain('non-empty');
    expect(result.separatorsString).toContain('array');
  });

  test('vanilla: a problem without a position has no position fields, not null ones', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const problems = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      module.registerLanguage({ label: 'Badpos1', parse: () => { throw { message: 'bad', line: 99, column: -3 }; } });
      return module.validate('badpos1', 'x');
    });
    expect(problems[0]).not.toHaveProperty('column');
    expect(problems[0]).not.toHaveProperty('endColumn');
  });
});

test.describe('round 12', () => {
  test('vanilla: value and update({ value }) see an Edit-as edit still waiting out the typing pause', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      const changes = [];
      const editor = new module.ConfigEditor(element, { value: '# c\nlisten: 1\n', editAs: 'json', onChange: (text) => changes.push(text) });
      await new Promise((resolve) => setTimeout(resolve, 50));
      const textarea = element.querySelector('textarea');
      const at = textarea.value.indexOf('1');
      textarea.focus();
      textarea.setSelectionRange(at, at + 1);
      document.execCommand('insertText', false, '7');
      await new Promise((resolve) => setTimeout(resolve, 0));
      const valueRightAway = editor.value;
      editor.update({ value: '# c\nlisten: 1\n' });
      await new Promise((resolve) => setTimeout(resolve, 1500));
      return { valueRightAway, final: editor.value, shown: textarea.value };
    });
    expect(result.valueRightAway).toBe('# c\nlisten: 7\n');
    expect(result.final).toBe('# c\nlisten: 1\n');
    expect(result.shown).toContain('1');
    expect(result.shown).not.toContain('7');
  });

  test('vanilla: colors must be CSS colors', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      const editor = new module.ConfigEditor(element, { value: 'a: 1\n' });
      const attempt = (color) => {
        try {
          editor.update({ colors: { key: color } });
          return 'ok';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      return ['#12345', 'redd', 'url(http://x/y)', 'red; x: y', '', '#ffcc66', 'var(--x)', 'rgb(1 2 3)'].map(attempt);
    });
    expect(result.slice(0, 5).every((message) => message.includes('is not a CSS color'))).toBe(true);
    expect(result.slice(5)).toEqual(['ok', 'ok', 'ok']);
  });
});

test.describe('ValueSync (internal to the bindings) without a framework', () => {
  test('a binding with a slow host never rolls back, and applies the host’s own value', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const { ValueSync } = await import('../config-editor/sync.js');
      const element = document.body.appendChild(document.createElement('div'));
      const sync = new ValueSync();
      let hostValue = 'a: 1\n';
      const editor = new module.ConfigEditor(element, {
        value: hostValue,
        // A slow host: each value comes back as the bound value 300 ms later.
        onChange: (text) => {
          sync.sent(text);
          setTimeout(() => {
            hostValue = text;
            const { apply, report } = sync.received(text, editor.value);
            if (apply !== null) editor.update({ value: apply });
            if (report !== null) hostValue = report;
          }, 300);
        },
      });
      const textarea = element.querySelector('textarea');
      textarea.focus();
      const shown = [];
      for (const letter of 'xyz') {
        textarea.setSelectionRange(textarea.value.length, textarea.value.length);
        document.execCommand('insertText', false, letter);
        await new Promise((resolve) => setTimeout(resolve, 100));
        shown.push(textarea.value);
      }
      await new Promise((resolve) => setTimeout(resolve, 800));
      const typed = editor.value;
      // The host's own value (a reset): applied.
      const { apply } = sync.received('reset: 1\n', editor.value);
      if (apply !== null) editor.update({ value: apply });
      await new Promise((resolve) => setTimeout(resolve, 50));
      return { shown, typed, host: hostValue, reset: editor.value };
    });
    expect(result.shown).toEqual(['a: 1\nx', 'a: 1\nxy', 'a: 1\nxyz']);
    expect(result.typed).toBe('a: 1\nxyz');
    expect(result.host).toBe('a: 1\nxyz');
    expect(result.reset).toBe('reset: 1\n');
  });
});

frameworkCases('react.html');
