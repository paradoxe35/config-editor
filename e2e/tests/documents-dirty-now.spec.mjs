import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A shown document's dirty state follows `update({ documents })` in the same
// task, as a hidden one's does: `isDirty` and `saveAll()` right after the
// update act on the new text and baseline (round 15's `D15.host-1`: one
// microtask late, so a same-task `saveAll()` saved the stale set).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

async function mount(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.saved = [];
    window.view = new module.ConfigDocuments(host, {
      documents: [{ id: 'main', fileName: 'main.yaml', value: 'a: 1\n', baseline: 'a: 1\n' }],
      onSave: (id, text) => window.saved.push([id, text]),
    });
  });
  await page.locator('#qa .config-editor__input').waitFor();
}

test.describe('dirty right after update({ documents })', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a new text makes the shown document dirty in the same task', async ({ page }) => {
    await mount(page);
    const found = await page.evaluate(() => {
      window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value: 'a: 2\n', baseline: 'a: 1\n' }] });
      return { dirty: window.view.isDirty('main'), saved: window.view.saveAll(), sent: window.saved };
    });
    expect(found).toEqual({ dirty: true, saved: ['main'], sent: [['main', 'a: 2\n']] });
  });

  test('the baseline text again makes it clean in the same task', async ({ page }) => {
    await mount(page);
    const input = page.locator('#qa .config-editor__input');
    await input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(4, 4);
    });
    await page.keyboard.type('9');
    await expect.poll(() => page.evaluate(() => window.view.isDirty('main'))).toBe(true);
    const found = await page.evaluate(() => {
      window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value: 'a: 1\n', baseline: 'a: 1\n' }] });
      return { dirty: window.view.isDirty('main'), saved: window.view.saveAll() };
    });
    expect(found).toEqual({ dirty: false, saved: [] });
  });
});
