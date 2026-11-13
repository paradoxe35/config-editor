import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 30 (documents view): the handle's `setValue` wins over echoes
// still on their way in every framework (React's render right after it
// listed the text it replaced, which rolled it back); a `replaceValue`
// leaves the language another document is edited as; a host reordering the
// tabs keeps the active one in view; keys pressed right after a focus move
// land where it moved (sent back to back, as a fast typist's).

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

// --- setValue against echoes still on their way (F30-2). --------------------

/** The orders the held echoes come back in: all at once backwards or in
 * order, or one at a time backwards. */
const ORDERS = {
  backwards: (count) => [[...Array(count).keys()].reverse()],
  'in order': (count) => [[...Array(count).keys()]],
  'one by one backwards': (count) => [...Array(count).keys()].reverse().map((index) => [index]),
};

/** The main document's first line in the editor, and in the parent's list. */
const main = (page) =>
  page.evaluate(() => ({
    text: window.documentsHandle.current.editor('main').value.split('\n')[0],
    parent: window.currentFiles().find((file) => file.id === 'main').value.split('\n')[0],
  }));

for (const name of DOCUMENTS_FRAMEWORK_PAGES) {
  test.describe(`${name}: setValue while echoes are on their way`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    for (const [order, groups] of Object.entries(ORDERS)) {
      test(`the text set stands, the echoes handed back ${order}`, async ({ page }) => {
        await open(page, name, '?manual');
        await page.click('[role="tab"][data-document-id="main"]');
        await page.keyboard.press('Enter');
        await page.keyboard.press('End');
        await page.keyboard.type('123', { delay: 60 });
        await page.waitForTimeout(300);
        expect(await page.evaluate(() => window.documentsHandle.current.setValue('main', 'listen: 1\n'))).toBe(true);
        await page.waitForTimeout(200);
        const held = await page.evaluate(() => window.heldEchoes());
        for (const group of groups(held)) {
          await page.evaluate((group) => window.releaseEchoes(group), group);
          await page.waitForTimeout(100);
        }
        // What the editor sends again (at once, or after the pause), until
        // nothing more comes.
        for (let released = held; ; ) {
          await page.waitForTimeout(400);
          const now = await page.evaluate(() => window.heldEchoes());
          if (now === released) break;
          expect(now - held).toBeLessThan(6);
          for (; released < now; released += 1) {
            await page.evaluate((index) => window.releaseEchoes([index]), released);
            await page.waitForTimeout(50);
          }
        }
        expect(await main(page)).toEqual({ text: 'listen: 1', parent: 'listen: 1' });
        expect(page.errors).toEqual([]);
      });
    }
  });
}

/** Mounts a documents view of `documents` with `echoes: true` in the
 * vanilla page, its `onChange` texts kept in `window.changes`. */
async function mountDocuments(page, documents, options = {}) {
  await open(page, 'documents.html');
  await page.evaluate(
    async ([documents, options]) => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const host = document.createElement('div');
      host.id = 'probe';
      host.style.height = options.height ?? '420px';
      document.body.prepend(host);
      window.changes = [];
      window.view = new module.ConfigDocuments(host, {
        documents,
        echoes: true,
        onChange: (id, value) => window.changes.push({ id, value }),
        onSave: () => {},
      });
    },
    [documents, options],
  );
}

test.describe('ConfigDocuments with echoes: replaceValue, then the list as before', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a list given again as before (another document edited) keeps the text replaced', async ({ page }) => {
    const files = [
      { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' },
      { id: 'b', fileName: 'b.yaml', value: 'b: 2\n' },
    ];
    await mountDocuments(page, files);
    const after = await page.evaluate(async (files) => {
      window.view.replaceValue('a', 'a: 9\n');
      // The host's render for another document's edit, before it took the
      // replaced text.
      window.view.update({ documents: files.map((file) => (file.id === 'b' ? { ...file, value: 'b: 3\n' } : file)) });
      await new Promise((resolve) => setTimeout(resolve, 600));
      return { a: window.view.editor('a').value, b: window.view.editor('b').value };
    }, files);
    expect(after).toEqual({ a: 'a: 9\n', b: 'b: 3\n' });
    expect(page.errors).toEqual([]);
  });
});

// --- replaceValue keeps another document's Edit-as (L30-4). ------------------

test.describe('ConfigDocuments: a document edited as another language through the handle', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const call of ['replaceValue of another document', 'a settings update']) {
    test(`stays edited as it after ${call}`, async ({ page }) => {
      // `a` has settings of its own: a view's settings update lists the
      // documents again with them.
      await mountDocuments(page, [
        { id: 'a', fileName: 'a.yaml', value: 'a: 1\n', settings: { showFooter: false } },
        { id: 'b', fileName: 'b.yaml', value: '# keep\nb: 2\n' },
        { id: 'c', fileName: 'c.yaml', value: 'c: 3\n' },
      ]);
      const onScreen = () => page.evaluate(() => window.view.editor('b').text);
      await page.evaluate(() => {
        window.view.select('b');
        window.view.editAs('b', 'json');
      });
      await expect.poll(onScreen).toBe('{\n  "b": 2\n}\n');
      await page.evaluate((call) => {
        if (call.startsWith('replaceValue')) window.view.replaceValue('c', 'c: 9\n');
        else window.view.update({ showFooter: true });
      }, call);
      await page.waitForTimeout(300);
      expect(await onScreen()).toBe('{\n  "b": 2\n}\n');
      expect(await page.evaluate(() => window.view.editor('b').value)).toBe('# keep\nb: 2\n');
      expect(page.errors).toEqual([]);
    });
  }
});

// --- The active tab in view after a reorder (L30-5). -------------------------

test.describe('ConfigDocuments: a host reordering the tabs', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('keeps the active tab in view', async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 700 });
    const files = Array.from({ length: 40 }, (_, index) => ({
      id: `d${index}`,
      fileName: `conf.d/document-${index}.yaml`,
      value: `k: ${index}\n`,
    }));
    await mountDocuments(page, files, { height: '300px' });
    await page.evaluate(() => window.view.select('d39'));
    const shown = () =>
      page.evaluate(() => {
        const strip = document.querySelector('#probe [role="tablist"]').getBoundingClientRect();
        const tab = document.querySelector('#probe [role="tab"][aria-selected="true"]').getBoundingClientRect();
        return tab.left >= strip.left - 1 && tab.right <= strip.right + 1;
      });
    await expect.poll(shown).toBe(true);
    for (const reorder of ['reversed', 'to the middle', 'back to the end']) {
      await page.evaluate(
        ([files, reorder]) => {
          const listed = reorder === 'reversed' ? [...files].reverse() : [...files];
          if (reorder === 'to the middle') listed.splice(20, 0, ...listed.splice(39, 1));
          window.view.update({ documents: listed });
        },
        [files, reorder],
      );
      await expect.poll(shown, { message: reorder }).toBe(true);
    }
  });
});

// --- Keys right after a focus move (L30-6). ----------------------------------

/** The keys the burst helper knows: `[key, code, key code]`. */
const KEYS = {
  Enter: ['Enter', 'Enter', 13],
  ArrowDown: ['ArrowDown', 'ArrowDown', 40],
  End: ['End', 'End', 35],
  PageDown: ['PageDown', 'PageDown', 34],
};

/** Presses `keys` (`Alt+PageDown`, `End`, a character) back to back: every
 * key event sent to the browser at once, in order, none waiting for the
 * page to handle the one before (as a fast typist's keys come). */
async function burst(page, keys) {
  const cdp = await page.context().newCDPSession(page);
  const sent = [];
  for (const press of keys) {
    const alt = press.startsWith('Alt+');
    const name = alt ? press.slice(4) : press;
    const [key, code, keyCode] = KEYS[name] ?? [name, `Key${name.toUpperCase()}`, name.toUpperCase().charCodeAt(0)];
    const text = KEYS[name] ? (name === 'Enter' ? '\r' : undefined) : name;
    const event = { key, code, windowsVirtualKeyCode: keyCode, modifiers: alt ? 1 : 0 };
    if (alt) sent.push(cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Alt', code: 'AltLeft', windowsVirtualKeyCode: 18, modifiers: 1 }));
    sent.push(cdp.send('Input.dispatchKeyEvent', { type: text && !alt ? 'keyDown' : 'rawKeyDown', ...event, ...(text && !alt ? { text } : {}) }));
    sent.push(cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...event }));
    if (alt) sent.push(cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Alt', code: 'AltLeft', windowsVirtualKeyCode: 18 }));
  }
  await Promise.all(sent);
  await cdp.detach();
}

test.describe('JS example documents.html: keys pressed right after the focus moves', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Enter on a tab, then End and a character at once: they go to the editor', async ({ page }) => {
    await open(page, 'documents.html');
    await page.click('[role="tab"][data-document-id="main"]');
    await burst(page, ['Enter', 'ArrowDown', 'End', '9']);
    await expect
      .poll(() => page.evaluate(() => window.documentsView.editor('main').value))
      .toBe('listen: 7835\npublic_address: tunnel.example.com9\n');
    expect(await page.evaluate(() => window.documentsView.activeId)).toBe('main');
  });

  for (const visited of [false, true]) {
    test(`Alt+PageDown, then typing at once: it goes to the next document (${visited ? 'shown before' : 'first shown'})`, async ({ page }) => {
      await open(page, 'documents.html');
      if (visited) {
        await page.click('[role="tab"][data-document-id="limits"]');
        await page.waitForTimeout(200);
      }
      await page.click('[role="tab"][data-document-id="main"]');
      await page.keyboard.press('Enter');
      await page.waitForTimeout(100);
      await page.keyboard.press('Control+End');
      await burst(page, ['Alt+PageDown', 'Q', 'W']);
      await expect
        .poll(() => page.evaluate(() => window.documentsView.editor('limits').value))
        .toBe('QW[limits]\nmax_pending = 512\n');
      expect(await page.evaluate(() => window.documentsView.editor('main').value)).toBe(
        'listen: 7835\npublic_address: tunnel.example.com\n',
      );
    });
  }
});
