import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// QA round 27, accessibility: why an edit was refused is said through the
// editor's live region (one there from the start), not a region made with
// its text; Escape closes the hover card or the problem tip with the
// pointer on the text it shows for and the focus nowhere (WCAG 1.4.13).

const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];

// L27-A1: a refused edit on a read-only field showed its message in a
// `role="status"` region made together with it, and made again for each
// refused key: screen readers may never say it.
for (const language of LANGUAGES) {
  test.describe(`L27-A1 ${language}`, () => {
    setupEditor(test, `?schema=protected&language=${language}`);

    test('a refused edit is said through the live region', async ({ page }) => {
      const e = editor(page);
      const before = await e.value();
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('srv-42') + 2;
        element.focus();
        element.setSelectionRange(at, at);
      });
      const said = page.locator('.config-editor__announcement');
      const tip = page.locator('.config-editor__protect-tip--refused');
      for (const key of ['x', 'y', 'Backspace']) {
        await page.keyboard.press(key);
        await expect(tip).toBeVisible();
        const message = (await tip.textContent()).trim();
        expect(message.length).toBeGreaterThan(0);
        await expect.poll(async () => (await said.textContent()).replace(/ /g, '').trim()).toBe(message);
        // The tip itself is no live region (it would be said twice, or not
        // at all).
        await expect(tip).not.toHaveAttribute('role', 'status');
        await expect(tip).not.toHaveAttribute('aria-live', /.+/);
      }
      expect(await e.value()).toBe(before);
    });
  });
}

// L27-A2: Escape with the focus nowhere closed a popup only with the
// pointer on it, not on the key or underline it shows for.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const DOCS = {
  yaml: 'a: 1\nb: 2\nport: 3\n',
  json: '{ "a": 1,\n  "b": 2,\n  "port": 3 }\n',
  toml: 'a = 1\nb = 2\nport = 3\n',
  hcl: 'a = 1\nb = 2\nport = 3\n',
  xml: '<a>1</a>\n<b>2</b>\n<port>3</port>\n',
};
const schema = JSON.stringify({ type: 'object', properties: { port: { type: 'integer', description: 'Port docs' } } });

test.describe('L27-A2', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 900, height: 500 } });
  for (const language of LANGUAGES) {
    test(`${language}: Escape closes the popup with the pointer on what it shows for`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      const value = DOCS[language];
      const line = value.split('\n').findIndex((text) => text.includes('port')) + 1;
      const problem = {
        range: { startLine: line, startColumn: 1, endColumn: value.split('\n')[line - 1].length + 1 },
        severity: 'warning',
        message: 'Checked by the host',
      };
      for (const [options, popup] of [
        [{ schema }, '.config-editor__hover'],
        [{ hostDiagnostics: [problem] }, '.config-editor__problem-tip'],
      ]) {
        await page.goto(pageUrl({ value, language, ...options }));
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        const port = await page.evaluate(() => {
          const row = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')].find((row) =>
            row.textContent.includes('port'),
          );
          // The middle of `port`'s second letter.
          let left = row.textContent.indexOf('port') + 1;
          const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
          let node;
          while ((node = walker.nextNode()) && left >= node.length) left -= node.length;
          const range = document.createRange();
          range.setStart(node, left);
          range.setEnd(node, left + 1);
          const box = range.getBoundingClientRect();
          return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
        });
        await page.mouse.move(port.x, port.y);
        await expect(page.locator(popup)).toBeVisible();
        expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
        await page.keyboard.press('Escape');
        await expect(page.locator(popup)).toBeHidden();
      }
      expect(errors).toEqual([]);
    });
  }
});
