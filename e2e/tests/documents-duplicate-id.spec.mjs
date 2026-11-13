import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A repeated document id is ignored (with a warning): every answer about
// the id is the first listed document's, before and after it is shown
// (round 3's `D3.host-1`: `value`, `unifiedDiff()` and `isDirty` answered
// the ignored duplicate while `text` and `save()` answered the first).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('a repeated document id', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const shown of [false, true]) {
    test(`answers the first document${shown ? ', shown' : ''}`, async ({ page }) => {
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const found = await page.evaluate(async (shown) => {
        const module = await import('../config-editor/index.js');
        window.editor?.destroy();
        const host = document.createElement('div');
        document.body.prepend(host);
        const saved = [];
        const view = new module.ConfigDocuments(host, {
          documents: [
            { id: 'main', fileName: 'main.yaml', value: 'a: 1\n' },
            { id: 'dup', fileName: 'dup.yaml', value: 'first: 2\n', baseline: 'first: 1\n' },
            { id: 'dup', fileName: 'dup2.yaml', value: 'second: 1\n', baseline: 'second: 1\n' },
          ],
          onSave: (id, text) => saved.push(text),
        });
        if (shown) view.select('dup');
        await new Promise((resolve) => setTimeout(resolve, 200));
        const editor = view.editor('dup');
        view.save('dup');
        return { value: editor.value, dirty: view.isDirty('dup'), diff: editor.unifiedDiff().includes('+first: 2'), saved };
      }, shown);
      expect(found).toEqual({ value: 'first: 2\n', dirty: true, diff: true, saved: ['first: 2\n'] });
    });
  }
});
