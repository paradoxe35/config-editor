import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Typing in a row of a million characters: the code layer draws again only
// the pieces of the row the keystroke changed, not the row (counted, not
// timed: the characters the keystrokes put into the page's elements), and
// the row drawn in pieces lies where the row drawn whole would.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const LINES = {
  yaml: (value) => `k: "${value}"\n`,
  json: (value) => `{"k": "${value}"}\n`,
  toml: (value) => `k = "${value}"\n`,
};

/** Mounts a view of `text`, types five keys at the end of its value and
 * returns the characters drawn into the code layer meanwhile, and how the
 * row lies. */
async function typeAtValueEnd(page, language, text) {
  return page.evaluate(
    async ({ language, text }) => {
      const { ConfigEditor } = await import('../config-editor/index.js');
      window.view?.destroy();
      document.getElementById('long-row')?.remove();
      const host = document.createElement('div');
      host.id = 'long-row';
      host.style.cssText = 'height: 400px; width: 900px';
      document.body.prepend(host);
      window.view = new ConfigEditor(host, { language, value: text, schema: '{}' });
      const frame = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      await frame();
      const input = host.querySelector('.config-editor__input');
      const at = input.value.lastIndexOf('a') + 1;
      input.focus();
      input.setSelectionRange(at, at);
      await frame();
      const layer = host.querySelector('.config-editor__highlight');
      let drawn = 0;
      const observer = new MutationObserver((records) => {
        for (const record of records) {
          if (record.type === 'characterData') drawn += record.target.data.length;
          for (const node of record.addedNodes) drawn += node.textContent.length;
        }
      });
      observer.observe(layer, { subtree: true, childList: true, characterData: true });
      for (let key = 0; key < 5; key += 1) {
        document.execCommand('insertText', false, 'x');
        await frame();
      }
      observer.disconnect();
      const row = layer.querySelector('.config-editor__code-line[data-line="1"]');
      const first = input.value.split('\n')[0];
      // The row drawn whole, unseen, in the same layer.
      const whole = document.createElement('span');
      whole.style.cssText = 'position: absolute; visibility: hidden; white-space: pre';
      whole.textContent = first;
      layer.append(whole);
      const width = (element) => element.getBoundingClientRect().width;
      const lies = {
        sameText: row.textContent === first,
        pieces: row.querySelectorAll('.config-editor__code-piece').length,
        widthApart: Math.abs(width(row.firstElementChild ?? row) - width(whole)),
      };
      if (lies.pieces > 0) {
        let sum = 0;
        for (const piece of row.querySelectorAll('.config-editor__code-piece')) sum += width(piece);
        lies.widthApart = Math.abs(sum - width(whole));
      }
      whole.remove();
      return { drawn, typed: input.value.length - text.length, ...lies };
    },
    { language, text },
  );
}

test.describe('typing in a long row', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [language, line] of Object.entries(LINES)) {
    test(`${language}: a keystroke draws the pieces it changed, at 100k and 1M characters alike`, async ({ page }) => {
      test.setTimeout(120_000);
      await page.goto(example);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      for (const length of [100_000, 1_000_000]) {
        const result = await typeAtValueEnd(page, language, line('a'.repeat(length)));
        expect(result.typed, `${length}`).toBe(5);
        expect(result.sameText, `${length}`).toBe(true);
        expect(result.pieces, `${length}`).toBeGreaterThan(10);
        // Five keystrokes, each drawing again at most two pieces of 8 KiB.
        expect(result.drawn, `${length}: characters drawn`).toBeLessThan(5 * 2 * 8192);
        expect(result.widthApart, `${length}: px between the pieces' width and the row's`).toBeLessThan(1);
      }
    });
  }
});
