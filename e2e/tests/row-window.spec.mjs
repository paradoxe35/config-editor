import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// An editor renders the rows in its own view and a screen's worth more each
// way, not a band sized by the window: a 200 px editor on a 720 px window
// rendered ~113 rows for its ~9 visible.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;
const TEXT = Array.from({ length: 8000 }, (_, index) => `key_${index}: value ${index}`).join('\n') + '\n';

test.describe('the rows an editor renders', () => {
  test.skip(!built, 'the npm package is not built: cd js && npm run build');

  for (const height of [200, 600]) {
    test(`a ${height} px editor renders about three screens of rows, at the top and scrolled`, async ({ page }) => {
      await page.goto(VANILLA);
      await page.waitForFunction(() => document.body.dataset.ready === 'true');
      await page.evaluate(
        async ({ text, height }) => {
          const CE = await import(new URL('../config-editor/index.js', location.href).href);
          const host = document.createElement('div');
          host.id = 'sized';
          host.style.height = `${height}px`;
          document.body.prepend(host);
          window.sized = new CE.ConfigEditor(host, { language: 'yaml', value: text });
        },
        { text: TEXT, height },
      );
      const sized = page.locator('#sized');
      const input = sized.locator('.config-editor__input');
      await input.waitFor();
      const measure = () =>
        sized.evaluate((host) => {
          const input = host.querySelector('.config-editor__input');
          const row = parseFloat(getComputedStyle(input).lineHeight);
          return {
            inView: Math.ceil(input.clientHeight / row),
            rendered: host.querySelectorAll('.config-editor__gutter-content > [data-line]').length,
          };
        });
      for (const scroll of [0, 40000]) {
        await input.evaluate((element, top) => {
          element.scrollTop = top;
          element.dispatchEvent(new Event('scroll'));
        }, scroll);
        await page.waitForTimeout(200);
        const { inView, rendered } = await measure();
        const overscan = Math.min(Math.max(inView, 10), 40);
        expect(rendered, `scrolled ${scroll}: ${inView} rows in view`).toBeLessThanOrEqual(inView + 2 * overscan + 2);
        expect(rendered, `scrolled ${scroll}: ${inView} rows in view`).toBeGreaterThanOrEqual(inView);
      }
    });
  }
});
