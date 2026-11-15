import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Switching Edit as: undo and redo across a switch, folds, host updates and values the view cannot
// hold, the picker's state, documents edited as another language.

async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

const saved = (page) => page.locator('.demo__output-text').textContent();

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

test.describe('edit as JSON, back to the host language during the typing pause', () => {
  setupEditor(test, '?language=yaml&edit=json');

  test('saves the edit', async ({ page }) => {
    const e = editor(page);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835') + 4;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('1');
    await page.locator('.demo__edit-select').selectOption('');
    await expect.poll(() => page.locator('.demo__output-text').textContent()).toContain('listen: 78351');
    await expect(e.input).toHaveValue(/listen: 78351/);
  });
});

test.describe('undo in Edit-as', () => {
  setupEditor(test, '?language=toml');

  test('undoing a deleted block restores the host with its comment', async ({ page }) => {
    const e = editor(page);
    const host = 'listen = 1\n\n# beta client\n[clients.beta]\nsecret = "y"\nmax_connections = 3\n';
    await e.paste(host);
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/beta:/);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('clients:');
      element.focus();
      element.setSelectionRange(start, element.value.length);
    });
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).not.toContain('beta');
    await page.keyboard.press('Control+z');
    await expect.poll(() => saved(page)).toBe(host);
  });
});

// Twelfth-round findings, replayed with real keys. The classes behind them
// are covered by the unit tests (`editor/tests/save/`, `editing_matrix`).
test.describe('toml', () => {
  setupEditor(test, '?language=toml');

  test('redo restores a paste after a refused Edit-as switch waited through the undo', async ({ page }) => {
    const e = editor(page);
    const sample = await e.value();
    await e.paste('[clients."acmé✓"]\nsecret = "s"\n');
    const pasted = await e.value();
    await page.locator('.demo__edit-select').selectOption('xml');
    await page.waitForTimeout(800);
    await e.input.click();
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(sample);
    await page.waitForTimeout(800);
    await page.keyboard.press('Control+Shift+z');
    await expect.poll(() => e.value()).toBe(pasted);
  });
});

test.describe('the vanilla example', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');
  test('the Edit as select follows undo back across a switch', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('#edit-as').selectOption('json');
    await expect(page.locator('body')).toHaveAttribute('data-editing', 'json');
    await page.locator('.config-editor__input').focus();
    await page.keyboard.press('Control+z');
    await expect(page.locator('body')).toHaveAttribute('data-editing', 'yaml');
    await expect(page.locator('#edit-as')).toHaveValue('');
  });
});

test.describe('a host update while editing as another language', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const editAs of ['json', 'toml', 'hcl', 'xml']) {
    test(`${editAs}: an update in the constructor's task sends no onChange`, async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async (editAs) => {
        const changes = [];
        const host = document.createElement('div');
        document.body.append(host);
        const editor = new window.module.ConfigEditor(host, { value: '# c\na: 1\n', editAs, onChange: (text) => changes.push(text) });
        editor.update({ value: '# c2\na: 5\nb: x\n' });
        await new Promise((resolve) => setTimeout(resolve, 700));
        const out = { changes, value: editor.value };
        editor.destroy();
        return out;
      }, editAs);
      expect(result).toEqual({ changes: [], value: '# c2\na: 5\nb: x\n' });
    });
  }
});

test.describe('host API details', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('vanilla example: the Edit-as select shows the language on screen', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    // TOML has no null: the switch waits, the YAML stays on screen.
    await page.evaluate(() => window.editor.update({ value: 'a: null\n' }));
    await page.selectOption('#edit-as', 'toml');
    await expect(page.locator('#edit-as')).toHaveValue('');
    await page.selectOption('#edit-as', 'json');
    await expect(page.locator('#edit-as')).toHaveValue('json');
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (options) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input').first();

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(example(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
      timeout: 20000,
    });
    await input(page).waitFor();
    return errors;
  }

  /** Selects `needle` (its `nth` occurrence) in the textarea. */
  async function select(page, needle, nth = 0) {
    await input(page).evaluate(
      (area, [needle, nth]) => {
        let at = -1;
        for (let index = 0; index <= nth; index++) at = area.value.indexOf(needle, at + 1);
        if (at < 0) throw new Error(`no ${needle}`);
        area.focus();
        area.setSelectionRange(at, at + needle.length);
        area.dispatchEvent(new Event('select', { bubbles: true }));
      },
      [needle, nth],
    );
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test('folds stay closed through an Edit-as switch and back', async ({ page }) => {
    const value = 'a: 1\nweb:\n  http: 80\n  hosts:\n    - x\n    - y\nz: 2\n';
    const errors = await open(page, { language: 'yaml', value });
    await select(page, 'web');
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(input(page)).not.toHaveValue(/http/);
    await page.evaluate(() => window.editor.update({ editAs: 'json' }));
    await expect(input(page)).toHaveValue(/"web": \{/);
    await expect(input(page)).toHaveValue(/"z": 2/);
    await expect(input(page)).not.toHaveValue(/http/);
    await page.evaluate(() => window.editor.update({ editAs: 'toml' }));
    await expect(input(page)).toHaveValue(/\[web\]/);
    await expect(input(page)).not.toHaveValue(/http/);
    await page.evaluate(() => window.editor.update({ editAs: 'yaml' }));
    await expect(input(page)).toHaveValue(/web:/);
    await expect(input(page)).not.toHaveValue(/http/);
    expect(errors).toEqual([]);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 26 (host API). A new host value the language on screen cannot
  // hold is edited in the host's language until it converts (every host and
  // editAs pair, an editor and a documents view, one and two documents per
  // update); an index is found only on a list; a custom language's span
  // callback returning nothing falls back to the flat default; undoing a
  // switch sends nothing new; framework handles ask for editAs again; the
  // documents list is checked as an editor's options are; a state callback
  // added later hears the state at once; the READMEs list every export; and
  // the caret in a block-scalar key is on that key.
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

  const LABELS = { yaml: 'YAML', json: 'JSON', toml: 'TOML', hcl: 'HCL', xml: 'XML' };

  const SAMPLES = { yaml: 'a: 1\n', json: '{\n  "a": 1\n}\n', toml: 'a = 1\n', hcl: 'a = 1\n', xml: '<a>1</a>\n' };

  // Host texts some other language cannot hold: one that does not parse, then
  // values only some languages lack (the test keeps those the pair lacks).
  const UNHELD = {
    yaml: ['a: [\n', 'a: 1\nport: ~\n', 'a: 1\n"x/y": 1\n'],
    json: ['{\n  "a": \n', '{\n  "a": 1,\n  "port": null\n}\n', '{\n  "x/y": 1\n}\n'],
    toml: ['a = \n', '"x/y" = 1\n'],
    hcl: ['a = \n', 'a = 1\nport = null\n'],
    xml: ['<a>\n', '<a>1<b>2</b></a>\n'],
  };


  test.describe('host API', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');
    test('a host value the view cannot hold is edited in the host language until it converts (ConfigEditor, every pair)', async ({ page }) => {
      test.slow();
      await open(page);
      const failures = await page.evaluate(
        async ([LANGUAGES, LABELS, SAMPLES, UNHELD]) => {
          const failures = [];
          for (const host of LANGUAGES) {
            for (const edit of LANGUAGES.filter((language) => language !== host)) {
              for (const text of UNHELD[host]) {
                try {
                  window.tools.convert(host, text, edit);
                  continue;
                } catch {
                  // This pair cannot hold it: the case.
                }
                const which = `${host} as ${edit}, the host sends ${JSON.stringify(text)}`;
                const element = window.mountPoint();
                const events = [];
                const editor = new window.module.ConfigEditor(element, {
                  language: host,
                  value: SAMPLES[host],
                  editAs: edit,
                  showToolbar: true,
                  onChange: (value) => events.push(['change', value]),
                  onEditAsChange: (language) => events.push(['language', language]),
                  onSyncChange: (inSync) => events.push(['sync', inSync]),
                });
                await window.pause(50);
                events.length = 0;
                editor.update({ value: text });
                await window.pause(50);
                const shown = element.querySelector('.config-editor__language').textContent;
                if (editor.text !== text || shown !== LABELS[host]) failures.push(`${which}: shows ${shown} ${JSON.stringify(editor.text)}`);
                if (JSON.stringify(events) !== JSON.stringify([['language', host]])) failures.push(`${which}: heard ${JSON.stringify(events)}`);
                events.length = 0;
                // Fixed by typing: the host hears it, then the switch is made.
                window.typeAll(element, SAMPLES[host]);
                await window.pause(400);
                const heard = JSON.stringify(events);
                const expected = JSON.stringify([['change', SAMPLES[host]], ['language', edit]]);
                if (heard !== expected) failures.push(`${which}: after the fix heard ${heard}`);
                if (element.querySelector('.config-editor__language').textContent !== LABELS[edit]) failures.push(`${which}: not switched after the fix`);
                editor.destroy();
                element.remove();
              }
            }
          }
          return failures;
        },
        [LANGUAGES, LABELS, SAMPLES, UNHELD],
      );
      expect(failures).toEqual([]);
    });
    test('a host value the view cannot hold waits in a documents view, one or two documents per update', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const element = window.mountPoint();
        const events = [];
        let files = [
          { id: 'a', fileName: 'a.yaml', value: 'name: x\nport: 1\n', editAs: 'toml' },
          { id: 'b', fileName: 'b.json', value: '{\n  "x": 1\n}\n', editAs: 'xml' },
        ];
        const view = new window.module.ConfigDocuments(element, {
          documents: files,
          onChange: (id, value) => {
            events.push(['change', id, value]);
            files = files.map((file) => (file.id === id ? { ...file, value } : file));
            view.update({ documents: files });
          },
          onEditAsChange: (id, language) => events.push(['language', id, language]),
          onSyncChange: (id, inSync) => events.push(['sync', id, inSync]),
        });
        await window.pause(100);
        events.length = 0;
        files = files.map((file) => (file.id === 'a' ? { ...file, value: 'name: x\nport: ~\n' } : file));
        view.update({ documents: files });
        await window.pause(100);
        const one = { events: [...events], text: view.editor('a').text };
        events.length = 0;
        window.typeAll(element, 'name: x\nport: 2\n');
        await window.pause(400);
        const fixed = { events: [...events], text: view.editor('a').text };
        events.length = 0;
        // Both documents at once, `b` shown.
        view.select('b');
        await window.pause(100);
        events.length = 0;
        files = files.map((file) => ({ ...file, value: file.id === 'a' ? 'name: q\nport: null\n' : '{\n  "x/y": 1\n}\n' }));
        view.update({ documents: files });
        await window.pause(100);
        const two = { events: [...events], a: view.editor('a').text, b: view.editor('b').text };
        return { one, fixed, two };
      });
      expect(result.one.events).toEqual([['language', 'a', 'yaml']]);
      expect(result.one.text).toBe('name: x\nport: ~\n');
      expect(result.fixed.events).toEqual([
        ['change', 'a', 'name: x\nport: 2\n'],
        ['language', 'a', 'toml'],
      ]);
      expect(result.fixed.text).toBe('name = "x"\nport = 2\n');
      expect(result.two.events).toEqual(expect.arrayContaining([['language', 'b', 'json']]));
      expect(result.two.events.filter(([kind]) => kind !== 'language')).toEqual([]);
      expect(result.two.b).toBe('{\n  "x/y": 1\n}\n');
      expect(result.two.a === 'name: q\nport: null\n' || result.two.a === null || result.two.a === '').toBe(true);
      expect(page.errors).toEqual([]);
    });
    test('asking for the host language while such a value waits ends the wait', async ({ page }) => {
      await open(page);
      const result = await page.evaluate(async () => {
        const element = window.mountPoint();
        const events = [];
        const editor = new window.module.ConfigEditor(element, {
          value: 'a: 1\n',
          editAs: 'toml',
          onEditAsChange: (language) => events.push(language),
          onProblems: (problems) => events.push(problems.map((problem) => problem.message)),
        });
        await window.pause(100);
        editor.update({ value: 'a: ~\n' });
        await window.pause(400);
        const waiting = events.flat().filter((message) => message.startsWith('Cannot switch to TOML yet'));
        editor.update({ editAs: null });
        await window.pause(400);
        return { waiting: waiting.length > 0, problems: events.at(-1), text: editor.text };
      });
      expect(result.waiting).toBe(true);
      expect(result.problems).toEqual([]);
      expect(result.text).toBe('a: ~\n');
    });
    test('undoing a switch back to the host text sends no onChange', async ({ page }) => {
      await open(page);
      const element = await page.evaluateHandle(() => {
        const element = window.mountPoint();
        window.heard = [];
        window.target = new window.module.ConfigEditor(element, {
          value: '# top\nname: web\n',
          onChange: (value) => window.heard.push(value),
          onEditAsChange: (language) => window.heard.push(`language ${language}`),
        });
        return element;
      });
      await page.evaluate(() => window.pause(100));
      await page.evaluate(() => window.target.update({ editAs: 'toml' }));
      await (await element.$('.config-editor__input')).focus();
      await page.keyboard.press('Control+z');
      await page.evaluate(() => window.pause(400));
      await page.keyboard.press('Control+y');
      await page.keyboard.press('Control+z');
      await page.evaluate(() => window.pause(400));
      expect(await page.evaluate(() => window.heard)).toEqual(['language yaml', 'language toml', 'language yaml', 'language toml', 'language yaml']);
    });
    test("a component's handle asks for editAs again after an undo across the switch", async ({ page }) => {
      await open(page);
      const element = await page.evaluateHandle(() => {
        const element = window.mountPoint();
        window.languages = [];
        window.binding = new window.bindings.EditorBinding('qa26');
        window.props = { value: 'name: web\n', onEditAsChange: (language) => window.languages.push(language) };
        window.binding.attach(element, window.props);
        return element;
      });
      await page.evaluate(async () => {
        await window.pause(150);
        window.props = { ...window.props, editAs: 'toml' };
        window.binding.update(window.props);
        await window.pause(100);
      });
      await (await element.$('.config-editor__input')).focus();
      await page.keyboard.press('Control+z');
      await page.evaluate(() => window.pause(100));
      // The same prop again is not sent; the handle asks.
      const asked = await page.evaluate(async () => {
        window.binding.update({ ...window.props });
        await window.pause(100);
        const before = [...window.languages];
        const answer = window.binding.handle.editAs('toml');
        await window.pause(100);
        return { before, answer, after: window.languages, text: window.binding.handle.text };
      });
      expect(asked.before).toEqual(['yaml', 'toml', 'yaml']);
      expect(asked.answer).toBe(true);
      expect(asked.after).toEqual(['yaml', 'toml', 'yaml', 'toml']);
      expect(asked.text).toBe('name = "web"\n');
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
    page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
    page.on('pageerror', (error) => page.errors.push(String(error)));
  }

  async function open(page, name, query = '') {
    listen(page);
    await page.goto(example(name) + query);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').first().waitFor();
  }

  /** Mounts a documents view of `documents` with `echoes: true` in the
   * vanilla page, its `onChange` texts kept in `window.changes`. */
  async function mountDocuments(page, documents, options = {}) {
    await open(page, 'documents.html');
    await page.evaluate(
      async ([documents, options]) => {
        window.documentsView.destroy();
        const module = await import('../config-editor/index.js');
        const host = document.createElement('div');
        host.id = 'probe';
        host.style.height = options.height ?? '420px';
        document.body.prepend(host);
        window.changes = [];
        window.view = new module.ConfigDocuments(host, {
          documents,
          echoes: true,
          onChange: (id, value) => window.changes.push({ id, value }),
          onSave: () => {},
        });
      },
      [documents, options],
    );
  }


  // --- replaceValue keeps another document's Edit-as (L30-4). ------------------

  test.describe('ConfigDocuments: a document edited as another language through the handle', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    for (const call of ['replaceValue of another document', 'a settings update']) {
      test(`stays edited as it after ${call}`, async ({ page }) => {
        // `a` has settings of its own: a view's settings update lists the
        // documents again with them.
        await mountDocuments(page, [
          { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings: { showFooter: false } },
          { id: 'b', fileName: 'b.yaml', value: '# keep\nb: 2\n' },
          { id: 'c', fileName: 'c.yaml', value: 'c: 3\n' },
        ]);
        const onScreen = () => page.evaluate(() => window.view.editor('b').text);
        await page.evaluate(() => {
          window.view.select('b');
          window.view.editAs('b', 'json');
        });
        await expect.poll(onScreen).toBe('{\n  "b": 2\n}\n');
        await page.evaluate((call) => {
          if (call.startsWith('replaceValue')) window.view.replaceValue('c', 'c: 9\n');
          else window.view.update({ showFooter: true });
        }, call);
        await page.waitForTimeout(300);
        expect(await onScreen()).toBe('{\n  "b": 2\n}\n');
        expect(await page.evaluate(() => window.view.editor('b').value)).toBe('# keep\nb: 2\n');
        expect(page.errors).toEqual([]);
      });
    }
  });
});
