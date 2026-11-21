import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A save a lock holds keeps the host on the text it heard last. When a late
// echo leaves the host on an older text, the editor sends that text again;
// the edits held since stay recorded, so a host change made after it is
// rebased over them (P4) and the lock follows them (P1).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const OPENING = 'listen: 7835\npublic_address: tunnel.example.com\n';
// "lic" of `public_address`.
const LOCK = { start: 16, end: 19 };

test.describe('a host change after a held save is sent again', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  for (const host of ['editor', 'documents']) {
    test(`${host}: keeps the held typing and the lock on its text`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(
        async ([host, opening, lock]) => {
          window.editor?.destroy();
          const module = await import('../config-editor/index.js');
          const element = document.createElement('div');
          document.body.prepend(element);
          // The host hands each text back when the test says so.
          window.held = [];
          if (host === 'editor') {
            const editor = new module.ConfigEditor(element, {
              value: opening,
              echoes: true,
              readOnlyRanges: [lock],
              onChange: (text) => window.held.push(() => editor.update({ value: text })),
            });
            window.shown = () => editor;
            window.change = (changes) => editor.applyChanges(changes);
          } else {
            const list = (value) => [{ id: 'a', fileName: 'a.yaml', value, readOnlyRanges: [lock] }];
            const view = new module.ConfigDocuments(element, {
              documents: list(opening),
              echoes: true,
              onChange: (id, text) => window.held.push(() => view.update({ documents: list(text) })),
            });
            window.shown = () => view.editor('a');
            window.change = (changes) => view.applyChanges('a', changes);
          }
        },
        [host, OPENING, LOCK],
      );
      const input = page.locator('.config-editor__input').first();
      await input.focus();
      await page.keyboard.press('Control+End');
      // Two texts sent; the third character makes the locked field read
      // otherwise, so its save is held.
      await page.keyboard.type('  Ω', { delay: 30 });
      await expect.poll(() => page.evaluate(() => window.held.length)).toBe(2);
      // The newer echo first, then the older one: the host is left behind,
      // and after the pause it hears the text it heard last again.
      await page.evaluate(() => {
        window.held[1]();
        window.held[0]();
      });
      await expect.poll(() => page.evaluate(() => window.held.length), { timeout: 3000 }).toBe(3);
      await page.evaluate(() => window.change([{ from: 13, to: 13, insert: '# host note\n' }]));
      const want = 'listen: 7835\n# host note\npublic_address: tunnel.example.com\n  Ω';
      await expect.poll(() => page.evaluate(() => window.shown().text)).toBe(want);
      const marked = await page.evaluate(() =>
        [...document.querySelectorAll('.config-editor__locked')].map((span) => span.textContent).join(''),
      );
      expect(marked).toContain('lic');
      expect(marked).not.toContain('ost');
      expect(errors).toEqual([]);
    });
  }
});
