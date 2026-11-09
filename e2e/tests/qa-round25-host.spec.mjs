import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 25 (host API): `height: 100%` fills its element both ways (a
// short one: the text area shrinks and the footer stays inside; a tall one:
// no band past the window's 80%), for an editor and a documents view.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

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

test.describe('QA round 25: host API', () => {
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

  // A document never shown answered `reveal` right after `select` (or an
  // `activeId` update) with false: its editor mounted a moment later.
  for (const how of ['select', 'update']) {
    for (const call of ['reveal', 'revealRange', 'rangeOf']) {
      test(`${call} right after ${how} on a document never shown works`, async ({ page }) => {
        await page.goto(example('vanilla.html'));
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        const answer = await page.evaluate(
          async ([how, call, big]) => {
            const module = await import('../config-editor/index.js');
            const host = document.createElement('div');
            host.id = 'sized';
            document.body.prepend(host);
            const view = new module.ConfigDocuments(host, {
              documents: [
                { id: 'a', fileName: 'a.yaml', value: big },
                { id: 'b', fileName: 'b.yaml', value: big },
              ],
            });
            window.sized = view;
            await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
            if (how === 'select') view.select('b');
            else view.update({ activeId: 'b' });
            const editor = view.editor('b');
            if (call === 'reveal') return editor.reveal('/k150');
            const range = editor.rangeOf('/k150');
            if (call === 'rangeOf') return range !== null;
            return editor.revealRange(range);
          },
          [how, call, big],
        );
        expect(answer).toBe(true);
        if (call === 'rangeOf') return;
        const b = page.locator('#sized [role="tabpanel"][data-document-id="b"] .config-editor__input');
        await expect(b).toBeFocused();
        const caretLine = await b.evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
        expect(caretLine).toBe(151);
      });
    }
  }

  // The host showing another document while the user typed in one dropped
  // the focus to the page.
  test('the host showing another document keeps the focus in the view', async ({ page }) => {
    await page.goto(example('documents.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const main = page.locator('[role="tabpanel"][data-document-id="main"] .config-editor__input');
    await main.focus();
    await page.evaluate(() => window.documentsView.update({ activeId: 'limits' }));
    await expect(page.locator('[role="tabpanel"][data-document-id="limits"] .config-editor__input')).toBeFocused();
    // From outside the view, the focus stays where it was.
    await page.evaluate(() => {
      const outside = document.createElement('button');
      outside.id = 'outside';
      document.body.append(outside);
      outside.focus();
      window.documentsView.update({ activeId: 'main' });
    });
    await page.waitForTimeout(200);
    await expect(page.locator('#outside')).toBeFocused();
  });

  // Tabs sharing a file's name showed their folders' starts (alike) or cut
  // the file's name: each shows the end of its folder that tells it apart,
  // then the whole file's name, at phone width too.
  for (const width of [360, 1200]) {
    test(`tabs sharing a file's name show what tells them apart (${width} px)`, async ({ page }) => {
      await page.setViewportSize({ width, height: 400 });
      await page.goto(example('documents.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(() =>
        window.documentsView.update({
          documents: [
            { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: '' },
            { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: '' },
            { id: 'c', fileName: 'prod/limits.toml', value: '' },
            { id: 'd', fileName: 'staging/eu/limits.toml', value: '' },
          ],
        }),
      );
      await expect(page.locator('[role="tab"]')).toHaveCount(4);
      const tabs = await page.evaluate(() =>
        [...document.querySelectorAll('[role="tab"]')].map((tab) => {
          const name = tab.querySelector('.config-editor-documents__name').getBoundingClientRect();
          const end = tab.querySelector('.config-editor-documents__name-end');
          const start = tab.querySelector('.config-editor-documents__name-start');
          const box = end.getBoundingClientRect();
          return {
            start: start.textContent,
            end: end.textContent,
            whole: box.left >= name.left - 0.5 && box.right <= name.right + 0.5,
            // Cut or whole, the start shows more than one letter.
            shown: start.clientWidth >= 14,
          };
        }),
      );
      expect(tabs.map(({ start, end }) => start + end)).toEqual([
        'eu-west-1/production/limits.toml',
        'us-east-1/production/limits.toml',
        'prod/limits.toml',
        'eu/limits.toml',
      ]);
      expect(tabs.every(({ end, whole }) => end === 'limits.toml' && whole)).toBe(true);
      expect(tabs.every(({ shown }) => shown)).toBe(true);
    });
  }

  // The constructor took a `null` value that `update` refused; a document
  // could have the id "", and a documents view kept a schema that does not
  // compile where a ConfigEditor throws.
  test('options are read alike: a null text, an empty id and a bad schema throw', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.prepend(host);
      const attempt = (run) => {
        try {
          const made = run();
          made?.destroy?.();
          return 'no error';
        } catch (error) {
          return String(error.message ?? error);
        }
      };
      const view = (documents) => new module.ConfigDocuments(host, { documents });
      const doc = { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' };
      const out = {
        editorValue: attempt(() => new module.ConfigEditor(host, { value: null })),
        editorBaseline: attempt(() => new module.ConfigEditor(host, { value: 'a: 1\n', baseline: null })),
        editorSchema: attempt(() => new module.ConfigEditor(host, { schema: '{bad' })),
        emptyId: attempt(() => view([{ ...doc, id: '' }])),
        documentsSchema: attempt(() => view([{ ...doc, schema: '{bad' }])),
      };
      // An update with a bad schema throws and changes nothing; the same
      // schema text again keeps the document's editor.
      const schema = JSON.stringify({ type: 'object', properties: { a: { type: 'integer' } } });
      const shown = view([{ ...doc, schema }]);
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
      const input = host.querySelector('.config-editor__input');
      out.updateSchema = attempt(() => shown.update({ documents: [{ ...doc, schema: '{bad' }] }));
      shown.update({ documents: [{ ...doc, value: 'a: 2\n', schema }] });
      await new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100)));
      out.kept = host.querySelector('.config-editor__input') === input;
      out.value = shown.editor('a').value;
      shown.destroy();
      host.remove();
      return out;
    });
    expect(result.editorValue).toContain('value must be a string');
    expect(result.editorBaseline).toContain('baseline must be a string');
    expect(result.editorSchema).not.toBe('no error');
    expect(result.emptyId).toContain('documents[0].id must not be empty');
    expect(result.documentsSchema).toContain('documents[0].schema');
    expect(result.updateSchema).toContain('documents[0].schema');
    expect(result.kept).toBe(true);
    expect(result.value).toBe('a: 2\n');
  });
});
