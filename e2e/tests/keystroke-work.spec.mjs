import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The work a keystroke costs a host that keeps its documents' list and hands
// it whole to `update` after each edit (`onChange` → `update({ documents })`):
// counted, not timed, so each count is the same on any machine.
//
// Copies of large strings into the module are counted at the glue's
// `TextEncoder.encodeInto`, which copies whatever a string holds from its
// first non-ASCII character on: each test text starts with one.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('ConfigDocuments.update after each keystroke', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('copies only the documents (and schemas) that changed into the module', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const copies = await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.prepend(host);
      const LARGE = 50_000;
      const text = (n) => `# é ${n}\n${Array.from({ length: 4000 }, (_, i) => `key${i}: value ${n}`).join('\n')}\n`;
      const schema = JSON.stringify({
        description: `é ${'x'.repeat(LARGE)}`,
        type: 'object',
      });
      let files = Array.from({ length: 10 }, (_, n) => ({ id: `d${n}`, fileName: `d${n}.yaml`, value: text(n), schema }));
      const view = new module.ConfigDocuments(host, { documents: files });
      let copied = 0;
      const encodeInto = TextEncoder.prototype.encodeInto;
      TextEncoder.prototype.encodeInto = function (source, into) {
        if (source.length > LARGE) copied += 1;
        return encodeInto.call(this, source, into);
      };
      const count = (run) => {
        copied = 0;
        run();
        return copied;
      };
      const same = count(() => view.update({ documents: files }));
      const copiedList = count(() => view.update({ documents: files.map((file) => ({ ...file })) }));
      files = files.map((file) => (file.id === 'd0' ? { ...file, value: `${file.value}more: 1\n` } : file));
      const oneEdited = count(() => view.update({ documents: files }));
      files = files.map((file) => (file.id === 'd5' ? { ...file, baseline: file.value } : file));
      const baselineAdded = count(() => view.update({ documents: files }));
      const baselineKept = count(() => view.update({ documents: files }));
      TextEncoder.prototype.encodeInto = encodeInto;
      const edited = view.editor('d0').value.endsWith('more: 1\n');
      view.destroy();
      return { same, copiedList, oneEdited, baselineAdded, baselineKept, edited };
    });
    expect(copies).toEqual({ same: 0, copiedList: 0, oneEdited: 1, baselineAdded: 1, baselineKept: 0, edited: true });
  });

  // A framework host keeps the list in its state and renders the view with
  // it again after each edit: the binding hands it whole to `update`.
  for (const name of ['documents-react.html', 'documents-vue.html', 'documents-svelte.html']) {
    test(`${name}: a keystroke re-sending every document copies none of them`, async ({ page }) => {
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(() => {
        const LARGE = 50_000;
        const text = (n) => `# é ${n}\n${Array.from({ length: 4000 }, (_, i) => `key${i}: value ${n}`).join('\n')}\n`;
        window.updateFile('main', { value: text(0) });
        window.updateFile('limits', { value: `# é\n[limits]\n${'#'.repeat(LARGE)}\nmax_pending = 512\n` });
        window.copied = 0;
        const encodeInto = TextEncoder.prototype.encodeInto;
        TextEncoder.prototype.encodeInto = function (source, into) {
          if (source.length > LARGE) window.copied += 1;
          return encodeInto.call(this, source, into);
        };
      });
      const input = page.locator('.config-editor__input:visible');
      await expect.poll(() => input.inputValue()).toContain('key3999: value 0');
      await input.click();
      await page.waitForTimeout(300);
      await page.evaluate(() => (window.copied = 0));
      await page.keyboard.type('x');
      await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100))));
      // The text area's text, read for the edit and to check that it shows
      // the document; neither the echo nor the other documents are copied.
      expect(await page.evaluate(() => window.copied)).toBe(2);
    });
  }

  test('takes back the text it handed to onChange without copying it again', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(async () => {
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      host.id = 'typed';
      document.body.prepend(host);
      const LARGE = 50_000;
      const text = (n) => `# é ${n}\n${Array.from({ length: 4000 }, (_, i) => `key${i}: value ${n}`).join('\n')}\n`;
      let files = Array.from({ length: 3 }, (_, n) => ({ id: `d${n}`, fileName: `d${n}.yaml`, value: text(n) }));
      const view = new module.ConfigDocuments(host, {
        documents: files,
        onChange: (id, value) => {
          files = files.map((file) => (file.id === id ? { ...file, value } : file));
          view.update({ documents: files });
        },
      });
      window.copied = 0;
      const encodeInto = TextEncoder.prototype.encodeInto;
      TextEncoder.prototype.encodeInto = function (source, into) {
        if (source.length > LARGE) window.copied += 1;
        return encodeInto.call(this, source, into);
      };
      window.typedView = view;
      window.typedLength = files[0].value.length;
    });
    const input = page.locator('#typed .config-editor__input');
    await input.click();
    await page.keyboard.press('End');
    await page.evaluate(() => (window.copied = 0));
    await page.keyboard.type('x');
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50))));
    // The text area's text, read for the edit and to check that it shows
    // the document; the host's echo of it is not copied.
    expect(await page.evaluate(() => window.copied)).toBe(2);
    expect(await page.evaluate(() => window.typedView.editor('d0').value.length - window.typedLength)).toBe(1);
  });
});
