import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The focus keeper (`focus::keep_focus`) watches the view's subtree, which
// re-renders rows on every keystroke and scroll: while the focus stays on a
// usable control it reads none of the mutation records (a constant check),
// and it hears at most a callback or two per keystroke or page, whatever the
// document's length.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

const LINES = 8000;
const TEXT = Array.from({ length: LINES }, (_, index) => `key_${index}: value ${index}`).join('\n') + '\n';

/**
 * Counts the editor's mutation callbacks (observers on an element of the
 * editor, not the test runner's own on the document) and every read of a
 * record's nodes they make.
 */
function countMutations() {
  window.mutationCallbacks = 0;
  window.recordReads = 0;
  let inEditor = false;
  const Native = window.MutationObserver;
  window.MutationObserver = class extends Native {
    constructor(callback) {
      super((records, observer) => {
        if (!this.editors) return callback(records, observer);
        window.mutationCallbacks += 1;
        inEditor = true;
        try {
          return callback(records, observer);
        } finally {
          inEditor = false;
        }
      });
    }
    observe(target, options) {
      if (target instanceof Element && target.closest('.config-editor, .config-editor-documents')) this.editors = true;
      return super.observe(target, options);
    }
  };
  for (const name of ['addedNodes', 'removedNodes', 'target']) {
    const getter = Object.getOwnPropertyDescriptor(MutationRecord.prototype, name).get;
    Object.defineProperty(MutationRecord.prototype, name, {
      configurable: true,
      get() {
        if (inEditor) window.recordReads += 1;
        return getter.call(this);
      },
    });
  }
}

const reset = (page) => page.evaluate(() => ((window.mutationCallbacks = 0), (window.recordReads = 0)));
const counts = (page) => page.evaluate(() => ({ callbacks: window.mutationCallbacks, reads: window.recordReads }));

test.describe('the focus keeper costs nothing while the focus stays', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test(`typing and paging through ${LINES} lines read no mutation record`, async ({ page }) => {
    await page.addInitScript(countMutations);
    await page.goto(example);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const input = page.locator('.config-editor__input').first();
    await input.waitFor();
    await page.evaluate((text) => window.editor.update({ value: text }), TEXT);
    await input.focus();
    await page.keyboard.press('Control+End');
    await page.waitForTimeout(300);

    await reset(page);
    const KEYS = 20;
    for (let key = 0; key < KEYS; key += 1) await page.keyboard.type('x');
    await page.waitForTimeout(300);
    const typing = await counts(page);
    expect(typing.reads, 'records read while typing').toBe(0);
    expect(typing.callbacks, 'callbacks while typing').toBeLessThanOrEqual(KEYS * 3);

    await reset(page);
    const PAGES = 10;
    for (let step = 0; step < PAGES; step += 1) await page.keyboard.press('PageUp');
    await page.waitForTimeout(300);
    const paging = await counts(page);
    expect(paging.reads, 'records read while paging').toBe(0);
    expect(paging.callbacks, 'callbacks while paging').toBeLessThanOrEqual(PAGES * 3);
    // The watch is the editor's: it heard the rows change.
    expect(typing.callbacks + paging.callbacks).toBeGreaterThan(0);
    await expect(input).toBeFocused();
  });
});
