import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A `ConfigDocuments` document listed with `editAsText` (an unsaved Edit-as
// edit the host kept) and never shown answers exactly as when it is shown:
// `isDirty`, the tab's dirty dot, `onSyncChange`, `saveAll()` and
// `editor(id).save()` (what is sent, or nothing while the text cannot
// convert or does not parse). Every host language, every other `editAs`
// language; a text that converts, one that cannot (a `null` for TOML) and
// one that does not parse. Documents listed without it stay unmounted.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];
const SAMPLES = { yaml: 'a: 1\n', json: '{\n  "a": 1\n}\n', toml: 'a = 1\n', hcl: 'a = 1\n', xml: '<a>1</a>\n' };
const BROKEN = { yaml: 'a: [\n', json: '{\n  "a": \n', toml: 'a = \n', hcl: 'a = \n', xml: '<a>\n' };
// Texts the host language cannot hold, by host and `editAs` language.
const UNHELD = { toml: { yaml: 'a: null\n', json: '{\n  "a": null\n}\n', hcl: 'a = null\n' } };

test.skip(!built, 'needs the npm package built (cd js && npm run build)');

/** What a view over `other` and `x` (shown or not) answers for `x`. */
async function answers(page, { host, view, text, shown }) {
  return page.evaluate(
    async ({ host, view, text, shown, value }) => {
      const element = document.createElement('div');
      document.body.prepend(element);
      const heard = [];
      const saved = [];
      const documents = new window.module.ConfigDocuments(element, {
        documents: [
          { id: 'other', fileName: 'other.yaml', value: 'o: 1\n' },
          { id: 'x', fileName: `x.${host}`, value, baseline: value, editAs: view, editAsText: text },
        ],
        activeId: shown ? 'x' : 'other',
        onSave: (id, saving) => saved.push(`${id}=${saving}`),
        onSyncChange: (id, inSync) => id === 'x' && heard.push(`sync ${inSync}`),
        onDirtyChange: (id, dirty) => id === 'x' && heard.push(`dirty ${dirty}`),
      });
      await new Promise((resolve) => setTimeout(resolve, 700));
      const tab = element.querySelector('[role="tab"][data-document-id="x"]');
      const out = {
        dirty: documents.isDirty('x'),
        dot: tab.classList.contains('config-editor-documents__tab-dirty'),
        heard: [...heard],
        saveAll: documents.saveAll(),
      };
      out.save = documents.editor('x').save();
      out.saved = saved;
      documents.destroy();
      element.remove();
      return out;
    },
    { host, view, text, shown, value: SAMPLES[host] },
  );
}

test.describe('a never-shown document with an Edit-as text', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.module = await import('../config-editor/index.js');
    });
  });

  for (const host of LANGUAGES) {
    test(`a ${host} document answers as when shown`, async ({ page }) => {
      test.setTimeout(120000);
      for (const view of LANGUAGES.filter((language) => language !== host)) {
        // The text the user had on screen, then one value changed.
        const shownText = await page.evaluate(
          async ({ host, view, value }) => {
            const element = document.createElement('div');
            document.body.prepend(element);
            const documents = new window.module.ConfigDocuments(element, {
              documents: [{ id: 'x', fileName: `x.${host}`, value, editAs: view }],
            });
            await new Promise((resolve) => setTimeout(resolve, 300));
            const text = documents.editor('x').text;
            documents.destroy();
            element.remove();
            return text;
          },
          { host, view, value: SAMPLES[host] },
        );
        expect(shownText, `${host} as ${view}`).toContain('1');
        const cases = [
          ['converts', shownText.replace('1', '2')],
          ['does not parse', BROKEN[view]],
        ];
        if (UNHELD[host]?.[view]) cases.push(['cannot convert', UNHELD[host][view]]);
        for (const [kind, text] of cases) {
          const hidden = await answers(page, { host, view, text, shown: false });
          const seen = await answers(page, { host, view, text, shown: true });
          const label = `${host} as ${view}, ${kind}`;
          expect(hidden, label).toEqual(seen);
          expect(hidden.dirty, label).toBe(true);
          expect(hidden.dot, label).toBe(true);
          if (kind === 'converts') {
            expect(hidden.saveAll, label).toEqual(['x']);
            expect(hidden.save, label).toBe(true);
            expect(hidden.saved[0], label).toContain('2');
            expect(hidden.heard, label).toContain('sync true');
          } else {
            expect(hidden.saveAll, label).toEqual([]);
            expect(hidden.save, label).toBe(false);
            expect(hidden.saved, label).toEqual([]);
            expect(hidden.heard, label).toContain('sync false');
          }
        }
      }
    });
  }

  test('a document listed without one stays unmounted until shown', async ({ page }) => {
    const mounted = await page.evaluate(async () => {
      const element = document.createElement('div');
      document.body.prepend(element);
      const documents = new window.module.ConfigDocuments(element, {
        documents: [
          { id: 'other', fileName: 'other.yaml', value: 'o: 1\n' },
          { id: 'plain', fileName: 'plain.toml', value: 'a = 1\n', editAs: 'yaml' },
          { id: 'kept', fileName: 'kept.toml', value: 'a = 1\n', editAs: 'yaml', editAsText: 'a: 2\n' },
        ],
      });
      await new Promise((resolve) => setTimeout(resolve, 300));
      const has = (id) =>
        !!element.querySelector(`[role="tabpanel"][data-document-id="${id}"] .config-editor__input`);
      const out = { plain: has('plain'), kept: has('kept') };
      documents.destroy();
      element.remove();
      return out;
    });
    expect(mounted).toEqual({ plain: false, kept: true });
  });
});

// QA round 37 (HO37-4): in the same task as `new ConfigDocuments(...)`, a
// document with `editAsText` answers as after a pause: `isDirty`,
// `saveAll()`, `editor(id).save()` and what it sends, `value`,
// `unifiedDiff`. Before, until one tick it answered as if the text were
// absent (clean, the old value sent). Listed by an `update`, its editor
// opens once the view renders the new list (its panel is not on the page
// before): until then nothing is sent for it, never the old value.

/** What `x` answers right after it is listed (`via` the constructor or an
 * `update`), or `wait`ing a pause first, shown or not. */
async function answersAfter(page, { host, view, text, shown, via, wait }) {
  return page.evaluate(
    async ({ host, view, text, shown, via, wait, value }) => {
      const element = document.createElement('div');
      document.body.prepend(element);
      const saved = [];
      const other = { id: 'other', fileName: 'other.yaml', value: 'o: 1\n' };
      const x = { id: 'x', fileName: `x.${host}`, value, baseline: value, editAs: view, editAsText: text };
      const documents = new window.module.ConfigDocuments(element, {
        documents: via === 'constructor' ? [other, x] : [other],
        activeId: shown && via === 'constructor' ? 'x' : 'other',
        onSave: (id, saving) => saved.push(`${id}=${saving}`),
      });
      if (via === 'update') {
        await new Promise((resolve) => setTimeout(resolve, 300));
        documents.update({ documents: [other, x], ...(shown ? { activeId: 'x' } : {}) });
      }
      if (wait) await new Promise((resolve) => setTimeout(resolve, 700));
      const editor = documents.editor('x');
      const out = { dirty: documents.isDirty('x'), value: editor.value, diff: editor.unifiedDiff() };
      out.saveAll = documents.saveAll();
      out.save = editor.save();
      out.saved = saved;
      documents.destroy();
      element.remove();
      return out;
    },
    { host, view, text, shown, via, wait, value: SAMPLES[host] },
  );
}

test.describe('a document with an Edit-as text, asked in the same task', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      window.module = await import('../config-editor/index.js');
    });
  });

  for (const via of ['constructor', 'update']) {
    test(`listed by the ${via}, it answers as after a pause`, async ({ page }) => {
      test.setTimeout(180000);
      for (const host of LANGUAGES) {
        for (const view of LANGUAGES.filter((language) => language !== host)) {
          // The value changed, written in the Edit-as language.
          const text = await page.evaluate(
            async ({ host, view, value }) => {
              const tools = await import('../config-editor/tools.js');
              return tools.serialize(view, tools.parse(host, value));
            },
            { host, view, value: SAMPLES[host] },
          );
          const edited = text.replace('1', '2');
          for (const shown of [false, true]) {
            const label = `${host} as ${view}, ${shown ? 'shown' : 'not shown'}`;
            const now = await answersAfter(page, { host, view, text: edited, shown, via, wait: false });
            const later = await answersAfter(page, { host, view, text: edited, shown, via, wait: true });
            expect(later.dirty, label).toBe(true);
            expect(later.saveAll, label).toEqual(['x']);
            expect(later.saved[0], label).toContain('2');
            if (via === 'constructor') {
              expect(now, label).toEqual(later);
            } else {
              // Its editor opens once the view renders the new list: until
              // then the old value is never sent.
              expect({ saveAll: now.saveAll, save: now.save, saved: now.saved }, label).toEqual({
                saveAll: [],
                save: false,
                saved: [],
              });
            }
          }
        }
      }
    });
  }
});
