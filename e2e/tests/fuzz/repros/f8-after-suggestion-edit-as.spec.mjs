import { expect, test } from '@playwright/test';
import { BUILT, PAGES, installProbe, pageUrl } from '../pages.mjs';
import { host } from '../perform.mjs';
import { replay } from '../runner.mjs';

// F8 right after a suggestion accepted in an Edit-as view puts the caret on
// the problem the editor lists (shrunk by the browser fuzz from seed
// 3900000068 on documents-svelte and documents-vue), on every page.
test.describe('F8 after a suggestion in an Edit-as view', () => {
  test.skip(!BUILT, 'the example pages are not built (npm run build in js/)');
  for (const name of Object.keys(PAGES)) {
    test(`${name}: F8 lands on the problem`, async ({ browser }) => {
      await replay(
        browser,
        name,
        '?random',
        [
          { k: 'editAs', language: 'xml' },
          { k: 'suggest', open: true, pick: 2 },
          { k: 'f8', shift: false, n: 2 },
        ],
        'f8',
      );
    });
  }
  // The cause, without the timing: the fuzz's host asked for the language
  // with the handle only, so its next list (the echo of an edit, with no
  // `editAs`) switched back to the document's own language, under the
  // caret F8 had just placed. A host keeps the language in its list.
  for (const name of Object.keys(PAGES).filter((page) => PAGES[page].kind === 'documents')) {
    test(`${name}: the fuzz host's switch stays through the echo of an edit`, async ({ page }) => {
      await page.goto(pageUrl(name));
      await page.waitForFunction(() => document.body.dataset.ready === 'true', null, { timeout: 20000 });
      const input = page.locator('.config-editor__input').first();
      await input.waitFor();
      await page.evaluate(installProbe, PAGES[name]);
      await page.waitForFunction(() => !!window.__fz.editor());
      await host(page, 'editAs', 'xml');
      const frame = page.locator('.config-editor__frame').first();
      await expect(frame).toHaveAttribute('aria-label', /XML/);
      await input.focus();
      await page.keyboard.press('Control+Home');
      for (let step = 0; step < '<listen>7'.length; step += 1) await page.keyboard.press('ArrowRight');
      await page.keyboard.type('1');
      // The typing pause sends the edit; the host echoes it at once.
      await page.waitForTimeout(1200);
      await expect(input).toHaveValue(/^<listen>71835</);
      await expect(frame).toHaveAttribute('aria-label', /XML/);
    });
  }
  test('documents-svelte: F8 after a selection and a click', async ({ browser }) => {
    await replay(
      browser,
      'documents-svelte',
      '?random',
      [
        { k: 'key', key: 'Shift+PageUp', n: 1 },
        { k: 'editAs', language: 'xml' },
        { k: 'click', x: 0.209, y: 0.312 },
        { k: 'suggest', open: true, pick: 2 },
        { k: 'f8', shift: false, n: 2 },
      ],
      'f8',
    );
  });
});
