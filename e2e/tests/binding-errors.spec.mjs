import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A host `onError` that throws is the host's bug: the binding every
// framework wrapper builds on logs it and keeps the editor (round 5's
// `D5.host-1`: in React the throw escaped and unmounted the editor, or a
// documents view with every document).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('a throwing onError', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const kind of ['EditorBinding', 'DocumentsBinding']) {
    test(`${kind}: is logged, and the editor stays`, async ({ page }) => {
      const logged = [];
      page.on('console', (message) => message.type() === 'error' && logged.push(message.text()));
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const found = await page.evaluate(async (kind) => {
        window.editor?.destroy();
        const bindings = await import('../config-editor/binding.js');
        const host = document.createElement('div');
        document.body.prepend(host);
        const props =
          kind === 'EditorBinding'
            ? { value: 'a: 1\n' }
            : { documents: [{ id: 'main', fileName: 'main.yaml', value: 'a: 1\n' }] };
        const binding = new bindings[kind]('Test', {
          ...props,
          onError: () => {
            throw new Error('handler bug');
          },
        });
        binding.attach(host);
        await new Promise((resolve) => setTimeout(resolve, 300));
        let escaped = null;
        try {
          binding.update({ ...binding.props, theme: 'not-a-theme' });
        } catch (error) {
          escaped = String(error);
        }
        await new Promise((resolve) => setTimeout(resolve, 300));
        return { escaped, inputs: host.querySelectorAll('.config-editor__input').length };
      }, kind);
      expect(found.escaped).toBeNull();
      expect(found.inputs).toBeGreaterThan(0);
      expect(logged.join('\n')).toContain('handler bug');
    });
  }
});
