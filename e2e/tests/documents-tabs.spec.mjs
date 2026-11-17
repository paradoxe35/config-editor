import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// Document tabs: Delete and close, names that tell tabs apart, narrow tabs, reordering, and keys
// pressed right after the focus moves.

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

// --- Keys right after a focus move. ----------------------------------

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

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);

  const documentInput = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"] .config-editor__input`);

  async function open(page, name, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).first().waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('Delete on a tab', () => {
    for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
      test(`${name}: the focus stays on the tabs, so a second Delete closes the next tab, never edits it`, async ({ page }) => {
        const errors = await open(page, name);
        page.on('dialog', (dialog) => dialog.accept());
        const add = page.locator('.config-editor-documents__add');
        for (const [id, text] of [
          ['new1', 'one: 1'],
          ['new2', 'two: 2'],
        ]) {
          await add.click();
          await expect(tab(page, id)).toHaveAttribute('aria-selected', 'true');
          await expect(documentInput(page, id)).toBeFocused();
          await page.keyboard.type(text);
          await expect(tab(page, id)).toHaveAttribute('data-dirty', 'true');
        }
        const changes = async () => (await page.locator('#log').textContent()).split('\n').filter((line) => line === 'change new2').length;
        const before = await changes();
        // Both are dirty: each Delete asks, and the host's confirm is accepted.
        await tab(page, 'new1').click();
        await expect(tab(page, 'new1')).toBeFocused();
        await page.keyboard.press('Delete');
        await expect(tab(page, 'new1')).toHaveCount(0);
        await expect(tab(page, 'new2')).toHaveAttribute('aria-selected', 'true');
        await expect(tab(page, 'new2')).toBeFocused();
        await expect(documentInput(page, 'new2')).toHaveValue('two: 2');
        await page.keyboard.press('Delete');
        await expect(tab(page, 'new2')).toHaveCount(0);
        await expect(tab(page, 'clients')).toBeFocused();
        expect(await changes()).toBe(before);
        expect(errors).toEqual([]);
      });
    }

    test('a click on × leaves the focus in the editor it was in', async ({ page }) => {
      const errors = await open(page, 'documents.html');
      await tab(page, 'limits').click();
      await documentInput(page, 'limits').click();
      await tab(page, 'limits').locator('.config-editor-documents__close').click();
      await expect(tab(page, 'limits')).toHaveCount(0);
      await expect(documentInput(page, 'clients')).toBeFocused();
      expect(errors).toEqual([]);
    });
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (name, options = {}) =>
    `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('narrow tabs leave their folder out', () => {
    test.use({ viewport: { width: 390, height: 600 } });

    test('the names that tell the tabs apart show; the title and the name read keep the folder', async ({ page }) => {
      await open(page, {}, 'documents.html');
      const add = page.locator('.config-editor-documents__add');
      await add.click();
      await add.click();
      const tabs = page.locator('[role="tab"]');
      const added = tabs.filter({ hasText: 'new' });
      await expect(added).toHaveCount(2);
      for (const tab of await added.all()) {
        const title = await tab.getAttribute('title');
        expect(title).toMatch(/\//);
        const seen = await tab.locator('.config-editor-documents__name').innerText();
        expect(seen).not.toContain('/');
        expect(title.endsWith(seen)).toBe(true);
        expect(await tab.getAttribute('aria-label')).toContain(title);
      }
      const seen = await added.locator('.config-editor-documents__name').allInnerTexts();
      expect(new Set(seen).size).toBe(2);
    });
  });
});

test.describe('host API', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  // Tabs sharing a file's name showed their folders' starts (alike) or cut
  // the file's name: each shows the end of its folder that tells it apart,
  // then the whole file's name, at phone width too.
  for (const width of [360, 1200]) {
    test(`tabs sharing a file's name show what tells them apart (${width} px)`, async ({ page }) => {
      await page.setViewportSize({ width, height: 400 });
      await page.goto(example('documents.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(() =>
        window.documentsView.update({
          documents: [
            { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: '' },
            { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: '' },
            { id: 'c', fileName: 'prod/limits.toml', value: '' },
            { id: 'd', fileName: 'staging/eu/limits.toml', value: '' },
          ],
        }),
      );
      await expect(page.locator('[role="tab"]')).toHaveCount(4);
      const tabs = await page.evaluate(() =>
        [...document.querySelectorAll('[role="tab"]')].map((tab) => {
          const name = tab.querySelector('.config-editor-documents__name').getBoundingClientRect();
          const end = tab.querySelector('.config-editor-documents__name-end');
          const start = tab.querySelector('.config-editor-documents__name-start');
          // The letters that tell it apart, kept whole.
          const kept = tab.querySelector('.config-editor-documents__name-kept');
          const box = end.getBoundingClientRect();
          return {
            start: (kept?.textContent ?? '') + start.textContent,
            end: end.textContent,
            whole: box.left >= name.left - 0.5 && box.right <= name.right + 0.5,
            // Cut or whole, the start shows more than one letter.
            shown: (kept?.clientWidth ?? 0) + start.clientWidth >= 14,
          };
        }),
      );
      expect(tabs.map(({ start, end }) => start + end)).toEqual([
        'eu-west-1/production/limits.toml',
        'us-east-1/production/limits.toml',
        'prod/limits.toml',
        'eu/limits.toml',
      ]);
      expect(tabs.every(({ end, whole }) => end === 'limits.toml' && whole)).toBe(true);
      expect(tabs.every(({ shown }) => shown)).toBe(true);
    });
  }
});

// A group of its own (needs the built npm packages, a 900×500 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 900, height: 500 } });

  // At phone width (a touch screen's wider tab buttons) tabs sharing
  // a file's name in different folders read alike ("e…limits.toml"): each
  // keeps the letters that tell it apart whole, then an ellipsis, then the
  // whole file's name.
  for (const [width, touch] of [[360, true], [600, false], [1200, false]]) {
    test.describe(`at ${width} px`, () => {
      test.use({ viewport: { width, height: 400 }, hasTouch: touch, isMobile: touch });
      test('tabs sharing a file name never read alike', async ({ page }) => {
        await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await page.evaluate(() =>
          window.documentsView.update({
            documents: [
              { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: 'a = 1\n', closable: false },
              { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: 'a = 1\n', readOnly: true },
              { id: 'c', fileName: 'prod/limits.toml', value: 'a = 1\n' },
              { id: 'd', fileName: 'staging/eu/limits.toml', value: 'a = 1\n' },
            ],
          }),
        );
        await expect(page.locator('[role="tab"]')).toHaveCount(4);
        const read = await page.evaluate(() =>
          [...document.querySelectorAll('[role="tab"]')].map((tab) => {
            const name = tab.querySelector('.config-editor-documents__name').getBoundingClientRect();
            const inside = (part) => {
              if (!part) return true;
              const box = part.getBoundingClientRect();
              return box.left >= name.left - 0.5 && box.right <= name.right + 0.5;
            };
            const kept = tab.querySelector('.config-editor-documents__name-kept');
            const start = tab.querySelector('.config-editor-documents__name-start');
            const end = tab.querySelector('.config-editor-documents__name-end');
            const cut = start.scrollWidth > start.clientWidth + 0.5;
            return {
              reads: `${kept?.textContent ?? ''}${cut ? '…' : start.textContent}${end.textContent}`,
              whole: inside(kept) && inside(end),
            };
          }),
        );
        expect(read.every(({ whole }) => whole)).toBe(true);
        expect(new Set(read.map(({ reads }) => reads)).size).toBe(4);
      });
    });
  }
});

// A group of its own (needs the built npm packages, a 900×500 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 900, height: 500 } });

  // At phone width a short folder before a shared file's name
  // (prod/limits.toml) was cut by a pixel or four (the touch screen's
  // wider close button took its room), and a tab that cannot be closed
  // kept the close button's room empty.
  for (const [width, touch] of [[360, true], [600, false], [1200, false]]) {
    test.describe(`at ${width} px`, () => {
      test.use({ viewport: { width, height: 400 }, hasTouch: touch, isMobile: touch });
      test('a short folder shows whole, and a tab that cannot close keeps no empty room', async ({ page }) => {
        await page.goto(pathToFileURL(join(js, 'examples', 'documents.html')).href);
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await page.evaluate(() =>
          window.documentsView.update({
            documents: [
              { id: 'a', fileName: 'clusters/eu-west-1/production/limits.toml', value: 'a = 1\n', closable: false },
              { id: 'b', fileName: 'clusters/us-east-1/production/limits.toml', value: 'a = 1\n' },
              { id: 'c', fileName: 'prod/limits.toml', value: 'a = 1\n' },
              { id: 'd', fileName: 'x.yaml', value: 'a: 1\n', closable: false },
              { id: 'e', fileName: 'y.yaml', value: 'a: 1\n', baseline: 'a: 2\n', closable: false },
            ],
          }),
        );
        await expect(page.locator('[role="tab"]')).toHaveCount(5);
        const tabs = () =>
          page.evaluate(() =>
            [...document.querySelectorAll('[role="tab"]')].map((tab) => {
              const name = tab.querySelector('.config-editor-documents__name');
              const start = name.querySelector('.config-editor-documents__name-start');
              return {
                cut: start.scrollWidth > start.clientWidth + 0.5 || name.scrollWidth > name.clientWidth + 0.5,
                room: tab.getBoundingClientRect().right - name.getBoundingClientRect().right,
              };
            }),
          );
        const [eu, , prod, x] = await tabs();
        expect(prod.cut).toBe(false);
        // The long folders still give way.
        expect(eu.cut).toBe(true);
        // Only the tab's own padding after the name.
        expect(x.room).toBeLessThanOrEqual(8);
        // Unsaved, it shows its dot.
        await expect(page.locator('[role="tab"]').nth(4).locator('.config-editor-documents__dirty-dot')).toBeVisible();
      });
    });
  }
});

// --- The active tab in view after a reorder. -------------------------

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
