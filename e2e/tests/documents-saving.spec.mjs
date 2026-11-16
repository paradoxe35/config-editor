import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Saving documents shown or never shown alike, whatever their line breaks; saveAll.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

// [name, file, text, refused]: what the parse gate refuses, and the same
// language's text that it lets through.
const CASES = [
  ['YAML unclosed flow list', 'b.yaml', 'a: [1, 2\n', true],
  ['YAML', 'b.yaml', 'a: [1, 2]\n', false],
  ['JSON unclosed object', 'a.json', '{"a": 2,\n', true],
  ['JSON', 'a.json', '{"a": 2}\n', false],
  ['TOML integer past i64', 'c.toml', 'id = 9223372036854775808\na = 2\n', true],
  ['TOML', 'c.toml', 'id = 9223372036854775807\na = 2\n', false],
  ['HCL unclosed block', 'e.hcl', 'a {\n  b = 1\n', true],
  ['HCL', 'e.hcl', 'a {\n  b = 1\n}\n', false],
  ['XML unbound prefix', 'd.xml', '<r><p:x>1</p:x></r>\n', true],
  ['XML', 'd.xml', '<r><x>1</x></r>\n', false],
];

async function saves(page, file, value, shown) {
  return page.evaluate(
    async ([file, value, shown]) => {
      const CE = await import(new URL('../config-editor/index.js', location.href).href);
      document.getElementById('host')?.remove();
      window.view?.destroy();
      const host = document.createElement('div');
      host.id = 'host';
      document.body.prepend(host);
      const sent = [];
      window.view = new CE.ConfigDocuments(host, {
        documents: [
          { id: 'first', fileName: 'first.yaml', value: 'k: 1\n' },
          { id: 'x', fileName: file, value, baseline: 'changed\n' },
        ],
        onSave: (id) => sent.push(id),
      });
      if (shown) {
        window.view.select('x');
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      }
      const one = window.view.save('x');
      const all = window.view.saveAll();
      return { one, all, sent };
    },
    [file, value, shown],
  );
}

const DOCUMENTS = pathToFileURL(join(js, 'examples', 'documents.html')).href;

const BOM = '﻿';

/** Mounts `documents` in a fresh view, recording what `onSave` hears. */
async function mount(page, documents) {
  await page.evaluate(async (documents) => {
    const CE = await import(new URL('../config-editor/index.js', location.href).href);
    window.documentsView?.destroy();
    document.getElementById('probe')?.remove();
    const host = document.createElement('div');
    host.id = 'probe';
    host.style.height = '300px';
    document.body.prepend(host);
    window.sent = [];
    window.documentsView = new CE.ConfigDocuments(host, {
      documents,
      onSave: (id, value) => window.sent.push([id, value]),
    });
  }, documents);
}

// `save(id)` and `saveAll()` of a `ConfigDocuments`
// view apply one gate to every document, shown or never shown: text that
// does not read in its language is not sent (it was, before the document
// was first shown), and text that reads is, in every language.
test.describe('saving a document shown or never shown', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  for (const [name, file, value, refused] of CASES) {
    test(`${name}: ${refused ? 'refused' : 'sent'} alike`, async ({ page }) => {
      await page.goto(VANILLA);
      await page.waitForFunction(() => document.body.dataset.ready === 'true');
      const unshown = await saves(page, file, value, false);
      const shown = await saves(page, file, value, true);
      expect(unshown, 'never shown').toEqual(shown);
      expect(unshown.one).toBe(!refused);
      expect(unshown.all).toEqual(refused ? [] : ['x']);
    });
  }
});

test.describe('a byte order mark on a document shown or never shown', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');
  test.beforeEach(async ({ page }) => {
    page.errors = [];
    page.on('pageerror', (error) => page.errors.push(String(error)));
    await page.goto(DOCUMENTS);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
  });
  test('saveAll sends every document past one it refuses, and one with a mark', async ({ page }) => {
    await mount(page, [
      { id: 'a', fileName: 'a.yaml', value: 'k: 2\n', baseline: 'k: 1\n' },
      { id: 'x', fileName: 'x.xml', value: `${BOM}<r>😀</r>\n`, baseline: '<r/>\n' },
      { id: 'bad', fileName: 'bad.json', value: '{', baseline: '{}' },
      { id: 'b', fileName: 'b.toml', value: 'k = 2\n', baseline: 'k = 1\n' },
    ]);
    const all = await page.evaluate(() => window.documentsView.saveAll());
    expect(all).toEqual(['a', 'x', 'b']);
    expect(await page.evaluate(() => window.sent.map(([id]) => id))).toEqual(['a', 'x', 'b']);
    expect(page.errors).toEqual([]);
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // A ConfigDocuments document saves alike shown and
  // never shown, whatever its line breaks (LF, CRLF, a lone CR, a mix), in
  // every language: the save gate reads the text with LF line breaks as the
  // editor does, and the text sent is the host's as given. Through the npm
  // package (run `npm run build` in js/).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

  const DOCUMENTS = {
    yaml: ['a.yaml', 'a: 1\nt:\n  b: 2\n'],
    json: ['a.json', '{\n  "a": 1,\n  "b": 2\n}\n'],
    toml: ['a.toml', 'a = 1\n[t]\nb = 2\n'],
    hcl: ['a.tf', 'a = 1\nt {\n  b = 2\n}\n'],
    xml: ['a.xml', '<r>\n  <a>1</a>\n  <b>2</b>\n</r>\n'],
  };

  const BREAKS = { LF: '\n', CRLF: '\r\n', CR: '\r', mixed: null };


  test('a document saves alike shown and never shown, whatever its line breaks', async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    await page.goto(VANILLA);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const failures = await page.evaluate(
      async ([documents, breaks]) => {
        window.editor?.destroy();
        const CE = await import('../config-editor/index.js');
        const host = document.createElement('div');
        host.style.height = '300px';
        document.body.prepend(host);
        const failures = [];
        for (const [language, [fileName, text]] of Object.entries(documents)) {
          for (const [name, ending] of Object.entries(breaks)) {
            const value =
              ending === null
                ? text
                    .split('\n')
                    .map((line, at, lines) => (at === lines.length - 1 ? line : line + ['\r\n', '\r', '\n'][at % 3]))
                    .join('')
                : text.replace(/\n/g, ending);
            const heard = {};
            for (const shown of [false, true]) {
              const saved = [];
              const view = new CE.ConfigDocuments(host, {
                documents: [
                  { id: 'first', fileName: 'first.yaml', value: 'k: 1\n' },
                  { id: 'x', fileName, value, baseline: '' },
                ],
                onSave: (id, text) => saved.push(text),
              });
              if (shown) {
                view.select('x');
                await new Promise((resolve) => setTimeout(resolve, 200));
              }
              heard[shown ? 'shown' : 'unshown'] = [view.save('x'), saved[0] ?? null];
              view.destroy();
            }
            if (JSON.stringify(heard.shown) !== JSON.stringify(heard.unshown) || heard.unshown[1] !== value) {
              failures.push(`${language} ${name}: ${JSON.stringify(heard)}`);
            }
          }
        }
        host.remove();
        return failures;
      },
      [DOCUMENTS, BREAKS],
    );
    expect(failures).toEqual([]);
  });
});
