import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Read-only fields and blocks: refused edits, duplicates, whitespace, suggestions, broken text, line
// commands next to a lock.

const saved = (page) => page.locator('.demo__output-text');

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

/** Puts the caret `offset` units into the first `marker`. */
async function caretAt(page, marker, offset = 0) {
  const text = await editor(page).value();
  const at = text.indexOf(marker);
  expect(at, marker).toBeGreaterThanOrEqual(0);
  await caretTo(page, at + offset);
}

// How to break each protected fixture outside its locks (caret at
// `marker` + `offset`, then the key), and the locked value inside the
// read-only `limits` block.
const BREAK = {
  yaml: { marker: 'name: edge', offset: 6, type: '"', max: 'max: 10' },
  json: { marker: '"edge"', offset: 5, press: 'Delete', max: '"max": 10' },
  toml: { marker: '"edge"', offset: 5, press: 'Delete', max: 'max = 10' },
  hcl: { marker: '"edge"', offset: 5, press: 'Delete', max: 'max = 10' },
  xml: { marker: '</name>', offset: 6, press: 'Delete', max: '<max>10' },
};

// Whitespace typed inside a locked field: [language, marker, offset, key].
const WHITESPACE = [
  ['json', '    "max"', 0, ' '],
  ['json', '"max": 10', 9, ' '],
  ['json', '"max": 10', 6, ' '],
  ['json', '"id": "srv', 5, ' '],
  ['json', '    "burst"', 2, 'Backspace'],
  ['toml', 'max = 10', 0, ' '],
  ['toml', 'max = 10', 4, ' '],
  ['toml', 'id = "srv', 3, ' '],
  ['hcl', '  max = 10', 0, ' '],
  ['hcl', 'max = 10', 4, ' '],
  ['xml', '  <max>', 0, ' '],
  ['xml', '</id>', 5, ' '],
  ['yaml', 'id: srv', 3, ' '],
  ['yaml', 'max: 10', 7, ' '],
  ['yaml', '  burst', 0, 'Enter'],
];

test.describe("a read-only key shows its docs and says why", () => {
  test.describe("keyboard", () => {
    setupEditor(test, "?schema=protected");

    test("Ctrl+K Ctrl+I on a read-only key", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("id"));
      const at = await pointAt(page, line, 1);
      await page.mouse.click(at.x, at.y);
      await page.keyboard.press("Control+k");
      await page.keyboard.press("Control+i");
      const card = page.locator(".config-editor__hover");
      await expect(card).toBeVisible();
      await expect(card.locator(".config-editor__hover-key")).toHaveText("id");
      const announcement = page.locator(".config-editor__announcement");
      await expect(announcement).toContainText("Assigned by the server");
      await expect(page.locator(".config-editor__protect-tip")).toContainText(
        "Assigned by the server",
      );
    });

    test("an editable key still shows its card alone", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("name"));
      const at = await pointAt(page, line, 1);
      await page.mouse.click(at.x, at.y);
      await page.keyboard.press("Control+k");
      await page.keyboard.press("Control+i");
      await expect(page.locator(".config-editor__hover")).toBeVisible();
      await expect(page.locator(".config-editor__protect-tip")).toHaveCount(0);
    });
  });

  test.describe("touch", () => {
    test.use({ hasTouch: true });
    setupEditor(test, "?schema=protected");

    test("a tap on a read-only key", async ({ page }) => {
      const e = editor(page);
      const line = (await e.value())
        .split("\n")
        .findIndex((text) => text.startsWith("id"));
      const at = await pointAt(page, line, 1);
      await page.touchscreen.tap(at.x, at.y);
      await expect(page.locator(".config-editor__hover")).toBeVisible();
      await expect(page.locator(".config-editor__announcement")).toContainText(
        "Assigned by the server",
      );
    });
  });
});

// Read-only blocks never grow, infinities stay numbers,
// placeholders are real only in their own field, serialize writes only what
// reads back the same, host inputs are checked, and a drag from a secret
// chip takes it whole.
for (const [language, { after, keys, typed }] of Object.entries(EXTEND)) {
  test.describe(`${language}: a read-only block`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test('takes no new key, typed one key at a time', async ({ page }) => {
      const text = await editor(page).value();
      await caretTo(page, text.indexOf(after) + after.length);
      for (const key of keys) await page.keyboard.press(key);
      await page.keyboard.type(typed);
      await page.waitForTimeout(300);
      // Taken and held where the key lands past the block's last line (no
      // keystroke is refused for how the parse reads it): the reason shows.
      if ((await editor(page).value()).includes('extra')) {
        await expect(page.locator('.config-editor__diagnostic-marker[data-message*="now reads text that was added into it or beside it"]')).not.toHaveCount(0);
      }
      // The host may hear the half-typed line (its locked values as they
      // were), never the key.
      await expect(saved(page)).not.toHaveText(/^\s*"?extra"?\s*[:=][ \t]*\S/m);
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
      // Taken and held: the text may write the key into the locked table
      // (no keystroke is refused for how the parse reads it), and the host
      // never gets it.
      await expect(saved(page)).not.toContainText('[limits.sub]');
      await expect(saved(page)).not.toContainText('limits.extra');
    }
  });
});

test.describe('yaml: a duplicate of a read-only key', () => {
  setupEditor(test, '?schema=protected');

  test('is held at the end of the document', async ({ page }) => {
    const text = await editor(page).value();
    await caretTo(page, text.length);
    await page.keyboard.type('limits:\n  max: 99\nid: other\n');
    await page.waitForTimeout(300);
    // Taken: the text may write each a second time (no keystroke is refused
    // for how the parse reads it). YAML takes no key twice, so the text does
    // not parse, the error says why, and the host hears nothing of it.
    await expect(page.locator('.config-editor__diagnostic-marker[data-message*="written twice"]')).not.toHaveCount(0);
    const sent = await saved(page).textContent();
    expect(sent.match(/^limits:/gm)).toHaveLength(1);
    expect(sent.match(/^id:/gm)).toHaveLength(1);
  });
});

for (const [language, broken] of Object.entries(BREAK)) {
  test.describe(`${language}: a document that does not parse`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test('keeps the read-only block locked, and its repair free', async ({ page }) => {
      const original = await editor(page).value();
      await caretAt(page, broken.marker, broken.offset);
      if (broken.type) await page.keyboard.type(broken.type);
      else await page.keyboard.press(broken.press);
      await page.waitForTimeout(300);
      const unparseable = await editor(page).value();
      expect(unparseable).not.toBe(original);
      const reported = await saved(page).textContent();

      // Inside the block: a digit typed, a line deleted.
      await caretAt(page, broken.max, broken.max.length);
      await page.keyboard.type('9');
      await caretAt(page, broken.max, 0);
      await page.keyboard.press('Shift+End');
      await page.keyboard.press('Delete');
      await page.waitForTimeout(300);
      expect(await editor(page).value()).toBe(unparseable);
      await expect(saved(page)).toHaveText(reported);

      // Repairing the syntax goes through.
      await caretAt(page, broken.marker.slice(0, broken.offset), broken.offset);
      if (broken.type) await page.keyboard.press('Delete');
      else await page.keyboard.type(language === 'xml' ? '>' : '"');
      await page.waitForTimeout(300);
      expect(await editor(page).value()).toBe(original);
    });
  });
}

for (const [language, marker, offset, key] of WHITESPACE) {
  test.describe(`${language}: whitespace in a locked field`, () => {
    setupEditor(test, `?schema=protected${language === 'yaml' ? '' : `&language=${language}`}`);

    test(`${JSON.stringify(key)} at ${JSON.stringify(marker)}+${offset} goes through`, async ({ page }) => {
      const before = await editor(page).value();
      await caretAt(page, marker, offset);
      if (key === ' ') await page.keyboard.type(' ');
      else await page.keyboard.press(key);
      await page.waitForTimeout(300);
      expect(await editor(page).value()).not.toBe(before);
    });
  });
}

test.describe('yaml: suggestions inside a read-only block', () => {
  setupEditor(test, '?schema=protected');

  test('show the reason, and accepting one changes nothing', async ({ page }) => {
    // On a locked key's text (the block holds every key it may).
    await caretAt(page, '  max: 10', 4);
    const before = await editor(page).value();
    await page.keyboard.press('Control+Space');
    const popup = page.locator('.config-editor__completions');
    await expect(popup).toBeVisible();
    await expect(popup).toContainText('Set by your plan');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toBe(before);
  });
});

test.describe('yaml: a locked line', () => {
  setupEditor(test, '?schema=protected');

  test('keeps its indent', async ({ page }) => {
    const before = await editor(page).value();
    await caretAt(page, '  max', 0);
    await page.keyboard.type(' ');
    await caretAt(page, '  burst', 1);
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toBe(before);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Read-only values hold whatever order the text and the
  // locks arrive in (a host text that never parsed, at mount or as a new
  // value; paths named while the text does not parse), no callback fires
  // after `destroy()`, and `ConfigDocuments` refuses an unknown `activeId`
  // and keeps an `EditorView` to the document it was made for.
  const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

  const js = join(root, 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

  const schema = readFileSync(join(root, 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'protected.schema.json'), 'utf8');

  // One document per language: `id` (a read-only value) and `limits` (a
  // read-only block) by the schema, `web` a block the host may lock; the
  // break (`good` → `bad`, an unclosed quote or tag on `name`) and its repair
  // (the caret `at` units into the first `bad`, then `key`).
  const DOCUMENTS = {
    yaml: {
      text: 'id: srv-42\nname: edge\nlisten: 7835\nweb:\n  port: 80\n  host: a\nlimits:\n  max: 10\n  burst: 20\n',
      good: 'name: edge',
      bad: 'name: "edge',
      repair: { at: 6, key: 'Delete' },
    },
    json: {
      text: '{\n  "id": "srv-42",\n  "name": "edge",\n  "listen": 7835,\n  "web": {\n    "port": 80,\n    "host": "a"\n  },\n  "limits": {\n    "max": 10,\n    "burst": 20\n  }\n}\n',
      good: '"edge",',
      bad: '"edge,',
      repair: { at: 5, type: '"' },
    },
    toml: {
      text: 'id = "srv-42"\nname = "edge"\nlisten = 7835\n\n[web]\nport = 80\nhost = "a"\n\n[limits]\nmax = 10\nburst = 20\n',
      good: '"edge"\n',
      bad: '"edge\n',
      repair: { at: 5, type: '"' },
    },
    hcl: {
      text: 'id = "srv-42"\nname = "edge"\nlisten = 7835\nweb {\n  port = 80\n  host = "a"\n}\nlimits {\n  max = 10\n  burst = 20\n}\n',
      good: '"edge"\n',
      bad: '"edge\n',
      repair: { at: 5, type: '"' },
    },
    xml: {
      text: '<id>srv-42</id>\n<name>edge</name>\n<listen>7835</listen>\n<web>\n  <port>80</port>\n  <host>a</host>\n</web>\n<limits>\n  <max>10</max>\n  <burst>20</burst>\n</limits>\n',
      good: '</name>',
      bad: '</name',
      repair: { at: 6, type: '>' },
    },
  };

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

  /** Types into the locked fields: `id`, `limits/max`, `web/port`. */
  async function editLocked(page) {
    for (const [marker, offset] of [
      ['srv-42', 4],
      ['10', 1],
      ['80', 1],
    ]) {
      await caretAt(page, marker, offset);
      await page.keyboard.type('5');
    }
  }

  async function repair(page, { bad, repair: { at, key, type } }) {
    await caretAt(page, bad, at);
    if (key) await page.keyboard.press(key);
    else await page.keyboard.type(type);
  }

  // How the text and the locks arrive.
  const ORDERS = {
    'the host text never parsed (at mount)': async (page, language, document, locks) => {
      await mount(page, { language, value: document.text.replace(document.good, document.bad), ...locks });
    },
    'a new host value that does not parse': async (page, language, document, locks) => {
      await mount(page, { language, value: document.text, ...locks });
      await page.evaluate((text) => window.qa.update({ value: text }), document.text.replace(document.good, document.bad));
    },
    'paths named while the user broke the text': async (page, language, document, locks) => {
      await mount(page, { language, value: document.text, schema: locks.schema });
      await caretAt(page, document.good);
      await page.evaluate(
        ([good, bad]) => {
          const element = document.querySelector('#qa .config-editor__input');
          const at = element.value.indexOf(good);
          element.setSelectionRange(at, at + good.length);
        },
        [document.good, document.bad],
      );
      await page.keyboard.insertText(document.bad);
      if (locks.readOnlyPaths) await page.evaluate((paths) => window.qa.update({ readOnlyPaths: paths }), locks.readOnlyPaths);
    },
    'paths named on a host text that never parsed': async (page, language, document, locks) => {
      await mount(page, { language, value: document.text.replace(document.good, document.bad), schema: locks.schema });
      if (locks.readOnlyPaths) await page.evaluate((paths) => window.qa.update({ readOnlyPaths: paths }), locks.readOnlyPaths);
    },
  };


  test.describe('read-only values while the text does not parse', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    for (const [language, document] of Object.entries(DOCUMENTS)) {
      for (const [order, arrive] of Object.entries(ORDERS)) {
        test(`${language}: ${order}`, async ({ page }) => {
          await page.goto(example('vanilla.html'));
          await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
          await arrive(page, language, document, { schema: true, readOnlyPaths: ['/web'] });
          expect(await value(page)).toBe(document.text.replace(document.good, document.bad));
          await editLocked(page);
          await repair(page, document);
          await page.waitForTimeout(300);
          expect(await value(page)).toBe(document.text);
          await page.evaluate(() => window.qa.save());
          const documents = await sentDocuments(page, language);
          expect(documents.length).toBeGreaterThan(0);
          for (const sent of documents) {
            expect(sent.id).toBe('srv-42');
            expect(sent.limits).toEqual({ max: 10, burst: 20 });
            expect(sent.web).toEqual({ port: 80, host: 'a' });
          }
        });
      }
    }
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // The rule for a host text that does not parse holds
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

  // A text missing its last closer, the closer, and the paths: the block left
  // open, a field inside it, a field beside it, a path the text never had.
  const UNCLOSED = {
    yaml: ['name: x\nweb: {listen: 80\n', '}\n', ['/web', '/web/listen', '/name', '/ghost']],
    json: ['{\n  "name": "x",\n  "web": {\n    "listen": 80\n', '  }\n}\n', ['/web', '/web/listen', '/name', '/ghost']],
    hcl: ['name = "x"\nweb {\n  listen = 80\n', '}\n', ['/web', '/web/listen', '/name', '/ghost']],
    xml: ['<name>x</name>\n<web>\n  <listen>80</listen>\n', '</web>\n', ['/web', '/web/listen', '/name', '/ghost']],
  };


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
});

// A group of its own (its own helpers).
test.describe(() => {
  // A read-only field in a language
  // without spans holds through broken text, and a save waits for a text that
  // parses; a refusal takes back only what it is about; a host update while
  // editing as another language sends no onChange; YAML tags are valid YAML
  // and Edit as keeps them; several YAML documents say so in plain words;
  // every position counts LF, CRLF and lone CR alike; infinities, NaN and
  // 64-bit integers come through `parse` exactly; and the small host LOWs.
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

  const TEXT = '# c\nid=abc\nlisten=7835\nname=x\n';


  test.describe('a read-only field in a language without spans, through broken text', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    const cases = {
      'break another line, then edit the locked value': async (page) => {
        await caretAt(page, 'name=x', 4, 1);
        await page.keyboard.press('Delete');
        await caretAt(page, 'listen=7835', 11);
        await page.keyboard.type('99');
      },
      'replace the locked line so the text no longer parses': async (page) => {
        await caretAt(page, 'listen=7835', 0, 11);
        await page.keyboard.type('z');
      },
      'select everything and type': async (page) => {
        await caretAt(page, '# c', 0);
        await page.keyboard.press('ControlOrMeta+a');
        await page.keyboard.type('x');
      },
    };
    // Without spans nothing says where the value is written: the edit is
    // taken, and nothing reaching the host reads another value (a text
    // reading it otherwise is held, one that does not parse waits).
    for (const [name, edit] of Object.entries(cases)) {
      test(`${name}: nothing sent carries another value`, async ({ page }) => {
        await open(page);
        await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
        await edit(page);
        await expect.poll(() => value(page)).not.toBe(TEXT);
        const carried = await page.evaluate(() => {
          window.qa.save();
          return [...window.sent, ...window.saved].filter((text) => !text.split('\n').includes('listen=7835'));
        });
        expect(carried).toEqual([]);
      });
    }

    test('a save waits while the text does not parse, saying which field', async ({ page }) => {
      await open(page);
      await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
      await caretAt(page, 'name=x', 4, 1);
      await page.keyboard.press('Delete');
      expect(await page.evaluate(() => window.qa.save())).toBe(false);
      await expect(page.locator('#qa .config-editor__protect-tip')).toContainText('/listen');
      expect(await page.evaluate(() => window.saved)).toEqual([]);
      await caretAt(page, 'namex', 4);
      await page.keyboard.type('=');
      expect(await page.evaluate(() => window.qa.save())).toBe(true);
    });

    test('typing over the locked line is taken, and nothing sent carries it', async ({ page }) => {
      // Typed on a free line, then over the locked line: both taken; the
      // text reads the locked field otherwise, so the save waits and
      // nothing sent or saved carries another value.
      await open(page);
      await mount(page, { language: 'bare21', value: TEXT, readOnlyPaths: ['/listen'] });
      await caretAt(page, 'name=x', 6);
      await page.keyboard.type('abc');
      await caretAt(page, 'listen=7835', 0, 11);
      await page.keyboard.type('z');
      await expect.poll(() => value(page)).toBe('# c\nid=abc\nz\nname=xabc\n');
      expect(await page.evaluate(() => window.qa.save())).toBe(false);
      const carried = await page.evaluate(() =>
        [...window.sent, ...window.saved].filter((text) => !text.split('\n').includes('listen=7835')),
      );
      expect(carried).toEqual([]);
    });
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options, name = 'vanilla') =>
    `${pathToFileURL(join(js, 'examples', `${name}.html`)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const said = (page) =>
    input(page).evaluate((element) => element.closest('.config-editor').querySelector('.config-editor__announcement').textContent);

  async function open(page, options, name) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(options, name));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  // The line commands did nothing at all, so next to a lock or in a
  // read-only editor they said nothing either.
  const COMMANDS = {
    'Alt+↑': 'Alt+ArrowUp',
    'Alt+↓': 'Alt+ArrowDown',
    'Shift+Alt+↑': 'Shift+Alt+ArrowUp',
    'Shift+Alt+↓': 'Shift+Alt+ArrowDown',
    'Ctrl+Shift+K': 'Control+Shift+K',
    'Ctrl+]': 'Control+BracketRight',
    'Ctrl+[': 'Control+BracketLeft',
  };

  // `name` on line 2, between two other fields, its value locked.
  const LOCKED = {
    yaml: 'a: 1\nname: edge\nb: 2\n',
    json: '{\n  "name": "edge",\n  "b": 2\n}\n',
    toml: 'a = 1\nname = "edge"\nb = 2\n',
    hcl: 'a = 1\nname = "edge"\nb = 2\n',
    xml: '<a>1</a>\n<name>edge</name>\n<b>2</b>\n',
  };

  async function caretOnLine2(page) {
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('line commands beside a lock', () => {
    for (const [language, value] of Object.entries(LOCKED)) {
      test(`${language}: next to a lock, a line command changes the locked value never, and says why when refused`, async ({ page }) => {
        const errors = await open(page, { value, language, readOnlyPaths: ['/name'] });
        let refused = 0;
        for (const [command, key] of Object.entries(COMMANDS)) {
          // An unindented line has nothing to outdent: nothing to refuse.
          if (command === 'Ctrl+[') continue;
          await caretOnLine2(page);
          await page.keyboard.press(key);
          await page.waitForTimeout(100);
          const now = await input(page).inputValue();
          if (now === value) {
            refused += 1;
            // Refused: why, at the caret and to screen readers.
            const tip = page.locator('.config-editor__protect-tip--refused');
            await expect(tip, command).toBeVisible();
            await expect.poll(() => said(page), command).toMatch(/name|read-only|locked/i);
          } else {
            // Made: the locked value is still there, as it was.
            expect(now, command).toContain('edge');
            // Undone: a swap of two lines is judged alike both ways.
            await page.keyboard.press('Control+z');
            await expect(input(page), `${command} undone`).toHaveValue(value);
          }
        }
        // Deleting the locked line is refused, at least.
        expect(refused).toBeGreaterThan(0);
        expect(errors).toEqual([]);
      });
    }
  });
});
