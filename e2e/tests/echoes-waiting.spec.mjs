import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// However many texts the editor sent wait for their echo (a busy page: a
// host's echo timers queued behind thousands of keys), each echo is told
// from the host's own text, in order or not: none rolls the document back,
// and the host ends on the document (QA 29: past 256 waiting, the oldest
// was forgotten and its echo taken for the host's text, losing what was
// typed since). For `echoes: true` (ConfigEditor, ConfigDocuments) and the
// framework components (`ValueSync`). The host holds every echo until all
// keys are typed, then hands them back.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const START = 'name: web\nnote: x\n';
const KEYS = 300;
const TYPED = `name: web\nnote: x${'abcdefghij'.repeat(KEYS / 10)}\n`;

// The orders the held echoes come back in.
const ORDERS = ['in order', 'backwards', 'shuffled'];

function listen(page) {
  page.errors = [];
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

/** The host's part, run in the page: `window.hold(text)` keeps an echo,
 * `window.release(order, giveBack)` hands the held ones back in `order`,
 * each through `giveBack(text)` (awaited), noting a text shown that is not
 * the document as it was before (a rollback). */
const HOST = `
  window.held = [];
  window.rollbacks = [];
  window.hold = (text) => window.held.push(text);
  window.release = async (order, giveBack, shown) => {
    for (let round = 0; round < 20 && window.held.length; round += 1) {
      const texts = window.held.splice(0);
      if (order === 'backwards') texts.reverse();
      if (order === 'shuffled') {
        let seed = 7;
        for (let at = texts.length - 1; at > 0; at -= 1) {
          seed = (seed * 1103515245 + 12345) % 2147483648;
          const other = seed % (at + 1);
          [texts[at], texts[other]] = [texts[other], texts[at]];
        }
      }
      for (const text of texts) {
        const before = shown();
        await giveBack(text);
        if (shown() !== before) window.rollbacks.push(shown().length);
      }
      // The pause after which a host left on an older text hears the document again.
      await new Promise((resolve) => setTimeout(resolve, 600));
    }
  };
`;

/** Types `KEYS` keys at the end of `note: x`. */
async function typeKeys(page) {
  const input = page.locator('.config-editor__input');
  await input.focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.type('abcdefghij'.repeat(KEYS / 10));
}

test.describe('Echoes: however many wait, each is told apart', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const host of ['editor', 'one document']) {
    for (const order of ORDERS) {
      test(`${host} with echoes, ${KEYS} waiting, handed back ${order}: typing is kept, and the host ends on it`, async ({ page }) => {
        listen(page);
        await page.goto(example('vanilla.html'));
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await page.evaluate(
          async ([host, start, script]) => {
            new Function(script)();
            window.editor?.destroy();
            const module = await import('../config-editor/index.js');
            const element = document.getElementById('editor');
            element.innerHTML = '';
            window.store = start;
            if (host === 'editor') {
              const editor = new module.ConfigEditor(element, { value: start, echoes: true, onChange: (text) => window.hold(text) });
              window.shown = () => editor.value;
              window.giveBack = (text) => {
                window.store = text;
                editor.update({ value: text });
              };
            } else {
              const list = () => [{ id: 'a', fileName: 'a.yaml', value: window.store }];
              const view = new module.ConfigDocuments(element, { documents: list(), echoes: true, onChange: (_, text) => window.hold(text) });
              window.shown = () => view.editor('a').value;
              window.giveBack = (text) => {
                window.store = text;
                view.update({ documents: list() });
              };
            }
            await new Promise((resolve) => setTimeout(resolve, 300));
          },
          [host, START, HOST],
        );
        await typeKeys(page);
        const result = await page.evaluate(async (order) => {
          const waiting = window.held.length;
          await window.release(order, window.giveBack, window.shown);
          return { waiting, text: window.shown(), store: window.store, rollbacks: window.rollbacks };
        }, order);
        expect(result.waiting).toBeGreaterThan(256);
        expect(result).toEqual({ waiting: result.waiting, text: TYPED, store: TYPED, rollbacks: [] });
        expect(page.errors).toEqual([]);
      });
    }
  }

  for (const name of FRAMEWORK_PAGES) {
    for (const order of ORDERS) {
      test(`${name}, ${KEYS} values waiting, handed back ${order}: typing is kept, and the parent ends on it`, async ({ page }) => {
        listen(page);
        // `?random`: Svelte's one-way `value` (the test's parent hands it back).
        await page.goto(example(name) + '?random');
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await page.locator('.config-editor__input').waitFor();
        await page.evaluate(
          async ([vue, start, script]) => {
            new Function(script)();
            const bind = (value, onChange) =>
              window.setExtraProps(vue ? { modelValue: value, 'onUpdate:modelValue': onChange } : { value, onChange });
            window.store = start;
            const onChange = (text) => window.hold(text);
            window.shown = () => document.querySelector('.config-editor__input').value;
            // A render per value handed back.
            window.giveBack = async (text) => {
              window.store = text;
              bind(text, onChange);
              await new Promise((resolve) => setTimeout(resolve, 0));
            };
            bind(start, onChange);
            await new Promise((resolve) => setTimeout(resolve, 300));
          },
          [name.startsWith('vue'), START, HOST],
        );
        await typeKeys(page);
        const result = await page.evaluate(async (order) => {
          const waiting = window.held.length;
          await window.release(order, window.giveBack, window.shown);
          return { waiting, text: window.shown(), store: window.store, rollbacks: window.rollbacks };
        }, order);
        expect(result.waiting).toBeGreaterThan(256);
        expect(result).toEqual({ waiting: result.waiting, text: TYPED, store: TYPED, rollbacks: [] });
        expect(page.errors).toEqual([]);
      });
    }
  }
});
