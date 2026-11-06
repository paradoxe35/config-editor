import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The npm packages in js/: the vanilla `config-editor` (wasm-bindgen) and the
// `config-editor-react` wrapper, through their example pages. Build them first
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

for (const name of ['vanilla.html', 'react.html']) {
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

test.describe('react.html', () => {
  test('a colors prop overrides the theme', async ({ page: browserPage }) => {
    await open(browserPage, 'react.html');
    const style = await browserPage.locator('.config-editor').first().getAttribute('style');
    expect(style).toContain('--config-editor-key: #ffcc66');
  });

  test('onEditAsChange reports the switch', async ({ page: browserPage }) => {
    await open(browserPage, 'react.html');
    await browserPage.locator('#edit-as').selectOption('toml');
    await expect(browserPage.locator('#editing')).toHaveText('toml');
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
        theme: attempt(() => window.editor.setTheme(5)),
        value: attempt(() => window.editor.setValue(null)),
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
       try { const editor = new module.ConfigEditor(host, {}); editor.setReadOnly('yes'); editor.destroy(); readOnly = 'no error'; } catch (error) { readOnly = String(error.message); }
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

test.describe('react: parent updates and re-mounts', () => {
  test('the parent can revert to an earlier value', async ({ page: browserPage }) => {
    await open(browserPage, 'react.html');
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('a');
    await browserPage.keyboard.type('b');
    await expect(saved(browserPage)).toContainText('ab');
    await browserPage.locator('#revert').click();
    await expect(input(browserPage)).toHaveValue(/tunnel\.example\.com\na$/);
  });

  for (const mode of ['', '?uncontrolled']) {
    test(`a re-mount keeps the edited document ${mode || '(controlled)'}`, async ({ page: browserPage }) => {
      await browserPage.goto(page('react.html') + mode);
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).waitFor();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('limits: {}');
      await browserPage.locator('#rename').click();
      await expect(browserPage.locator('.config-editor__toolbar')).toContainText('renamed-burrow.yaml');
      await expect(input(browserPage)).toHaveValue(/limits: \{\}/);
      // The diff gutter still compares against the original text.
      await expect(browserPage.locator('.config-editor__diff-marker-added').first()).toBeVisible();
    });
  }

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
    expect(refused.separators).toBe('no error');
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

  test('setCallbacks and setColors check their arguments', async ({ page: browserPage }) => {
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
        callbackType: attempt(() => window.editor.setCallbacks({ onChange: 'x' })),
        callbacksNumber: attempt(() => window.editor.setCallbacks(5)),
        colorsNumber: attempt(() => window.editor.setColors(5)),
        colorsString: attempt(() => window.editor.setColors('red')),
        removal: attempt(() => window.editor.setCallbacks({ onChange: null })),
      };
    });
    expect(messages.callbackType).toContain('onChange must be a function or null');
    expect(messages.callbacksNumber).toContain('setCallbacks takes an object');
    expect(messages.colorsNumber).toContain('colors must be an object');
    expect(messages.colorsString).toContain('colors must be an object');
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
    await browserPage.evaluate(() => window.editor.setEditAs(null));
    await expect(saved(browserPage)).toContainText('listen: 78351');
    expect(await browserPage.evaluate(() => window.editor.value)).toContain('listen: 78351');
  });
});

test.describe('react: late parents and failed mounts', () => {
  test('a reset after a repeated value applies', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?late');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const before = await input(browserPage).inputValue();
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('x');
    await browserPage.keyboard.press('Backspace');
    await browserPage.keyboard.type('x');
    // The parent applies only the latest value, late.
    await expect(saved(browserPage)).toContainText('tunnel.example.com\nx');
    await browserPage.locator('#reset').click();
    await expect(input(browserPage)).toHaveValue(before);
  });

  test('ref.setValue resets to a text the user just passed through', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?late');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const before = await input(browserPage).inputValue();
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('k2: 2');
    await expect(saved(browserPage)).toContainText('k2: 2');
    for (let count = 0; count < 5; count += 1) await browserPage.keyboard.press('Backspace');
    await browserPage.keyboard.type('k3: 3');
    // Before the late parent applies it, discard back to the initial text.
    await browserPage.locator('#force-reset').click();
    await expect(input(browserPage)).toHaveValue(before);
    await browserPage.waitForTimeout(600);
    await expect(input(browserPage)).toHaveValue(before);
    await expect(saved(browserPage)).not.toContainText('k3');
  });

  test('more than 50 values in flight to a slow parent never roll the document back', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?slow');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const before = await input(browserPage).inputValue();
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.press('Enter');
    await browserPage.keyboard.type('# ' + 'a'.repeat(80), { delay: 5 });
    const typed = before + '\n# ' + 'a'.repeat(80);
    await expect(saved(browserPage)).toHaveText(typed, { timeout: 10000 });
    expect(await input(browserPage).inputValue()).toBe(typed);
  });

  test('fixing a typo against a slow parent never rolls the document back', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?slow');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const before = await input(browserPage).inputValue();
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.press('Enter');
    await browserPage.keyboard.type('# ab', { delay: 80 });
    await browserPage.keyboard.press('Backspace');
    await browserPage.keyboard.type('bc', { delay: 80 });
    // Keep typing while the echoes come back.
    for (let index = 0; index < 20; index += 1) {
      await browserPage.keyboard.type('d');
      await browserPage.waitForTimeout(100);
    }
    const typed = before + '\n# abc' + 'd'.repeat(20);
    expect(await input(browserPage).inputValue()).toBe(typed);
    await expect(saved(browserPage)).toHaveText(typed, { timeout: 5000 });
    expect(await input(browserPage).inputValue()).toBe(typed);
  });

  test('a value the user only passed through is applied once the parent sets it', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?late');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const before = await input(browserPage).inputValue();
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('x');
    await browserPage.keyboard.press('Backspace');
    // The late parent applies the unchanged text: no render, nothing echoed.
    await browserPage.waitForTimeout(600);
    // Later than the echo window, the parent sets the passed-through text.
    await browserPage.evaluate((text) => {
      const realNow = Date.now;
      Date.now = () => realNow() + 11000;
      window.setParentValue(text);
    }, before + 'x');
    await expect(input(browserPage)).toHaveValue(before + 'x');
  });

  test('fixing a prop that failed a re-mount brings the editor back', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'react.html');
    await browserPage.locator('#bad-theme').click();
    await browserPage.locator('#rename').click();
    await expect(input(browserPage)).toHaveCount(0);
    await browserPage.locator('#theme').selectOption('nord');
    await expect(input(browserPage)).toHaveCount(1);
    await expect(browserPage.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
    // The failures went to the console (the example has no onError).
    expect(errors.some((error) => error.includes('not-a-theme') || error.includes('theme'))).toBe(true);
  });
});

test.describe('vanilla API: setters right after construction', () => {
  test('setEditAs in the same task as the constructor switches the view', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.createElement('div');
      document.body.append(element);
      const reported = [];
      const editor = new module.ConfigEditor(element, { value: 'listen: 7835\n', onEditAsChange: (name) => reported.push(name) });
      editor.setEditAs('json');
      await new Promise((resolve) => setTimeout(resolve, 100));
      const shown = element.querySelector('textarea').value;
      const back = new module.ConfigEditor(document.body.appendChild(document.createElement('div')), { value: 'listen: 7835\n', editAs: 'json' });
      back.setEditAs(null);
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
      return [
        attempt({ label: 'IconLong', icon: 'ABCDEFG' }),
        attempt({ label: 'ExtString', extensions: 'env' }),
        attempt({ label: 'Fine', icon: 'Fi', extensions: ['fine'] }),
      ];
    });
    expect(errors[0]).toContain('icon');
    expect(errors[1]).toContain('extensions');
    expect(errors[2]).toBeNull();
  });
});

test.describe('vanilla API: misspelled keys are warned about', () => {
  test('setCallbacks and registerLanguage name the key they ignore, with a hint', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const warnings = [];
    browserPage.on('console', (message) => {
      if (message.type() === 'warning') warnings.push(message.text());
    });
    await browserPage.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const element = document.body.appendChild(document.createElement('div'));
      const editor = new module.ConfigEditor(element, { value: 'a: 1\n' });
      editor.setCallbacks({ onChnge: () => {} });
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

test.describe('react: round 5', () => {
  test('a re-mount while the parent is late keeps the newest document', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?late');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('debug: true');
    // Re-mount before the late parent catches up, then keep typing.
    await browserPage.locator('#rename').click();
    await expect(browserPage.locator('.config-editor__toolbar')).toContainText('renamed-');
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('\nfoo: 1');
    await browserPage.waitForTimeout(1500);
    await expect(input(browserPage)).toHaveValue(/debug: true\nfoo: 1$/);
    await expect(saved(browserPage)).toContainText('debug: true\nfoo: 1');
  });
});

test.describe('react: the ref while a re-mount has failed', () => {
  test('ref.value is the document, not the initial value', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html') + '?uncontrolled');
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('debug: true');
    await browserPage.locator('#bad-theme').click();
    await browserPage.locator('#rename').click();
    await expect(input(browserPage)).toHaveCount(0);
    const value = await browserPage.evaluate(() => window.editorHandle.current.value);
    expect(value).toContain('debug: true');
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
      for (let round = 0; round < 40; round += 1) {
        // As in the README quick start: no reference kept, no destroy().
        new module.ConfigEditor(host, { value });
      }
      await new Promise((resolve) => setTimeout(resolve, 300));
      return wasm.memory.buffer.byteLength / 1024 / 1024;
    });
    expect(megabytes).toBeLessThan(40);
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
        setter: attempt(() => window.editor.setEditAs('readonlylang')),
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
    expect(types).toContain('setReadOnly(readOnly');
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
