import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 32 (FW32-1, FW32-2): a leading byte order mark is no part of a
// document, shown or never shown. `save(id)`, `saveAll()` and
// `editor(id).save()` of a never-shown document answer as they do once it
// is shown, in every language, whatever characters it holds (a marked XML
// document with an emoji once crashed the view; a marked JSON or HCL one
// was refused), and the text sent keeps its mark. `saveAll()` sends every
// other document past one it refuses.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const DOCUMENTS = pathToFileURL(join(js, 'examples', 'documents.html')).href;

const BOM = '﻿';

// [name, file, text]: every one reads; with a mark in front it still does.
const CASES = [
  ['YAML', 'a.yaml', 'a: 😀日本é\n'],
  ['YAML key', 'a.yaml', '😀日本é: 1\n'],
  ['JSON', 'a.json', '{"a": "😀日本é"}\n'],
  ['JSON key', 'a.json', '{"😀日本é": 1}\n'],
  ['TOML', 'a.toml', 'a = "😀日本é"\n'],
  ['HCL', 'a.tf', 'a = "😀日本é"\n'],
  ['XML element text', 'a.xml', '<r><a>😀</a></r>\n'],
  ['XML element name', 'a.xml', '<r><日本>1</日本></r>\n'],
  ['XML attribute', 'a.xml', '<r a="é"/>\n'],
  ['XML declaration', 'a.xml', '<?xml version="1.0" encoding="utf-8"?>\n<r><a>😀</a></r>\n'],
];

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

/** What `save('x')`, `saveAll()` and `editor('x').save()` answer and send. */
async function saves(page, file, value, shown) {
  await mount(page, [
    { id: 'first', fileName: 'first.yaml', value: 'k: 1\n' },
    { id: 'x', fileName: file, value, baseline: 'changed\n' },
  ]);
  if (shown) {
    await page.evaluate(() => window.documentsView.select('x'));
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  }
  return page.evaluate(() => {
    const view = window.documentsView;
    const one = view.save('x');
    const all = view.saveAll();
    const editor = view.editor('x').save();
    return { one, all, editor, sent: window.sent };
  });
}

test.describe('a byte order mark on a document shown or never shown', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test.beforeEach(async ({ page }) => {
    page.errors = [];
    page.on('pageerror', (error) => page.errors.push(String(error)));
    await page.goto(DOCUMENTS);
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
  });

  for (const [name, file, text] of CASES) {
    test(`${name}: saved alike, the mark kept`, async ({ page }) => {
      for (const value of [text, BOM + text]) {
        const unshown = await saves(page, file, value, false);
        const shown = await saves(page, file, value, true);
        expect(unshown, `never shown ${JSON.stringify(value)}`).toEqual({
          one: true,
          all: ['x'],
          editor: true,
          sent: [
            ['x', value],
            ['x', value],
            ['x', value],
          ],
        });
        expect(shown, `shown ${JSON.stringify(value)}`).toEqual(unshown);
      }
      expect(page.errors).toEqual([]);
    });
  }

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
