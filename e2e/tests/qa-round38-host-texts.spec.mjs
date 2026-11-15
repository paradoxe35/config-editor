import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 38: a host's texts and the ranges it works out on them.
// - FW38-1: right after a reset, a host still listing the text it held
//   before (its own copy lags) with the lock it worked out on that text:
//   typing on the line above the lock is taken. Before, those ranges were
//   moved across the reset as if it were an edit, the lock grew over the
//   line above, and the typing was refused.
// - FW38-3: a framework's render listing a newer text with the lock worked
//   out on the text it rendered before (line ranges): the lock stays on its
//   field. Before, they were read in the newer text, on the line above.
// - FW38-2: a framework page's `setValue(id, text)` twice in one task
//   reports the reset once (the second changes nothing).
// - H38-4: a document listed with an Edit-as text, reset in the same task
//   as `new ConfigDocuments`: its value is the reset's text at once.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const LOADED = 'listen: 7835\npublic_address: tunnel.example.com\n';
const RESET = 'listen: 2\npublic_address: two.example.com\nx: 1\n';

async function open(page, name, query = '') {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
  await page.goto(example(name) + query);
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
}

/** Mounts on the vanilla page a `ConfigEditor` (`kind` 'editor') or a
 * `ConfigDocuments` with `echoes`, whose host keeps its own copy
 * (`window.stored`), takes each text back only when told
 * (`window.release()`), and gives its copy back with its lock on the
 * `public_address` line every 17 ms: `window.lockText(stored)` names the
 * text the lock is worked out on (its copy, unless a test says another). */
async function mountLagging(page, kind) {
  await open(page, 'documents.html');
  await page.evaluate(
    async ({ kind, loaded }) => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const element = document.createElement('div');
      element.id = 'probe';
      element.style.height = '420px';
      document.body.prepend(element);
      window.lockFor = (text) => {
        const lines = text.split('\n');
        const at = lines.findIndex((line) => line.startsWith('public_address'));
        if (at < 0) return [];
        return [{ startLine: at + 1, startColumn: 1, endLine: at + 1, endColumn: lines[at].length + 1 }];
      };
      window.stored = loaded;
      window.lockText = (stored) => stored;
      window.held = [];
      window.release = () => {
        for (const take of window.held.splice(0)) take();
      };
      const hold = (value) => window.held.push(() => (window.stored = value));
      const give = () => {
        const value = window.stored;
        const readOnlyRanges = window.lockFor(window.lockText(value));
        if (kind === 'editor') window.view.update({ value, readOnlyRanges });
        else window.view.update({ documents: [{ id: 'main', fileName: 'main.yaml', value, readOnlyRanges }] });
      };
      if (kind === 'editor') {
        window.view = new module.ConfigEditor(element, {
          language: 'yaml',
          value: loaded,
          readOnlyRanges: window.lockFor(loaded),
          echoes: true,
          onChange: hold,
        });
        window.reset = (text) => {
          window.view.replaceValue(text);
          hold(text);
        };
        window.current = () => window.view.value;
      } else {
        const list = [{ id: 'main', fileName: 'main.yaml', value: loaded, readOnlyRanges: window.lockFor(loaded) }];
        window.view = new module.ConfigDocuments(element, { documents: list, echoes: true, onChange: (id, value) => hold(value) });
        window.reset = (text) => {
          window.view.replaceValue('main', text);
          hold(text);
        };
        window.current = () => window.view.editor('main').value;
      }
      window.relisting = setInterval(give, 17);
    },
    { kind, loaded: LOADED },
  );
  return page.locator('#probe textarea.config-editor__input');
}

/** Types `text` at the end of line `line` (one-based). */
async function typeAtEndOf(page, input, line, text) {
  await input.focus();
  await page.keyboard.press('Control+Home');
  for (let at = 1; at < line; at += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.type(text, { delay: 15 });
}

for (const kind of ['editor', 'documents']) {
  const name = kind === 'editor' ? 'ConfigEditor' : 'ConfigDocuments';

  test(`${name}: right after a reset, a host still giving its older copy with its lock: the line above the lock takes typing`, async ({
    page,
  }) => {
    const input = await mountLagging(page, kind);
    await page.waitForTimeout(100);
    await typeAtEndOf(page, input, 2, '');
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('a: 1', { delay: 15 });
    await page.evaluate((text) => window.reset(text), RESET);
    await page.waitForTimeout(100);
    // The host still gives its copy from before the reset.
    await typeAtEndOf(page, input, 1, '0');
    await page.waitForTimeout(100);
    // The lock is on its field.
    await typeAtEndOf(page, input, 2, 'Z');
    await page.waitForTimeout(100);
    expect(await page.evaluate(() => window.current())).toBe('listen: 20\npublic_address: two.example.com\nx: 1\n');
    await page.evaluate(() => window.release());
    await page.waitForTimeout(400);
    await page.evaluate(() => window.release());
    await page.waitForTimeout(400);
    await page.evaluate(() => clearInterval(window.relisting));
    expect(await page.evaluate(() => window.current())).toBe('listen: 20\npublic_address: two.example.com\nx: 1\n');
    expect(page.errors).toEqual([]);
  });

  test(`${name}: a newer text given with the lock worked out on the text given before: the lock stays on its field`, async ({ page }) => {
    const input = await mountLagging(page, kind);
    await page.waitForTimeout(100);
    // From now on the host works its lock out on the copy it gave before.
    await page.evaluate(() => {
      let before = window.stored;
      let given = window.stored;
      window.lockText = (stored) => {
        if (stored !== given) [before, given] = [given, stored];
        return before;
      };
    });
    // Two new lines, each given back on its own: the second with the lock
    // worked out on the text with the first.
    for (const _ of [0, 1]) {
      await typeAtEndOf(page, input, 1, '');
      await page.keyboard.press('Enter');
      await page.waitForTimeout(50);
      await page.evaluate(() => window.release());
      await page.waitForTimeout(100);
    }
    await typeAtEndOf(page, input, 4, 'Z');
    await page.waitForTimeout(100);
    await page.evaluate(() => window.release());
    await page.waitForTimeout(300);
    await page.evaluate(() => clearInterval(window.relisting));
    expect(await page.evaluate(() => window.current())).toBe('listen: 7835\n\n\npublic_address: tunnel.example.com\n');
    expect(page.errors).toEqual([]);
  });
}

for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
  test(`${name}: setValue twice to one text reports it once`, async ({ page }) => {
    await open(page, name);
    await page.evaluate(() => {
      const handle = window.documentsHandle.current;
      handle.setValue('limits', '[limits]\nmax_pending = 1\n');
      handle.setValue('limits', '[limits]\nmax_pending = 1\n');
    });
    await page.waitForTimeout(200);
    const log = await page.locator('#log').textContent();
    expect(log.split('\n').filter((line) => line === 'change limits')).toHaveLength(1);
  });
}

for (const active of ['x', 'other']) {
  test(`ConfigDocuments: a document listed with an Edit-as text, reset at once (${active === 'x' ? 'shown' : 'not shown'}), reads as the reset`, async ({
    page,
  }) => {
    await open(page, 'documents.html');
    const read = await page.evaluate(async (active) => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const element = document.createElement('div');
      element.style.height = '420px';
      document.body.prepend(element);
      const documents = [
        { id: 'other', fileName: 'o.yaml', value: 'o: 1\n' },
        { id: 'x', fileName: 'x.toml', value: 'a = 1\n', baseline: 'a = 1\n', editAs: 'yaml', editAsText: 'a: 2\n' },
      ];
      const view = new module.ConfigDocuments(element, { documents, activeId: active, onChange: () => {} });
      view.replaceValue('x', 'a = 5\n');
      const now = { value: view.editor('x').value, diff: view.editor('x').unifiedDiff() };
      await new Promise((resolve) => setTimeout(resolve, 300));
      return { now, later: { value: view.editor('x').value, diff: view.editor('x').unifiedDiff() } };
    }, active);
    expect(read.now.value).toBe('a = 5\n');
    expect(read.later.value).toBe('a = 5\n');
    expect(read.now.diff).toBe(read.later.diff);
    expect(read.now.diff).toContain('+a = 5');
  });
}
