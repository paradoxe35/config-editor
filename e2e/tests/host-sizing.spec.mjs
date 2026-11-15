import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Sizing: height 100% fills the element both ways, --config-editor-height fixes the text area.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

const js = join(root, 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
  });
}

const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;

// Mounts a `kind` ('editor' or 'documents') with toolbar and footer in a
// `#sized` element `height` tall, filled with `height: 100%`; returns the
// boxes of the element, the editor, its text area and its footer.
async function filled(page, kind, height, value) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  return page.evaluate(
    async ([kind, height, value]) => {
      const module = await import('../config-editor/index.js');
      const style = document.createElement('style');
      style.textContent = `#sized { height: ${height}; } #sized .config-editor, #sized .config-editor-documents { height: 100%; }`;
      document.head.append(style);
      const host = document.createElement('div');
      host.id = 'sized';
      document.body.prepend(host);
      const options = { showToolbar: true, showFooter: true };
      window.sized =
        kind === 'editor'
          ? new module.ConfigEditor(host, { value, ...options })
          : new module.ConfigDocuments(host, {
              documents: [
                { id: 'a', fileName: 'a.yaml', value },
                { id: 'b', fileName: 'b.yaml', value },
              ],
              ...options,
            });
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 150)));
      const box = (element) => {
        const rect = element.getBoundingClientRect();
        return { top: Math.round(rect.top), bottom: Math.round(rect.bottom) };
      };
      const editor = [...host.querySelectorAll('.config-editor')].find((element) => element.checkVisibility());
      return {
        container: box(host),
        editor: box(editor),
        input: box(editor.querySelector('.config-editor__input')),
        footer: box(editor.querySelector('.config-editor__status')),
      };
    },
    [kind, height, value],
  );
}

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test('the editor sizes itself as the READMEs say: height 100% fills its element, --config-editor-height fixes the text area', async ({ page }) => {
    await open(page);
    const heights = await page.evaluate(async () => {
      const host = document.createElement('div');
      host.id = 'sized';
      document.body.prepend(host);
      const style = document.createElement('style');
      document.head.append(style);
      const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;
      const out = {};
      for (const [name, css] of [
        ['fill', '#sized { height: 300px; } #sized .config-editor { height: 100%; }'],
        ['fixed', '#sized .config-editor { --config-editor-height: 400px; }'],
      ]) {
        style.textContent = css;
        out[name] = [];
        for (const value of ['a: 1\n', big]) {
          const editor = new window.module.ConfigEditor(host, { value, showToolbar: true, showFooter: true });
          await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
          const of = (selector) => Math.round(host.querySelector(selector).getBoundingClientRect().height);
          out[name].push(name === 'fill' ? of('.config-editor') : of('.config-editor__surface'));
          editor.destroy();
        }
      }
      style.remove();
      host.remove();
      return out;
    });
    expect(heights).toEqual({ fill: [300, 300], fixed: [400, 400] });
  });
});

// QA round 24 (host API): a constructor takes a nested `undefined` in
// `colors` and `strings` as `update` does (the entry takes its default),
// in ConfigEditor and ConfigDocuments alike.
test.describe('host API', () => {
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
});

// QA round 25 (host API): `height: 100%` fills its element both ways (a
// short one: the text area shrinks and the footer stays inside; a tall one:
// no band past the window's 80%), for an editor and a documents view.
test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  for (const kind of ['editor', 'documents']) {
    for (const height of ['150px', '250px', '1200px']) {
      test(`height 100% fills a ${height} element both ways (${kind})`, async ({ page }) => {
        await page.setViewportSize({ width: 900, height: 700 });
        const boxes = await filled(page, kind, height, big);
        // The editor ends at the element's end, its footer inside it, the
        // text area above the footer.
        expect(Math.abs(boxes.editor.bottom - boxes.container.bottom)).toBeLessThanOrEqual(1);
        expect(Math.abs(boxes.footer.bottom - boxes.container.bottom)).toBeLessThanOrEqual(2);
        expect(boxes.input.bottom).toBeLessThanOrEqual(boxes.footer.top + 1);
        // The end of the text is reached inside the text area: nothing
        // around it scrolls.
        await page.locator('#sized .config-editor__input:visible').focus();
        await page.keyboard.press('Control+End');
        const scrolled = await page.evaluate(() =>
          [...document.querySelectorAll('#sized, #sized *')]
            .filter((element) => !element.matches('.config-editor__input, .config-editor__input *, .config-editor-documents__tabs, .config-editor-documents__tabs *'))
            .filter((element) => element.scrollTop > 0)
            .map((element) => element.className),
        );
        expect(scrolled).toEqual([]);
        // In an element taller than the window, the page shows the caret.
        if (boxes.container.bottom <= 700) expect(await page.evaluate(() => window.scrollY)).toBe(0);
      });
    }

    test(`without a height the ${kind} keeps its minimum and its 80% of the window`, async ({ page }) => {
      await page.setViewportSize({ width: 900, height: 700 });
      const short = await filled(page, kind, 'auto', 'a: 1\n');
      expect(short.input.bottom - short.input.top).toBeGreaterThanOrEqual(239);
      const tall = await filled(page, kind, 'auto', big);
      expect(tall.input.bottom - tall.input.top).toBeLessThanOrEqual(Math.ceil(0.8 * 700));
    });
  }
});
