import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The preset stylesheet is on a page once, however many editors and
// documents views show there (QA D7.perf-2): a copy per editor is a
// stylesheet the browser matches every element against, so with 100
// editors live the 100th mounted 3× slower than the first (its style pass
// 30×), and each copy added or removed restyled the whole page. One
// `<style>` per root (the page's head, a shadow root) is shared, styles
// each view from the moment it is built, follows `styled` on and off, and
// goes with the last view; a view built on no page keeps a copy of its own.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

/** In the page: `window.copies(root)` counts the preset's `<style>`s in
 * `root` (the document or a shadow root, editors' insides included), and
 * `window.styled(element)` says whether the preset styles an editor's root. */
async function open(page) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
  await page.goto(VANILLA);
  await page.waitForFunction(() => document.body.dataset.ready === 'true');
  await page.evaluate(async () => {
    window.CE = await import(new URL('../config-editor/index.js', location.href).href);
    const { presetCss } = await import(new URL('../config-editor/tools.js', location.href).href);
    const preset = presetCss();
    window.copies = (root = document) => [...root.querySelectorAll('style')].filter((style) => style.textContent === preset).length;
    window.styled = (element) => getComputedStyle(element).getPropertyValue('--config-editor-mono').trim() !== '';
    window.host = (parent = document.body) => {
      const host = document.createElement('div');
      host.style.height = '160px';
      parent.append(host);
      return host;
    };
  });
}

const frames = (page) => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

test.describe('The preset stylesheet, once per page', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  test('one copy for many editors and documents views, styled at once, gone with the last', async ({ page }) => {
    await open(page);
    // The example's own editor.
    expect(await page.evaluate(() => window.copies())).toBe(1);
    const counts = await page.evaluate(() => {
      window.views = [];
      const seen = [];
      for (let index = 0; index < 6; index += 1) {
        const host = window.host();
        const view =
          index % 3 === 2
            ? new window.CE.ConfigDocuments(host, { documents: [{ id: `d${index}`, fileName: 'a.yaml', value: 'a: 1\n' }] })
            : new window.CE.ConfigEditor(host, { language: 'yaml', value: `n: ${index}\n` });
        // Styled before the host's next line runs.
        seen.push(window.styled(host.firstElementChild));
        window.views.push(view);
      }
      return seen;
    });
    expect(counts).toEqual([true, true, true, true, true, true]);
    await frames(page);
    expect(await page.evaluate(() => window.copies())).toBe(1);
    expect(await page.evaluate(() => document.head.querySelectorAll('style[data-config-editor-preset]').length)).toBe(1);
    // Any but the last going leaves it; the last takes it.
    await page.evaluate(() => window.views.splice(0).forEach((view) => view.destroy()));
    await frames(page);
    expect(await page.evaluate(() => window.copies())).toBe(1);
    await page.evaluate(() => window.editor.destroy());
    await frames(page);
    expect(await page.evaluate(() => window.copies())).toBe(0);
    // And a new first editor brings it back.
    const styled = await page.evaluate(() => {
      const host = window.host();
      window.again = new window.CE.ConfigEditor(host, { language: 'json', value: '{}' });
      return window.styled(host.firstElementChild);
    });
    expect(styled).toBe(true);
    await frames(page);
    expect(await page.evaluate(() => window.copies())).toBe(1);
    expect(page.errors).toEqual([]);
  });

  test('a shadow root has its own copy, shared by the editors in it', async ({ page }) => {
    await open(page);
    await page.evaluate(() => {
      const outer = window.host();
      window.shadow = outer.attachShadow({ mode: 'open' });
      window.inShadow = [0, 1, 2].map((index) => new window.CE.ConfigEditor(window.host(window.shadow), { language: 'yaml', value: `n: ${index}\n` }));
    });
    await frames(page);
    const state = () =>
      page.evaluate(() => ({
        shadow: window.copies(window.shadow),
        page: window.copies(),
        styled: [...window.shadow.querySelectorAll('.config-editor')].map(window.styled),
      }));
    expect(await state()).toEqual({ shadow: 1, page: 1, styled: [true, true, true] });
    await page.evaluate(() => window.inShadow.forEach((editor) => editor.destroy()));
    await frames(page);
    expect(await state()).toEqual({ shadow: 0, page: 1, styled: [] });
    expect(page.errors).toEqual([]);
  });

  test('styled switched off and on in each view: the copy stays while one wants it', async ({ page }) => {
    await open(page);
    await page.evaluate(() => {
      window.second = new window.CE.ConfigEditor(window.host(), { language: 'yaml', value: 'a: 1\n' });
    });
    await frames(page);
    const copies = async () => {
      await frames(page);
      return page.evaluate(() => window.copies());
    };
    await page.evaluate(() => window.editor.update({ styled: false }));
    expect(await copies()).toBe(1);
    await page.evaluate(() => window.second.update({ styled: false }));
    expect(await copies()).toBe(0);
    expect(await page.evaluate(() => window.styled(document.querySelector('.config-editor')))).toBe(false);
    await page.evaluate(() => window.second.update({ styled: true }));
    expect(await copies()).toBe(1);
    await page.evaluate(() => window.editor.update({ styled: true }));
    expect(await copies()).toBe(1);
    await page.evaluate(() => window.second.destroy());
    expect(await copies()).toBe(1);
    await page.evaluate(() => window.editor.destroy());
    expect(await copies()).toBe(0);
    expect(page.errors).toEqual([]);
  });

  test('an editor built on no page keeps its own copy, styled once attached', async ({ page }) => {
    await open(page);
    await page.evaluate(() => {
      window.loose = document.createElement('div');
      window.detached = new window.CE.ConfigEditor(window.loose, { language: 'yaml', value: 'a: 1\n' });
    });
    await frames(page);
    expect(await page.evaluate(() => window.copies(window.loose))).toBe(1);
    await page.evaluate(() => document.body.append(window.loose));
    await frames(page);
    expect(await page.evaluate(() => window.styled(window.loose.firstElementChild))).toBe(true);
    await page.evaluate(() => window.detached.destroy());
    expect(await page.evaluate(() => [window.copies(window.loose), window.copies()])).toEqual([0, 1]);
    expect(page.errors).toEqual([]);
  });
});
