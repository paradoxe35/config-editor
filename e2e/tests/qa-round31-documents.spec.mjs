import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 31 (FW31-1): `save(id)` and `saveAll()` of a `ConfigDocuments`
// view apply one gate to every document, shown or never shown: text that
// does not read in its language is not sent (it was, before the document
// was first shown), and text that reads is, in every language.

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
