import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 17 (host): read-only values hold whatever order the text and the
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

test.describe('ConfigDocuments', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('refuses an unknown activeId; an EditorView stays with its own document', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const { ConfigDocuments } = await import('../config-editor/index.js');
      window.editor?.destroy();
      const host = document.createElement('div');
      document.body.prepend(host);
      const view = new ConfigDocuments(host, {
        documents: [
          { id: 'main', fileName: 'main.yaml', value: 'a: 1\n' },
          { id: 'z', fileName: 'z.yaml', value: 'z: 1\n' },
        ],
        onSave: () => {},
      });
      const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
      await wait();
      let unknown = null;
      try {
        view.update({ activeId: 'nope' });
      } catch (error) {
        unknown = error.message;
      }
      const active = view.activeId;
      const old = view.editor('main');
      const before = old.value;
      view.update({ documents: [{ id: 'z', fileName: 'z.yaml', value: 'z: 1\n' }] });
      await wait();
      const removed = { value: old.value, text: old.text, save: old.save(), dirty: old.isDirty, path: old.currentPath };
      view.update({
        documents: [
          { id: 'z', fileName: 'z.yaml', value: 'z: 1\n' },
          { id: 'main', fileName: 'main.yaml', value: 'again: 2\n', baseline: 'x: 0\n' },
        ],
      });
      await wait();
      const readded = { value: old.value, text: old.text, save: old.save(), dirty: old.isDirty, fresh: view.editor('main').value };
      return { unknown, active, before, removed, readded };
    });
    expect(result.unknown).toMatch(/activeId: no document has the id "nope"/);
    expect(result.active).toBe('main');
    expect(result.before).toBe('a: 1\n');
    expect(result.removed).toEqual({ value: '', text: '', save: false, dirty: false, path: null });
    expect(result.readded).toEqual({ value: '', text: '', save: false, dirty: false, fresh: 'again: 2\n' });
  });
});
