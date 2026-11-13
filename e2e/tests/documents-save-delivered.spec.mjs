import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// `saveAll()` and `save(id)` answer only for what reached `onSave`: a save
// whose host callback is gone (the view destroyed, or `onSave` removed, by
// an earlier save's callback) is not counted as sent (round 6's
// `D6.host-1`).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const SHAPES = {
  'destroys the view': 'window.view.destroy()',
  'removes onSave': 'window.view.update({ onSave: null })',
};

test.describe('saves counted as sent reached the host', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, inSave] of Object.entries(SHAPES)) {
    test(`saveAll when the first onSave ${name}`, async ({ page }) => {
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const found = await page.evaluate(async (inSave) => {
        const module = await import('../config-editor/index.js');
        window.editor?.destroy();
        const host = document.createElement('div');
        document.body.prepend(host);
        const heard = [];
        window.view = new module.ConfigDocuments(host, {
          documents: ['main', 'limits', 'tls'].map((id) => ({ id, fileName: `${id}.yaml`, value: 'a: 2\n', baseline: 'a: 1\n' })),
          onSave: (id) => {
            heard.push(id);
            if (heard.length === 1) new Function(inSave)();
          },
        });
        const saved = window.view.saveAll();
        return { saved, heard };
      }, inSave);
      expect(found.saved).toEqual(found.heard);
      expect(found.heard).toEqual(['main']);
    });
  }
});
