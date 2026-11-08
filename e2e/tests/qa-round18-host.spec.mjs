import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 18 (host): the rule for a host text that does not parse holds
// both ways. A read-only value never reaches the host changed, also in a
// text of hundreds of lines broken in more places than the editor can mend
// to read it; and a repair that keeps every read-only value goes through:
// closing a block left open that holds a lock, a lock on a path the text
// never had, the repairs of a large text.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const schema = readFileSync(join(root, 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'protected.schema.json'), 'utf8');

const input = (page) => page.locator('#qa .config-editor__input');
const value = (page) => input(page).inputValue();

async function caretAt(page, marker, offset = 0) {
  const text = await value(page);
  const at = text.indexOf(marker);
  expect(at, `${JSON.stringify(marker)} in the text`).toBeGreaterThanOrEqual(0);
  await input(page).evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, at + offset);
}

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

/** Every text sent to the host that parses, parsed. */
async function sentDocuments(page, language) {
  return page.evaluate(async (language) => {
    const { parse } = await import('../config-editor/tools.js');
    return window.sent.flatMap((text) => {
      try {
        return [parse(language, text)];
      } catch {
        return [];
      }
    });
  }, language);
}

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
}

// A large document per language: `id` and `limits` read-only by the schema,
// filler lines, five of them broken. Each break's repair types `fix` at
// `offset` units into its marker (the break's `index`).
const LARGE = {
  yaml: {
    max: ['max: 10', 6],
    head: 'id: srv-42\nlimits:\n  max: 10\n',
    tail: '',
    line: (index) => `k${index}: ${index}`,
    broken: (index) => `k${index}: "${index}" y`,
    fix: (index) => [`k${index}: "${index}"`, ' #'],
  },
  json: {
    max: ['"max": 10', 8],
    head: '{\n  "id": "srv-42",\n  "limits": {\n    "max": 10\n  },\n',
    tail: '  "z": 0\n}\n',
    line: (index) => `  "k${index}": ${index},`,
    broken: (index) => `  "k${index}" ${index},`,
    fix: (index) => [`"k${index}"`, ':'],
  },
  toml: {
    max: ['max = 10', 7],
    head: 'id = "srv-42"\n',
    tail: '[limits]\nmax = 10\n',
    line: (index) => `k${index} = ${index}`,
    broken: (index) => `k${index} = `,
    fix: (index) => [`k${index} = `, '1'],
  },
  hcl: {
    max: ['max = 10', 7],
    head: 'id = "srv-42"\nlimits {\n  max = 10\n}\n',
    tail: '',
    line: (index) => `k${index} = ${index}`,
    broken: (index) => `k${index} = `,
    fix: (index) => [`k${index} = `, '1'],
  },
  xml: {
    max: ['<max>10', 6],
    head: '<id>srv-42</id>\n<limits>\n  <max>10</max>\n</limits>\n',
    tail: '',
    line: (index) => `<k${index}>${index}</k${index}>`,
    broken: (index) => `<k${index}>${index}</k${index}`,
    fix: (index) => [`</k${index}`, '>'],
  },
};
const BROKEN = [40, 160, 300, 450, 580];

function largeText({ head, tail, line, broken }) {
  let text = head;
  for (let index = 0; index < 600; index += 1) text += `${BROKEN.includes(index) ? broken(index) : line(index)}\n`;
  return text + tail;
}

test.describe('a large host text broken in five places', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [language, document] of Object.entries(LARGE)) {
    for (const order of ['edit the locked values first', 'repair first']) {
      test(`${language}: ${order}`, async ({ page }) => {
        await open(page);
        await mount(page, { language, value: largeText(document), schema: true });
        const editLocked = async () => {
          for (const [marker, offset] of [['srv-42', 3], document.max]) {
            await caretAt(page, marker, offset);
            await page.keyboard.type('9');
          }
        };
        if (order === 'edit the locked values first') await editLocked();
        for (const index of BROKEN) {
          const before = await value(page);
          const [marker, typed] = document.fix(index);
          await caretAt(page, marker, marker.length);
          await page.keyboard.insertText(typed);
          await expect.poll(() => value(page), `repairing break ${index}`).not.toBe(before);
        }
        if (order === 'repair first') await editLocked();
        await page.evaluate(() => window.qa.save());
        const documents = await sentDocuments(page, language);
        expect(documents.length).toBeGreaterThan(0);
        for (const sent of documents) {
          expect(sent.id).toBe('srv-42');
          expect(sent.limits).toEqual({ max: 10 });
        }
      });
    }
  }
});

// A text missing its last closer, the closer, and the paths: the block left
// open, a field inside it, a field beside it, a path the text never had.
const UNCLOSED = {
  yaml: ['name: x\nweb: {listen: 80\n', '}\n', ['/web', '/web/listen', '/name', '/ghost']],
  json: ['{\n  "name": "x",\n  "web": {\n    "listen": 80\n', '  }\n}\n', ['/web', '/web/listen', '/name', '/ghost']],
  hcl: ['name = "x"\nweb {\n  listen = 80\n', '}\n', ['/web', '/web/listen', '/name', '/ghost']],
  xml: ['<name>x</name>\n<web>\n  <listen>80</listen>\n', '</web>\n', ['/web', '/web/listen', '/name', '/ghost']],
};

test.describe('closing what the host left open', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [language, [text, closer, paths]] of Object.entries(UNCLOSED)) {
    test(`${language}: goes through with any one path locked, given with the text or after`, async ({ page }) => {
      await open(page);
      for (const path of paths) {
        for (const later of [false, true]) {
          await mount(page, { language, value: text, readOnlyPaths: later ? undefined : [path] });
          if (later) await page.evaluate((path) => window.qa.update({ readOnlyPaths: [path] }), path);
          await input(page).evaluate((element) => {
            element.focus();
            element.setSelectionRange(element.value.length, element.value.length);
          });
          await page.keyboard.insertText(closer);
          await expect.poll(() => value(page), `${path}${later ? ' given after' : ''}`).toBe(text + closer);
        }
      }
    });
  }
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
