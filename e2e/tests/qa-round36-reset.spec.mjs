import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 36 (FW36-1): a documents view's reset (the handle's `setValue`,
// `ConfigDocuments.replaceValue`) then typing, with the parent applying
// every text late and in any order, the reset's own too: the typing stays.
// Before, the reset's text listed after the typing was taken for the
// host's own and rolled the typing back.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

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

/** The order the texts held after the typing before the reset come back
 * in: `resets` are the resets' reports, `typed` what was typed after. */
const ORDERS = {
  'in order': ({ resets, typed }) => [...resets, ...typed],
  'reset last': ({ resets, typed }) => [...typed, ...resets],
  'reset in the middle': ({ resets, typed }) => [...typed.slice(0, 2), ...resets, ...typed.slice(2)],
  backwards: ({ resets, typed }) => [...resets, ...typed].reverse(),
};

/** The cases: the order, how many resets, whether the echoes of what was
 * typed before are still held, whether the tabs are switched in between. */
const CASES = [
  ...Object.keys(ORDERS).map((order) => ({ order, resets: 1, before: 'back', switches: false })),
  { order: 'reset last', resets: 1, before: 'held', switches: false },
  { order: 'reset in the middle', resets: 1, before: 'held', switches: false },
  { order: 'reset last', resets: 2, before: 'back', switches: false },
  { order: 'backwards', resets: 2, before: 'held', switches: false },
  { order: 'reset last', resets: 1, before: 'back', switches: true },
];

const describe = ({ order, resets, before, switches }) =>
  `${resets} reset${resets > 1 ? 's' : ''}, typed echoes ${before}, ${order}${switches ? ', tabs switched' : ''}`;

/** Releases the held texts `order` one at a time, then what the editor
 * sends again (at once, or after the pause) until nothing more comes. */
async function release(page, order, count) {
  for (const index of order) {
    await page.evaluate((index) => window.releaseEchoes([index]), index);
    await page.waitForTimeout(5);
  }
  for (let released = count; ; ) {
    await page.waitForTimeout(400);
    const now = await page.evaluate(() => window.heldEchoes());
    if (now === released) break;
    expect(now - count).toBeLessThan(8);
    for (; released < now; released += 1) {
      await page.evaluate((index) => window.releaseEchoes([index]), released);
      await page.waitForTimeout(20);
    }
  }
}

for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
  test.describe(`${name}: setValue, then typing, every text handed back late`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    for (const scenario of CASES) {
      test(describe(scenario), async ({ page }) => {
        await open(page, name, '?manual');
        const input = page.locator('.config-editor-documents__panel:visible textarea.config-editor__input');
        const typeAtEnd = async (text) => {
          await input.focus();
          await page.keyboard.press('Control+End');
          await page.keyboard.type(text, { delay: 10 });
          await page.waitForTimeout(100);
        };
        await page.click('[role="tab"][data-document-id="limits"]');
        await typeAtEnd('x = 1');
        const first = await page.evaluate(() => window.heldEchoes());
        if (scenario.before === 'back') {
          await page.evaluate((count) => window.releaseEchoes([...Array(count).keys()]), first);
          await page.waitForTimeout(400);
        }
        let reset = '';
        for (let index = 0; index < scenario.resets; index += 1) {
          reset = `[limits]\nmax_pending = ${index + 1}\n`;
          expect(await page.evaluate((reset) => window.documentsHandle.current.setValue('limits', reset), reset)).toBe(
            true,
          );
          await page.waitForTimeout(50);
        }
        if (scenario.switches) {
          await page.click('[role="tab"][data-document-id="main"]');
          await page.waitForTimeout(100);
          await page.click('[role="tab"][data-document-id="limits"]');
          await page.waitForTimeout(100);
        }
        const reported = await page.evaluate(() => window.heldEchoes());
        await typeAtEnd('y = 2');
        const count = await page.evaluate(() => window.heldEchoes());
        const range = (from, to) => [...Array(to - from).keys()].map((index) => index + from);
        const typedBefore = scenario.before === 'held' ? range(0, first) : [];
        const resets = range(first, reported);
        const typed = range(reported, count);
        const order = [...typedBefore, ...ORDERS[scenario.order]({ resets, typed })];
        await release(page, order, count);
        const expected = `${reset}y = 2`;
        expect(
          await page.evaluate(() => ({
            editor: window.documentsHandle.current.editor('limits').value,
            parent: window.currentFiles().find((file) => file.id === 'limits').value,
          })),
        ).toEqual({ editor: expected, parent: expected });
        expect(page.errors).toEqual([]);
      });
    }
  });
}

// --- The vanilla view: the host lists `replaceValue`'s text late. ----------

/** Mounts a documents view with `echoes: true` in the vanilla page whose
 * host holds every text it is to list (`onChange`'s and the resets'):
 * `window.held` of them, `window.release(index)` lists one. */
async function mountHolding(page) {
  await open(page, 'documents.html');
  await page.evaluate(async () => {
    window.documentsView.destroy();
    const module = await import('../config-editor/index.js');
    const host = document.createElement('div');
    host.id = 'probe';
    host.style.height = '420px';
    document.body.prepend(host);
    window.files = [
      { id: 'main', fileName: 'main.yaml', value: 'listen: 7835\n' },
      { id: 'other', fileName: 'other.yaml', value: 'other: 1\n' },
    ];
    window.held = [];
    window.hold = (id, value) =>
      window.held.push(() => {
        window.files = window.files.map((file) => (file.id === id ? { ...file, value } : file));
        window.view.update({ documents: window.files });
      });
    window.release = (index) => window.held[index]();
    window.view = new module.ConfigDocuments(host, {
      documents: window.files,
      echoes: true,
      onChange: (id, value) => window.hold(id, value),
    });
  });
}

test.describe('ConfigDocuments: replaceValue, then typing, the host listing every text late', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const scenario of CASES) {
    test(describe(scenario), async ({ page }) => {
      await mountHolding(page);
      const typeAtEnd = async (text) => {
        await page.evaluate(() => window.view.focus());
        await page.keyboard.press('Control+End');
        await page.keyboard.type(text, { delay: 10 });
        await page.waitForTimeout(100);
      };
      await typeAtEnd('a: 1');
      const first = await page.evaluate(() => window.held.length);
      if (scenario.before === 'back') {
        await page.evaluate((count) => [...Array(count).keys()].forEach(window.release), first);
        await page.waitForTimeout(400);
      }
      let reset = '';
      for (let index = 0; index < scenario.resets; index += 1) {
        reset = `listen: ${index + 1}\n`;
        await page.evaluate((reset) => {
          window.view.replaceValue('main', reset);
          window.hold('main', reset);
        }, reset);
        await page.waitForTimeout(50);
      }
      if (scenario.switches) {
        await page.evaluate(() => window.view.select('other'));
        await page.waitForTimeout(100);
        await page.evaluate(() => window.view.select('main'));
        await page.waitForTimeout(100);
      }
      const reported = await page.evaluate(() => window.held.length);
      await typeAtEnd('b: 2');
      const count = await page.evaluate(() => window.held.length);
      const range = (from, to) => [...Array(to - from).keys()].map((index) => index + from);
      const typedBefore = scenario.before === 'held' ? range(0, first) : [];
      const order = [
        ...typedBefore,
        ...ORDERS[scenario.order]({ resets: range(first, reported), typed: range(reported, count) }),
      ];
      for (const index of order) {
        await page.evaluate((index) => window.release(index), index);
        await page.waitForTimeout(5);
      }
      for (let released = count; ; ) {
        await page.waitForTimeout(400);
        const now = await page.evaluate(() => window.held.length);
        if (now === released) break;
        expect(now - count).toBeLessThan(8);
        for (; released < now; released += 1) await page.evaluate((index) => window.release(index), released);
      }
      const expected = `${reset}b: 2`;
      expect(
        await page.evaluate(() => ({
          editor: window.view.editor('main').value,
          host: window.files.find((file) => file.id === 'main').value,
        })),
      ).toEqual({ editor: expected, host: expected });
      expect(page.errors).toEqual([]);
    });
  }
});
