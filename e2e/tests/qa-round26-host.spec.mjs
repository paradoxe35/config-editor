import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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

test.describe('QA round 26: host API', () => {
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

  test('an index is found only on a list, in every language', async ({ page }) => {
    await open(page);
    const found = await page.evaluate(() => {
      const value = { name: 'web', o: { k: 1 }, l: [{ a: 1 }, { a: 2 }] };
      const out = {};
      for (const language of ['yaml', 'json', 'toml', 'hcl', 'xml']) {
        const prefix = language === 'xml' ? '/root' : '';
        const text = window.tools.serialize(language, language === 'xml' ? { root: value } : value);
        out[language] = Object.fromEntries(
          ['/name/0', '/o/0/0', '/o/k/0/0', '/l/0', '/l/0/0', '/l/1/0', '/l/1/a', '/0'].map((pointer) => {
            const at = pointer === '/0' ? pointer : `${prefix}${pointer}`;
            return [pointer, window.tools.rangeOf(language, text, at) !== null];
          }),
        );
      }
      return out;
    });
    for (const language of LANGUAGES) {
      // XML reads an element written once as a one-item list too.
      const lone = language === 'xml';
      expect(found[language], language).toEqual({
        '/name/0': lone,
        '/o/0/0': false,
        '/o/k/0/0': false,
        '/l/0': true,
        '/l/0/0': false,
        '/l/1/0': false,
        '/l/1/a': true,
        '/0': false,
      });
    }
  });

  test('a custom language span callback returning null or undefined falls back to the flat default', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(() => {
      const parse = (text) =>
        Object.fromEntries(
          text
            .split('\n')
            .filter((line) => line.includes('='))
            .map((line) => line.split('=')),
        );
      const out = {};
      for (const [label, given] of [
        ['Stub26u', () => undefined],
        ['Stub26n', () => null],
        ['Stub26e', () => {}],
      ]) {
        window.module.registerLanguage({ label, separators: ['='], parse, keySpan: given, valueSpan: given });
        const name = label.toLowerCase();
        const text = '# c\nlisten=x\nname=web\n';
        const range = window.tools.rangeOf(name, text, '/name');
        const problems = window.tools.validate(name, text, JSON.stringify({ properties: { listen: { type: 'integer' } } }));
        out[label] = { range: range && text.slice(range.start, range.end), line: problems[0]?.line };
      }
      // A span given is used as it is.
      window.module.registerLanguage({
        label: 'Given26',
        separators: ['='],
        parse,
        keySpan: () => ({ line: 1, startColumn: 1, endColumn: 2 }),
      });
      const given = window.tools.rangeOf('given26', 'a=1\nb=2\n', '/b');
      out.given = given && [given.startLine, given.startColumn];
      return out;
    });
    for (const label of ['Stub26u', 'Stub26n', 'Stub26e']) {
      expect(result[label], label).toEqual({ range: 'name=web', line: 2 });
    }
    expect(result.given).toEqual([1, 1]);
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

  test('the documents list is checked as an editor is', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const attempt = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const element = window.mountPoint();
      const documents = (extra) => [{ id: 'a', fileName: 'a.yaml', value: 'k: 1\n' }, { id: 'b', fileName: 'b.yaml', value: 'm: 2\n', ...extra }];
      const out = {
        nullValue: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ value: null }) })),
        unknownActive: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents(), activeId: 'zz' })),
        secrets: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ secrets: 5 }) })),
        secretsCheck: attempt(() => new window.module.ConfigDocuments(window.mountPoint(), { documents: documents({ secrets: { isPlaceholder: 5 } }) })),
      };
      const view = new window.module.ConfigDocuments(element, { documents: documents(), activeId: 'b' });
      await window.pause(100);
      out.updateNull = attempt(() => view.update({ documents: documents({ value: null }) }));
      const [first, second] = documents();
      delete second.value;
      out.leftOut = attempt(() => view.update({ documents: [first, second] }));
      await window.pause(100);
      out.kept = view.editor('b').value;
      out.ignored = attempt(() => view.update({ documents: documents({ editAsText: 'm = 2\n' }) }));
      return out;
    });
    expect(result.nullValue).toContain('documents[1].value must be a string');
    expect(result.unknownActive).toContain('activeId: no document has the id "zz"');
    expect(result.secrets).toContain('documents[1].secrets must be an object');
    expect(result.secretsCheck).toContain('documents[1].secrets.isPlaceholder must be a function');
    expect(result.updateNull).toContain('documents[1].value must be a string');
    expect(result.leftOut).toBe('no error');
    expect(result.kept).toBe('m: 2\n');
    expect(result.ignored).toBe('no error');
    expect(page.warnings.filter((text) => text.includes('documents[1].editAsText is ignored'))).toHaveLength(1);
  });

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

  test('the caret in a block-scalar key is on that key', async ({ page }) => {
    await open(page);
    const paths = await page.evaluate(() => {
      const text = '? |\n  multi\n  line\n: v\n';
      return ['multi', 'line', ': v'].map((at) => window.tools.pathAt('yaml', text, text.indexOf(at) + 1)?.pointer);
    });
    expect(paths).toEqual(['/multi\nline\n', '/multi\nline\n', '/multi\nline\n']);
  });
});

test('the framework READMEs list every type and tool their packages export', () => {
  const exported = (path) => {
    const source = readFileSync(join(js, path), 'utf8');
    return [...source.matchAll(/export (?:type )?\{([^}]*)\}/g)]
      .flatMap((match) => match[1].split(','))
      .map((name) => name.trim().replace(/^type /, '').split(' as ').pop().trim())
      .filter(Boolean);
  };
  const tools = exported('config-editor/tools.js').filter((name) => name !== 'loadConfigEditor');
  const missing = [];
  for (const [name, entry] of [
    ['config-editor-react', 'src/index.tsx'],
    ['config-editor-vue', 'src/index.ts'],
    ['config-editor-svelte', 'src/index.ts'],
  ]) {
    const readme = readFileSync(join(js, name, 'README.md'), 'utf8');
    const start = readme.indexOf('## Exports');
    const end = readme.indexOf('\n## ', start + 1);
    const section = readme.slice(start, end < 0 ? undefined : end);
    const listed = new Set([...section.matchAll(/`([A-Za-z]+)`/g)].map((match) => match[1]));
    for (const item of [...exported(`${name}/${entry}`), ...tools]) {
      if (!listed.has(item)) missing.push(`${name}: ${item}`);
    }
  }
  expect(missing).toEqual([]);
});
