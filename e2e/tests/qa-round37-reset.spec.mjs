import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 37: a reset while echoes are still on their way.
//
// - FW37-2: a host handing every text back late with `readOnlyRanges`
//   worked out on its own copy, a reset (`replaceValue`, the handle's
//   `setValue`) after typing a line above a locked one: the lock stays on
//   its field. Before, it was placed again where it was last given, a line
//   below, and the field could be edited (and was saved).
// - FW37-1: two resets to the same text (a double Discard), then typing,
//   then the host's own revert to that text: it applies. Before, the
//   second reset waited for a listing of its own, so for 5 seconds the
//   revert was taken for its echo and the editor overwrote the host.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

function listen(page) {
  page.errors = [];
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page, name, query = '') {
  listen(page);
  await page.goto(example(name) + query);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').first().waitFor();
}

const LOADED = 'listen: 7835\npublic_address: tunnel.example.com\n';
const RESET = 'listen: 7835\npublic_address: reset.example.com\n';

/** Installs `window.lockFor(text)`: the host's lock on the
 * `public_address` line of its text, in `form` (`line` or `index`). */
const lockFor = (page, form) =>
  page.evaluate((form) => {
    window.lockFor = (text) => {
      const lines = text.split('\n');
      const at = lines.findIndex((line) => line.startsWith('public_address'));
      if (at < 0) return [];
      if (form === 'line') return [{ startLine: at + 1, startColumn: 1, endLine: at + 1, endColumn: lines[at].length + 1 }];
      const start = text.indexOf('public_address');
      return [{ start, end: start + lines[at].length }];
    };
  }, form);

/** Mounts, in the vanilla page, a `ConfigEditor` (`kind` 'editor') or a
 * `ConfigDocuments` with `echoes: true` whose host holds every text it is
 * to give back with its lock: `window.held` of them, `window.release(i)`
 * gives one. `window.reset(text)` resets; `window.current()` is the
 * editor's text and the host's. */
async function mountHolding(page, kind, form) {
  await open(page, 'documents.html');
  await lockFor(page, form);
  await page.evaluate(
    async ({ kind, loaded }) => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const element = document.createElement('div');
      element.id = 'probe';
      element.style.height = '420px';
      document.body.prepend(element);
      window.stored = loaded;
      window.held = [];
      window.release = (index) => window.held[index]();
      const list = () => [{ id: 'main', fileName: 'main.yaml', value: window.stored, readOnlyRanges: window.lockFor(window.stored) }];
      const give = (value) => {
        window.stored = value;
        if (kind === 'editor') window.view.update({ value, readOnlyRanges: window.lockFor(value) });
        else window.view.update({ documents: list() });
      };
      const hold = (value) => window.held.push(() => give(value));
      if (kind === 'editor') {
        window.view = new module.ConfigEditor(element, {
          language: 'yaml',
          value: loaded,
          readOnlyRanges: window.lockFor(loaded),
          echoes: true,
          onChange: hold,
        });
        window.reset = (text) => window.view.replaceValue(text);
        window.current = () => ({ editor: window.view.value, host: window.stored });
      } else {
        window.view = new module.ConfigDocuments(element, { documents: list(), echoes: true, onChange: (id, value) => hold(value) });
        window.reset = (text) => window.view.replaceValue('main', text);
        window.current = () => ({ editor: window.view.editor('main').value, host: window.stored });
      }
    },
    { kind, loaded: LOADED },
  );
  return page.locator('#probe textarea.config-editor__input');
}

/** Puts the caret at the end of the document's `public_address` line and
 * tries to change it: two Backspaces and an `X`. */
async function editLockedLine(page, input, text) {
  const line = text.split('\n').findIndex((each) => each.startsWith('public_address'));
  await input.focus();
  await page.keyboard.press('Control+Home');
  for (let at = 0; at < line; at += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.type('X');
  await page.waitForTimeout(200);
}

const lockedLine = (text) => text.split('\n').find((line) => line.startsWith('public_address'));

for (const kind of ['editor', 'documents']) {
  const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';
  test.describe(`${name}: a reset with echoes and their locks still on their way`, () => {
    for (const form of ['line', 'index']) {
      for (const typedAfter of [false, true]) {
        test(`the lock stays on its field (${form} ranges${typedAfter ? ', typing after the reset' : ''})`, async ({
          page,
        }) => {
          const input = await mountHolding(page, kind, form);
          await input.focus();
          await page.keyboard.press('Control+Home');
          await page.keyboard.press('End');
          await page.keyboard.press('Enter');
          await page.keyboard.type('a: 1', { delay: 15 });
          await expect.poll(() => page.evaluate(() => window.held.length)).toBeGreaterThan(2);
          // The host takes only the first three texts back (its lock on
          // line 3 of each), then the user resets.
          await page.evaluate(() => [0, 1, 2].forEach(window.release));
          await page.waitForTimeout(100);
          await page.evaluate((text) => window.reset(text), RESET);
          await page.waitForTimeout(100);
          if (typedAfter) {
            await input.focus();
            await page.keyboard.press('Control+End');
            await page.keyboard.type('b: 2', { delay: 15 });
            await page.keyboard.press('Escape');
          }
          const before = await page.evaluate(() => window.current().editor);
          await editLockedLine(page, input, before);
          const after = await page.evaluate(() => window.current().editor);
          expect(lockedLine(after)).toBe(lockedLine(RESET));
          // Every text the host still holds given back: the field stays.
          const count = await page.evaluate(() => window.held.length);
          for (let index = 3; index < count; index += 1) await page.evaluate((index) => window.release(index), index);
          await page.waitForTimeout(400);
          expect(lockedLine(await page.evaluate(() => window.current().editor))).toBe(lockedLine(RESET));
        });
      }
    }
  });
}

for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
  test.describe(`${name}: setValue with echoes and their locks still on their way`, () => {
    for (const form of ['line', 'index']) {
      test(`the lock stays on its field (${form} ranges)`, async ({ page }) => {
        await open(page, name, '?manual');
        await lockFor(page, form);
        // The host gives its text back with its lock now and then, worked
        // out on the text it holds.
        await page.evaluate(() => {
          window.relisting = setInterval(() => {
            const file = window.currentFiles().find((each) => each.id === 'main');
            window.updateFile('main', { value: file.value, readOnlyRanges: window.lockFor(file.value) });
          }, 17);
        });
        await page.waitForTimeout(200);
        const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
        await input.focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('End');
        await page.keyboard.press('Enter');
        await page.keyboard.type('a: 1', { delay: 15 });
        await expect.poll(() => page.evaluate(() => window.heldEchoes())).toBeGreaterThan(2);
        await page.evaluate(() => window.releaseEchoes([0, 1, 2]));
        await page.waitForTimeout(100);
        await page.evaluate((text) => window.documentsHandle.current.setValue('main', text), RESET);
        await input.focus();
        await page.keyboard.press('Control+End');
        await page.keyboard.type('b: 2', { delay: 15 });
        await page.keyboard.press('Escape');
        await page.waitForTimeout(100);
        const value = () => page.evaluate(() => window.documentsHandle.current.editor('main').value);
        await editLockedLine(page, input, await value());
        await page.evaluate(() => clearInterval(window.relisting));
        expect(lockedLine(await value())).toBe(lockedLine(RESET));
        expect(page.errors.filter((error) => !error.includes('past the end'))).toEqual([]);
      });
    }
  });
}

// --- FW37-1: a double Discard, typing, then the host's own revert. --------

const LIMITS = '[limits]\nmax_pending = 1\n';

for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
  test.describe(`${name}: setValue twice to one text, typing, then the host's revert`, () => {
    for (const when of ['in one task', '30 ms apart']) {
      test(`the revert applies (${when})`, async ({ page }) => {
        await open(page, name);
        await page.click('[role="tab"][data-document-id="limits"]');
        const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
        const typeAtEnd = async (text) => {
          await input.focus();
          await page.keyboard.press('Control+End');
          await page.keyboard.type(text, { delay: 20 });
          await page.waitForTimeout(400);
        };
        await typeAtEnd('x = 1');
        await page.evaluate(
          async ({ text, apart }) => {
            const handle = window.documentsHandle.current;
            handle.setValue('limits', text);
            if (apart) await new Promise((resolve) => setTimeout(resolve, 30));
            handle.setValue('limits', text);
          },
          { text: LIMITS, apart: when !== 'in one task' },
        );
        await page.waitForTimeout(400);
        await typeAtEnd('y = 2');
        await page.waitForTimeout(400);
        await page.evaluate((text) => window.updateFile('limits', { value: text }), LIMITS);
        await page.waitForTimeout(1000);
        expect(
          await page.evaluate(() => ({
            editor: window.documentsHandle.current.editor('limits').value,
            host: window.currentFiles().find((file) => file.id === 'limits').value,
          })),
        ).toEqual({ editor: LIMITS, host: LIMITS });
        expect(page.errors).toEqual([]);
      });
    }
  });
}

for (const kind of ['editor', 'documents']) {
  const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';
  test.describe(`${name}: replaceValue twice to one text, typing, then the host's revert`, () => {
    for (const listings of [1, 2]) {
      test(`the revert applies (the host lists the reset ${listings === 1 ? 'once' : 'twice'})`, async ({ page }) => {
        await open(page, 'documents.html');
        await page.evaluate(
          async ({ kind }) => {
            window.documentsView.destroy();
            const module = await import('../config-editor/index.js');
            const element = document.createElement('div');
            element.id = 'probe';
            element.style.height = '420px';
            document.body.prepend(element);
            window.stored = 'listen: 7835\n';
            window.give = (value) => {
              window.stored = value;
              if (kind === 'editor') window.view.update({ value });
              else window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value }] });
            };
            if (kind === 'editor') {
              window.view = new module.ConfigEditor(element, {
                language: 'yaml',
                value: window.stored,
                echoes: true,
                onChange: window.give,
              });
              window.reset = (text) => window.view.replaceValue(text);
              window.current = () => ({ editor: window.view.value, host: window.stored });
            } else {
              window.view = new module.ConfigDocuments(element, {
                documents: [{ id: 'main', fileName: 'main.yaml', value: window.stored }],
                echoes: true,
                onChange: (id, value) => window.give(value),
              });
              window.reset = (text) => window.view.replaceValue('main', text);
              window.current = () => ({ editor: window.view.editor('main').value, host: window.stored });
            }
          },
          { kind },
        );
        const input = page.locator('#probe textarea.config-editor__input');
        const typeAtEnd = async (text) => {
          await input.focus();
          await page.keyboard.press('Control+End');
          await page.keyboard.type(text, { delay: 20 });
          await page.waitForTimeout(400);
        };
        const reset = 'listen: 1\n';
        await typeAtEnd('a: 1');
        await page.evaluate(
          ({ reset, listings }) => {
            window.reset(reset);
            window.reset(reset);
            for (let listed = 0; listed < listings; listed += 1) window.give(reset);
          },
          { reset, listings },
        );
        await page.waitForTimeout(100);
        await typeAtEnd('b: 2');
        await page.waitForTimeout(400);
        await page.evaluate((reset) => window.give(reset), reset);
        await page.waitForTimeout(1000);
        expect(await page.evaluate(() => window.current())).toEqual({ editor: reset, host: reset });
        expect(page.errors).toEqual([]);
      });
    }
  });
}
