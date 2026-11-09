import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOT, ENV, HJSON, ODD, SCHEMA } from './custom-languages.mjs';
import { editor, pointAt } from './helpers.mjs';

// Languages registered from JavaScript get schema completion and hover from
// the same engine as the built-in ones: a flat `.env` language from its
// `separators` alone, a nested HJSON-like one from its `contextAt`,
// `insertKey` and `keyAt` callbacks; `multilineRanges` colors its strings
// and comments written over lines. On the vanilla page and the React one.
// Build the packages first with `npm run build` in js/.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(page, name) {
  page.errors = [];
  page.warnings = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
    if (message.type() === 'warning') page.warnings.push(message.text());
  });
  await page.goto(pageUrl(name));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
}

/**
 * Registers `language` (source text, see custom-languages.mjs) and mounts a
 * new editor with it on the vanilla page, its saves in `window.changes`.
 */
async function mount(page, language, value, options = '{}') {
  await page.evaluate(
    async ({ language, value, schema, options }) => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      // eslint-disable-next-line no-new-func
      const name = module.registerLanguage(new Function(`return ${language}`)());
      window.changes = [];
      window.editor = new module.ConfigEditor(document.getElementById('editor'), {
        language: name,
        value,
        schema,
        onChange: (text) => window.changes.push(text),
        // eslint-disable-next-line no-new-func
        ...new Function(`return ${options}`)(),
      });
    },
    { language, value, schema: SCHEMA, options },
  );
  await page.locator('.config-editor__input').waitFor();
}

/** Puts the caret at the end of the first `marker` in the document. */
async function caretAfter(page, marker) {
  await editor(page).input.evaluate((element, marker) => {
    element.focus();
    const at = element.value.indexOf(marker) + marker.length;
    element.setSelectionRange(at, at);
  }, marker);
}

async function hover(page, line, key) {
  const text = (await editor(page).value()).split('\n')[line];
  const at = await pointAt(page, line, text.indexOf(key) + 1);
  await page.mouse.move(at.x, at.y, { steps: 4 });
  return page.locator('.config-editor__hover');
}

const selection = (page) => editor(page).input.evaluate((element) => element.selectionStart);

test.describe('a flat JS language (separators only)', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'vanilla.html');
    await mount(page, ENV, '# Server\nPORT=80\n');
  });
  test.afterEach(async ({ page }) => expect(page.errors).toEqual([]));

  test('suggests the top-level keys not yet written, and inserts KEY=', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('P');
    await expect.poll(e.keys).toContain('PATH_PREFIX');
    expect(await e.keys()).not.toContain('PORT');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('LOG');
    await expect.poll(e.keys).toEqual(['LOG_LEVEL']);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('# Server\nPORT=80\nLOG_LEVEL=');
    expect(await selection(page)).toBe('# Server\nPORT=80\nLOG_LEVEL='.length);
  });

  test('suggests enum and boolean values after the separator, written bare', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('LOG_LEVEL=');
    await page.keyboard.press('Control+Space');
    await expect.poll(e.keys).toEqual(['debug', 'info', 'warn']);
    await page.keyboard.type('i');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('# Server\nPORT=80\nLOG_LEVEL=info');
    await page.keyboard.type('\nDEBUG=');
    await page.keyboard.press('Control+Space');
    await expect.poll(e.keys).toEqual(expect.arrayContaining(['true', 'false']));
    await e.clickKey('true');
    await expect(e.input).toHaveValue('# Server\nPORT=80\nLOG_LEVEL=info\nDEBUG=true');
    await expect.poll(() => page.evaluate(() => window.changes.at(-1))).toBe('# Server\nPORT=80\nLOG_LEVEL=info\nDEBUG=true');
  });

  test('nothing is suggested on a comment line', async ({ page }) => {
    const e = editor(page);
    await caretAfter(page, '# Server');
    await page.keyboard.type(' P');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
  });

  test('hovering a key shows its schema docs', async ({ page }) => {
    const card = await hover(page, 1, 'PORT');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('PORT');
    await expect(card).toContainText('The port to listen on');
  });
});

const NESTED = 'server: {\n  host: example.com\n  \n}\n';

test.describe('a nested JS language (contextAt, insertKey, keyAt)', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'vanilla.html');
    await mount(page, HJSON, NESTED);
  });
  test.afterEach(async ({ page }) => expect(page.errors).toEqual([]));

  test('suggests the keys of the object around the caret, not those written', async ({ page }) => {
    const e = editor(page);
    await caretAfter(page, 'example.com\n  ');
    await page.keyboard.type('h');
    await expect.poll(e.keys).toEqual(['hosts']);
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Control+Space');
    await expect.poll(e.keys).toEqual(expect.arrayContaining(['mode', 'tls', 'hosts']));
    expect(await e.keys()).not.toContain('host');
    expect(await e.keys()).not.toContain('LOG_LEVEL');
  });

  test('inserts a scalar key and offers its values', async ({ page }) => {
    const e = editor(page);
    await caretAfter(page, 'example.com\n  ');
    await page.keyboard.type('mo');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('server: {\n  host: example.com\n  mode: \n}\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(e.keys).toEqual(['fast', 'safe']);
    await e.clickKey('safe');
    await expect(e.input).toHaveValue('server: {\n  host: example.com\n  mode: "safe"\n}\n');
  });

  test('inserts an object key over lines, indented, the caret inside', async ({ page }) => {
    const e = editor(page);
    await caretAfter(page, 'example.com\n  ');
    await page.keyboard.type('tl');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('server: {\n  host: example.com\n  tls: {\n    \n  }\n}\n');
    await page.keyboard.type('en');
    await expect.poll(e.keys).toEqual(['enabled']);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('server: {\n  host: example.com\n  tls: {\n    enabled: \n  }\n}\n');
  });

  test('hovering a nested key shows the docs at its path', async ({ page }) => {
    const card = await hover(page, 1, 'host');
    await expect(card.locator('.config-editor__hover-key')).toHaveText('host');
    await expect(card).toContainText('Host name the server answers to');
    await page.mouse.move(0, 0);
    const outer = await hover(page, 0, 'server');
    await expect(outer).toContainText('The HTTP server');
  });
});

test.describe('callbacks that return the wrong shape or throw', () => {
  test('are warned about or logged, and read as null', async ({ page }) => {
    await open(page, 'vanilla.html');
    const broken = HJSON.replace(
      'contextAt(text, offset) {',
      'contextAt(text, offset) { if (text.includes("SHAPE")) return { path: 5, at: "key" }; if (text.includes("THROW")) throw new Error("boom");',
    );
    await mount(page, broken, 'SHAPE: 1\n');
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
    expect(page.warnings.filter((text) => text.includes("contextAt() must return"))).toHaveLength(1);
    await e.paste('THROW: 1\n');
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
    expect(page.errors.some((text) => text.includes("HJSON's contextAt() threw"))).toBe(true);
  });

  test('registerLanguage refuses a callback that is not a function', async ({ page }) => {
    await open(page, 'vanilla.html');
    const message = await page.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      try {
        module.registerLanguage({ label: 'Bad', parse: () => ({}), contextAt: 5 });
        return 'no error';
      } catch (error) {
        return String(error.message ?? error);
      }
    });
    expect(message).toBe('contextAt must be a function');
  });

  test('registerLanguage refuses syntax members of the wrong shape', async ({ page }) => {
    await open(page, 'vanilla.html');
    const messages = await page.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      const bad = [
        { mapBrackets: ['{{', '}}'] },
        { listBrackets: '[]' },
        { itemSeparator: ', ' },
        { itemMarker: '- ' },
        { literalQuotes: ['`'] },
        { separators: ['='], writtenSeparator: ': ' },
      ];
      return bad.map((members) => {
        try {
          module.registerLanguage({ label: 'Bad', parse: () => ({}), ...members });
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      });
    });
    expect(messages.map((message) => message.split(' ')[0])).toEqual([
      'mapBrackets', 'listBrackets', 'itemSeparator', 'itemMarker', 'literalQuotes', 'writtenSeparator',
    ]);
  });
});

const MULTILINE = "about: '''\n  name: not a key\n  # not a comment\n  ''' \nport: 1\n/* listen: 2\n   still: comment */ mode: fast\nhost: x\n";

test.describe('multilineRanges', () => {
  test.beforeEach(async ({ page }) => {
    await open(page, 'vanilla.html');
    await mount(page, HJSON, MULTILINE);
  });
  test.afterEach(async ({ page }) => expect(page.errors).toEqual([]));

  test('colors the lines of strings and comments written over lines, up to their end', async ({ page }) => {
    const rows = page.locator('.config-editor__highlight .config-editor__code-line');
    const kinds = (line) =>
      rows.nth(line).evaluate((row) =>
        [...row.querySelectorAll('span[class*="config-editor__"]')]
          .filter((span) => span.textContent.trim())
          .map((span) => [span.className.replace('config-editor__', ''), span.textContent.trim()]),
      );
    expect(await kinds(1)).toEqual([['string', 'name: not a key']]);
    expect(await kinds(2)).toEqual([['string', '# not a comment']]);
    expect(await kinds(3)).toEqual([['string', "'''"]]);
    expect(await kinds(4)).toContainEqual(['key', 'port']);
    expect(await kinds(6)).toEqual([['comment', 'still: comment */'], ['key', 'mode'], ['punctuation', ':'], ['value', 'fast']]);
    expect(await kinds(7)).toContainEqual(['key', 'host']);
  });

  test('suggests nothing inside them', async ({ page }) => {
    const e = editor(page);
    await caretAfter(page, 'not a key');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
    await caretAfter(page, '/* listen: 2');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('Edit-as with a custom host language', () => {
  test.afterEach(async ({ page }) => expect(page.errors).toEqual([]));

  const lastSave = (page) => page.evaluate(() => window.changes.at(-1));

  async function replaceIn(page, from, to) {
    await caretAfter(page, from);
    for (let index = 0; index < from.length; index += 1) await page.keyboard.press('Backspace');
    await page.keyboard.type(to);
  }

  test('a .env host keeps its comments and untouched lines', async ({ page }) => {
    await open(page, 'vanilla.html');
    await mount(page, ENV, '# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=info\n', "{ editAs: 'yaml' }");
    const e = editor(page);
    await expect(e.input).toHaveValue(/LOG_LEVEL: info/);
    await replaceIn(page, 'info', 'debug');
    await expect.poll(() => lastSave(page)).toBe('# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=debug\n');
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('PATH_PREFIX: api');
    await expect.poll(() => lastSave(page)).toBe('# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=debug\nPATH_PREFIX=api\n');
  });

  test('a nested host keeps its comments when edited as JSON', async ({ page }) => {
    await open(page, 'vanilla.html');
    const host = '# The server\nserver: {\n  host: example.com # public name\n  mode: fast\n}\n';
    await mount(page, HJSON, host, "{ editAs: 'json' }");
    const e = editor(page);
    await expect(e.input).toHaveValue(/"mode": "fast"/);
    await replaceIn(page, 'fast', 'safe');
    await expect.poll(() => lastSave(page)).toBe('# The server\nserver: {\n  host: example.com # public name\n  mode: "safe"\n}\n');
  });

  // A language with neither `keySpan` nor `valueSpan`: its saves still
  // change only the lines that differ.
  const SPANLESS = ENV.replace(/ {2}keySpan\(text, pointer\) \{[\s\S]*?\n {2}\},\n {2}valueSpan\(text, pointer\) \{[\s\S]*?\n {2}\},\n/, '')
    .replace('label: "ENV"', 'label: "Spanless"')
    .replace('extensions: ["env"]', 'extensions: ["spanless"]');
  const DOTENV = '# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=info\n';
  const NESTED_HOST = '# The server\nserver: {\n  host: example.com # public name\n  mode: fast\n}\nport: 1\n';
  // (name, language, host text, YAML text replaced → by, host text saved)
  const SAVES = [
    ['a .env value', ENV, DOTENV, ['LOG_LEVEL: info', 'LOG_LEVEL: debug'], '# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=debug\n'],
    ['a .env key removed', ENV, DOTENV, ['LOG_LEVEL: info\n', ''], '# Server\nPORT=80\n'],
    ['a .env key renamed', ENV, DOTENV, ['LOG_LEVEL:', 'LEVEL:'], '# Server\nPORT=80\n\n# Logging\nLEVEL=info\n'],
    ['a value, no spans', SPANLESS, DOTENV, ['LOG_LEVEL: info', 'LOG_LEVEL: debug'], '# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=debug\n'],
    ['a key added, no spans', SPANLESS, DOTENV, ['LOG_LEVEL: info\n', 'LOG_LEVEL: info\nX: y\n'], '# Server\nPORT=80\n\n# Logging\nLOG_LEVEL=info\nX=y\n'],
    ['a nested key added', HJSON, NESTED_HOST, ['  mode: fast\n', '  mode: fast\n  extra: 2\n'], '# The server\nserver: {\n  host: example.com # public name\n  mode: fast\n  extra: 2\n}\nport: 1\n'],
    ['an object added', HJSON, NESTED_HOST, ['port: 1\n', 'port: 1\ntls:\n  enabled: true\n'], '# The server\nserver: {\n  host: example.com # public name\n  mode: fast\n}\nport: 1\ntls: {\n  enabled: true\n}\n'],
    ['a nested key removed', HJSON, NESTED_HOST, ['  mode: fast\n', ''], '# The server\nserver: {\n  host: example.com # public name\n}\nport: 1\n'],
    ['a key renamed', HJSON, NESTED_HOST, ['port:', 'listen:'], '# The server\nserver: {\n  host: example.com # public name\n  mode: fast\n}\nlisten: 1\n'],
  ];
  // A language that declares its one-line maps and lists (`mapBrackets`,
  // `listBrackets`, `itemSeparator`): an edit inside one keeps the line as
  // the host wrote it, its spacing and its comment.
  const ODD_HOST = '; odd host\nports := (80 | 443) ; public\ntls := <cert := a.pem | days := 30> ; certificates\nname := web\n';
  const odd = (line, by) => ODD_HOST.replace(line, by);
  const ONE_LINE_SAVES = [
    ['an item of a one-line list changed', 'yaml', ['  - 443\n', '  - 8443\n'], odd('(80 | 443)', '(80 | 8443)')],
    ['an item added to a one-line list', 'yaml', ['  - 443\n', '  - 443\n  - 8080\n'], odd('(80 | 443)', '(80 | 443 | 8080)')],
    ['an entry added to a one-line map', 'yaml', ['  days: 30\n', '  days: 30\n  key: k.pem\n'], odd('days := 30>', 'days := 30 | key := k.pem>')],
    ['an item of a one-line list changed as JSON', 'json', ['443', '8443'], odd('(80 | 443)', '(80 | 8443)')],
  ];
  for (const [name, view, [from, to], saved] of ONE_LINE_SAVES) {
    test(`a language declaring its brackets: ${name} keeps the line`, async ({ page }) => {
      await open(page, 'vanilla.html');
      await mount(page, ODD, ODD_HOST, `{ editAs: '${view}' }`);
      const e = editor(page);
      await expect(e.input).toHaveValue(new RegExp(from.trim()));
      await e.input.evaluate((element, [from, to]) => {
        const at = element.value.indexOf(from);
        element.focus();
        element.setRangeText(to, at, at + from.length, 'end');
        element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: to }));
      }, [from, to]);
      await expect.poll(() => lastSave(page)).toBe(saved);
    });
  }

  // A language with no spans that declares its list items (`itemMarker`)
  // and its literal quotes; without the marker its items cannot be told
  // apart, and a reorder waits rather than lose their comments.
  const DOT_HOST = "name: web  # the name\nobj: <x: 1; y: 'two'>  # a map\nitems:\n  # first item\n  * <n: 1>\n  # second item\n  * <n: 2>  # two\nlast: true\n";
  const DOT_BARE = DOT.replace('itemMarker: "*",', '').replace('label: "Dot"', 'label: "DotBare"').replace('extensions: ["dot"]', 'extensions: ["dotbare"]');
  const ITEMS = '"items": [\n    {\n      "n": 1\n    },\n    {\n      "n": 2\n    }\n  ]';
  const SWAPPED = '"items": [\n    {\n      "n": 2\n    },\n    {\n      "n": 1\n    }\n  ]';
  const dot = (line, by) => DOT_HOST.replace(line, by);
  const DOT_SAVES = [
    ['list items reordered keep their comments', ITEMS, SWAPPED, dot('  # first item\n  * <n: 1>\n  # second item\n  * <n: 2>  # two\n', '  # second item\n  * <n: 2>  # two\n  # first item\n  * <n: 1>\n')],
    ['a string in a one-line map keeps its quotes', '"y": "two"', '"y": "three"', dot("y: 'two'", "y: 'three'")],
    ['a value keeps the comment after it', '"name": "web"', '"name": "api"', dot('name: web  #', 'name: api  #')],
  ];
  async function paste(page, from, to) {
    await editor(page).input.evaluate((element, [from, to]) => {
      const at = element.value.indexOf(from);
      element.focus();
      element.setRangeText(to, at, at + from.length, 'end');
      element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: to }));
    }, [from, to]);
  }
  for (const [name, from, to, saved] of DOT_SAVES) {
    test(`a language declared without spans, edited as JSON: ${name}`, async ({ page }) => {
      await open(page, 'vanilla.html');
      await mount(page, DOT, DOT_HOST, "{ editAs: 'json' }");
      await expect.poll(() => editor(page).input.inputValue()).toContain(from);
      await paste(page, from, to);
      await expect.poll(() => lastSave(page)).toBe(saved);
    });
  }
  test('a language declaring no item marker: a reorder waits, its comments kept', async ({ page }) => {
    await open(page, 'vanilla.html');
    await mount(page, DOT_BARE, DOT_HOST, "{ editAs: 'json' }");
    await expect(editor(page).input).toHaveValue(/"n": 2/);
    await paste(page, ITEMS, SWAPPED);
    await expect
      .poll(() =>
        page
          .locator('.config-editor__diagnostic-marker')
          .evaluateAll((markers) => markers.map((marker) => marker.dataset.message).join('\n')),
      )
      .toContain('cannot be moved with their comments');
    expect(await page.evaluate(() => window.changes.filter((text) => !text.includes('# first item')))).toEqual([]);
  });

  for (const [name, language, host, [from, to], saved] of SAVES) {
    test(`edited as YAML, ${name} changes only its lines`, async ({ page }) => {
      await open(page, 'vanilla.html');
      await mount(page, language, host, "{ editAs: 'yaml' }");
      const e = editor(page);
      await expect(e.input).toHaveValue(new RegExp(from.trim()));
      await e.input.evaluate((element, [from, to]) => {
        const at = element.value.indexOf(from);
        element.focus();
        element.setRangeText(to, at, at + from.length, 'end');
        element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: to }));
      }, [from, to]);
      await expect.poll(() => lastSave(page)).toBe(saved);
    });
  }
});

test.describe('react.html: a registered nested language', () => {
  test('gets suggestions and hover docs in the component', async ({ page }) => {
    await open(page, 'react.html');
    await page.evaluate(
      ({ language, value, schema }) => {
        // eslint-disable-next-line no-new-func
        window.coreApi.registerLanguage(new Function(`return ${language}`)());
        window.setParentValue(value);
        window.setExtraProps({ language: 'hjson', schema });
      },
      { language: HJSON, value: NESTED, schema: SCHEMA },
    );
    const e = editor(page);
    await expect(e.input).toHaveValue(NESTED);
    await caretAfter(page, 'example.com\n  ');
    await page.keyboard.type('tl');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('server: {\n  host: example.com\n  tls: {\n    \n  }\n}\n');
    await expect(page.locator('#saved')).toContainText('tls: {');
    await page.keyboard.press('Escape');
    const card = await hover(page, 1, 'host');
    await expect(card).toContainText('Host name the server answers to');
    expect(page.errors).toEqual([]);
  });
});
