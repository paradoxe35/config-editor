import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 33 (FW32-3): a ConfigDocuments document saves alike shown and
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
