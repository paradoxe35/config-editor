import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 24 (host API): a constructor takes a nested `undefined` in
// `colors` and `strings` as `update` does (the entry takes its default),
// in ConfigEditor and ConfigDocuments alike.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('QA round 24: host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // The editor's own box filled its element, but a long text ran its text
  // area and footer past it, and a short one left a gap under the footer.
  test('height 100% fills the element: the text area takes what the toolbar and footer leave', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const style = document.createElement('style');
      style.textContent = '#sized { height: 450px; } #sized .config-editor { height: 100%; }';
      document.head.append(style);
      const host = document.createElement('div');
      host.id = 'sized';
      document.body.prepend(host);
      const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;
      const out = [];
      for (const value of ['a: 1\n', big]) {
        const editor = new module.ConfigEditor(host, { value, showToolbar: true, showFooter: true });
        await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
        const bottom = (selector) => Math.round(document.querySelector(selector).getBoundingClientRect().bottom);
        const input = host.querySelector('.config-editor__input');
        out.push({
          footer: bottom('#sized .config-editor__status') - bottom('#sized'),
          frame: bottom('#sized .config-editor__frame') - bottom('#sized'),
          scrolls: input.scrollHeight > input.clientHeight,
        });
        editor.destroy();
      }
      return out;
    });
    expect(result.map(({ footer, frame }) => [Math.abs(footer) <= 2, frame])).toEqual([[true, 0], [true, 0]]);
    expect(result[1].scrolls).toBe(true);
  });

  test('a constructor takes a nested undefined or null in colors and strings as its default', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.prepend(host);
      const settings = {
        colors: { key: undefined, string: '#ff0000', number: null },
        strings: { noProblems: undefined, tabList: 'Dateien' },
      };
      const out = [];
      const editor = new module.ConfigEditor(host, { value: 'a: 1\n', ...settings });
      out.push([editor.settings.colors, editor.settings.strings.tabList, editor.settings.strings.noProblems]);
      editor.destroy();
      const view = new module.ConfigDocuments(host, {
        documents: [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings }],
        ...settings,
      });
      out.push([view.settings.colors, view.settings.strings.tabList, view.settings.strings.noProblems]);
      view.destroy();
      host.remove();
      return out;
    });
    for (const [colors, tabList, noProblems] of result) {
      expect(colors).toEqual({ string: '#ff0000' });
      expect(tabList).toBe('Dateien');
      expect(typeof noProblems).toBe('string');
    }
  });
});
