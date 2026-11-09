import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES, setupEditor } from './helpers.mjs';

// Several documents as tabs (`ConfigDocuments`), on the demo's `?documents`
// page: `main` (burrow.yaml), `limits` (conf.d/limits.toml) and `clients`
// (conf.d/clients.json, read-only). The host keeps the list, saves (the
// baseline becomes the saved text), asks before closing a document with
// unsaved edits and makes a new drop-in for the "+"; what it saw is on
// `.demo__documents-status`.
//
// Roles: the strip holds a `tablist` of `tab`s (each controlling its
// `tabpanel`) and, after it, the "+" (`on_add`): a plain `button` outside
// the tablist, next in the `Tab` order; the arrow keys cover only the tabs.
// A tab's close button (`tabindex=-1`, inside the tab) is drawn only while
// its document may be closed.

const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const panel = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"]`);
const input = (page, id) => panel(page, id).locator('.config-editor__input');
const status = (page) => page.locator('.demo__documents-status');
const row = (page, id, line) => panel(page, id).locator(`.config-editor__gutter-content > div[data-line="${line}"]`);
const toggle = (page, id, line) => panel(page, id).locator(`.config-editor__fold-toggle[data-line="${line}"]`);
const chip = (page, id, line) => panel(page, id).locator(`.config-editor__fold-chip[data-line="${line}"]`);
const caret = (page, id) => input(page, id).evaluate((element) => [element.selectionStart, element.selectionEnd]);
const focusedDocument = (page) =>
  page.evaluate(() => document.activeElement?.closest('[role="tabpanel"]')?.dataset.documentId ?? null);
const plus = (page) => page.locator('.config-editor-documents__add');
const closeButton = (page, id) => tab(page, id).locator('.config-editor-documents__close');
const focusedRole = (page) => page.evaluate(() => document.activeElement?.getAttribute('role') ?? document.activeElement?.tagName);

async function expectActive(page, id) {
  await expect(tab(page, id)).toHaveAttribute('aria-selected', 'true');
  await expect(panel(page, id)).not.toHaveClass(/panel-hidden/);
  await expect(status(page)).toHaveAttribute('data-active', id);
}

/** Puts the caret at `offset` in a document's textarea, with the mouse-free API a user's click ends in. */
async function placeCaret(page, id, offset) {
  await input(page, id).click();
  await input(page, id).evaluate((element, at) => element.setSelectionRange(at, at), offset);
}

test.describe('documents as tabs', () => {
  setupEditor(test, '?documents');

  test('one tab per document with its icon and name; each panel edits its own language', async ({ page }) => {
    await expect(page.locator('[role="tablist"]')).toHaveAttribute('aria-label', 'Open files');
    await expect(page.locator('[role="tab"]')).toHaveCount(3);
    await expect(page.locator('[role="tab"] .config-editor-documents__name')).toHaveText([
      'burrow.yaml',
      'conf.d/limits.toml',
      'conf.d/clients.json',
    ]);
    const icons = await page.locator('[role="tab"] .config-editor-documents__icon').allTextContents();
    expect(new Set(icons).size).toBe(3);
    await expectActive(page, 'main');
    await expect(tab(page, 'main')).toHaveAttribute('aria-controls', (await panel(page, 'main').getAttribute('id')));
    await expect(panel(page, 'main')).toHaveAttribute('aria-labelledby', (await tab(page, 'main').getAttribute('id')));

    await tab(page, 'limits').click();
    await expectActive(page, 'limits');
    await expect(input(page, 'limits')).toHaveValue(/max_pending = 512/);
    await expect(panel(page, 'limits').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
    await expect(panel(page, 'main')).toHaveClass(/panel-hidden/);
    await expect(panel(page, 'main')).toHaveAttribute('inert', '');
    // A schema problem in the TOML drop-in is marked in its own editor.
    await input(page, 'limits').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('bogus_key = 1');
    await expect(panel(page, 'limits').locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    await expect(panel(page, 'main').locator('.config-editor__diagnostic-marker')).toHaveCount(0);
    await expect(page.locator('.demo__documents-values')).toContainText('bogus_key = 1');
  });

  test('editors mount the first time their document is shown, then stay', async ({ page }) => {
    await expect(page.locator('.config-editor-documents .config-editor')).toHaveCount(1);
    await expect(panel(page, 'limits').locator('.config-editor')).toHaveCount(0);
    await tab(page, 'limits').click();
    await expect(page.locator('.config-editor-documents .config-editor')).toHaveCount(2);
    await tab(page, 'main').click();
    await tab(page, 'limits').click();
    await expect(page.locator('.config-editor-documents .config-editor')).toHaveCount(2);
    // One stylesheet for the whole view.
    await expect(page.locator('style')).toHaveCount(1);
  });

  test('switching keeps each document\'s caret, selection, undo history and folds', async ({ page }) => {
    // main: select "7835", type over it, then fold `web:` (line 5).
    const mainText = await input(page, 'main').inputValue();
    const at = mainText.indexOf('7835');
    await input(page, 'main').click();
    await input(page, 'main').evaluate((element, start) => element.setSelectionRange(start, start + 4), at);
    await page.keyboard.type('9999');
    await row(page, 'main', 5).hover();
    await toggle(page, 'main', 5).click();
    await expect(chip(page, 'main', 5)).toBeVisible();
    await placeCaret(page, 'main', at + 2);
    await page.keyboard.press('Shift+ArrowRight');

    // limits: an edit of its own.
    await tab(page, 'limits').click();
    await input(page, 'limits').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# limits edit');

    await tab(page, 'main').click();
    await expectActive(page, 'main');
    expect(await caret(page, 'main')).toEqual([at + 2, at + 3]);
    await expect(chip(page, 'main', 5)).toBeVisible();
    await input(page, 'main').focus();
    await page.keyboard.press('Control+z');
    await expect(input(page, 'main')).toHaveValue(/listen: 7835/);
    // Undo in main left limits alone.
    await expect(input(page, 'limits')).toHaveValue(/# limits edit/);
    await tab(page, 'limits').click();
    await input(page, 'limits').focus();
    await page.keyboard.press('Control+z');
    await expect(input(page, 'limits')).not.toHaveValue(/# limits edit/);
  });

  test('the dirty dot follows edits and the baseline; Ctrl+S saves the active document with its id', async ({ page }) => {
    await tab(page, 'limits').click();
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'false');
    await expect(tab(page, 'limits').locator('.config-editor-documents__dirty-dot')).toHaveCSS('visibility', 'hidden');
    await input(page, 'limits').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# more');
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'true');
    await page.mouse.move(5, 500);
    await expect(tab(page, 'limits').locator('.config-editor-documents__dirty-dot')).toHaveCSS('visibility', 'visible');
    await expect(tab(page, 'limits')).toHaveAttribute('aria-label', 'conf.d/limits.toml, unsaved changes');
    await expect(status(page)).toHaveAttribute('data-dirty', 'limits');
    await expect(tab(page, 'main')).toHaveAttribute('data-dirty', 'false');

    // Undoing back to the baseline clears it.
    await page.keyboard.press('Control+z');
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'false');
    await page.keyboard.type('# saved');
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'true');
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', 'limits:TOML');
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'false');
    await expect(status(page)).toHaveAttribute('data-dirty', '');

    // Save all, through the handle: only the dirty documents.
    await tab(page, 'main').click();
    await input(page, 'main').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# main edit');
    await page.locator('.demo__save-all').click();
    await expect(status(page)).toHaveAttribute('data-saves', 'limits:TOML,main:YAML');
  });

  test('closing a dirty document asks the host; a clean one closes at once', async ({ page }) => {
    await input(page, 'main').click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# unsaved');
    await tab(page, 'main').hover();
    await tab(page, 'main').locator('.config-editor-documents__close').click();
    await expect(page.locator('.demo__ask')).toContainText('Discard unsaved changes to main?');
    await expect(status(page)).toHaveAttribute('data-open', 'main,limits,clients');
    await page.locator('.demo__keep').click();
    await expect(page.locator('[role="tab"]')).toHaveCount(3);

    // Delete on the focused tab asks the same.
    await tab(page, 'main').focus();
    await page.keyboard.press('Delete');
    await expect(page.locator('.demo__ask')).toBeVisible();
    await page.locator('.demo__discard').click();
    await expect(status(page)).toHaveAttribute('data-open', 'limits,clients');
    // The tab after the closed one becomes active.
    await expectActive(page, 'limits');
    await expect(status(page)).toHaveAttribute('data-selects', 'limits');

    // clients is clean: a middle click closes it without asking.
    await tab(page, 'clients').click({ button: 'middle' });
    await expect(status(page)).toHaveAttribute('data-open', 'limits');
    await expect(page.locator('.demo__ask')).toHaveCount(0);
    await expectActive(page, 'limits');
  });

  test('read-only is per document and switches live', async ({ page }) => {
    await tab(page, 'clients').click();
    await expect(tab(page, 'clients')).toHaveAttribute('aria-label', 'conf.d/clients.json, read-only');
    await expect(panel(page, 'clients').locator('.config-editor')).toHaveClass(/config-editor__readonly/);
    const before = await input(page, 'clients').inputValue();
    await input(page, 'clients').click();
    await page.keyboard.type('zzz');
    await expect(input(page, 'clients')).toHaveValue(before);

    await tab(page, 'limits').click();
    await expect(panel(page, 'limits').locator('.config-editor')).not.toHaveClass(/config-editor__readonly/);
    await page.locator('.demo__toggle-read-only').click();
    await expect(panel(page, 'limits').locator('.config-editor')).toHaveClass(/config-editor__readonly/);
    await expect(tab(page, 'limits')).toHaveClass(/tab-readonly/);
  });

  test('the tab list takes arrows, Home, End and Enter; Alt+PageDown switches from inside an editor', async ({ page }) => {
    await tab(page, 'main').click();
    await expect(tab(page, 'main')).toBeFocused();
    await expect(tab(page, 'limits')).toHaveAttribute('tabindex', '-1');
    await page.keyboard.press('ArrowRight');
    await expect(tab(page, 'limits')).toBeFocused();
    await expectActive(page, 'limits');
    await expect(tab(page, 'limits')).toHaveAttribute('tabindex', '0');
    await page.keyboard.press('End');
    await expect(tab(page, 'clients')).toBeFocused();
    await page.keyboard.press('Home');
    await expect(tab(page, 'main')).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(tab(page, 'clients')).toBeFocused();
    await expectActive(page, 'clients');
    // Alt+PageDown in the tab list moves the focus along the tabs too.
    await page.keyboard.press('Alt+PageDown');
    await expect(tab(page, 'main')).toBeFocused();
    await page.keyboard.press('Enter');
    await expect.poll(() => focusedDocument(page)).toBe('main');
    expect(await focusedRole(page)).toBe('textbox');

    // From inside an editor: the next document's editor takes the focus,
    // with its caret where it was.
    await placeCaret(page, 'main', 10);
    await page.keyboard.press('Alt+PageDown');
    await expectActive(page, 'limits');
    await expect.poll(() => focusedDocument(page)).toBe('limits');
    await placeCaret(page, 'limits', 5);
    await page.keyboard.press('Alt+PageUp');
    await expectActive(page, 'main');
    await expect.poll(() => focusedDocument(page)).toBe('main');
    expect(await caret(page, 'main')).toEqual([10, 10]);
    // The shortcut typed nothing.
    await expect(input(page, 'main')).not.toHaveValue(/PageDown/);
    await page.keyboard.press('Alt+PageDown');
    await expect.poll(() => focusedDocument(page)).toBe('limits');
    expect(await caret(page, 'limits')).toEqual([5, 5]);
  });

  test('the host renames and adds documents; a new one mounts when first shown', async ({ page }) => {
    await placeCaret(page, 'main', 12);
    await page.locator('.demo__rename').click();
    await expect(tab(page, 'limits').locator('.config-editor-documents__name')).toHaveText('conf.d/10-limits.toml');
    await page.locator('.demo__add').click();
    await expect(page.locator('[role="tab"]')).toHaveCount(4);
    await expect(panel(page, 'extra1').locator('.config-editor')).toHaveCount(0);
    await tab(page, 'extra1').click();
    await expect(panel(page, 'extra1').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'HCL editor');
    await tab(page, 'main').click();
    expect(await caret(page, 'main')).toEqual([12, 12]);
  });

  test('a new baseline from the host reaches its document only', async ({ page }) => {
    await page.locator('.demo__server-change').click();
    // main is clean: the new baseline is taken silently; the diff marks listen.
    await expect(tab(page, 'main')).toHaveAttribute('data-dirty', 'true');
    await expect(panel(page, 'main').locator('.config-editor__notice')).toBeHidden();
    await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'false');
  });

  test('editing as another language is per document and survives a switch; saves stay in the document\'s language', async ({ page }) => {
    await page.locator('.demo__edit-as').click();
    await expect(input(page, 'main')).toHaveValue(/"listen": 7835/);
    const text = await input(page, 'main').inputValue();
    const at = text.indexOf('7835');
    await input(page, 'main').click();
    await input(page, 'main').evaluate((element, start) => element.setSelectionRange(start, start + 4), at);
    await page.keyboard.type('7000');
    await expect(page.locator('.demo__documents-values')).toContainText('listen: 7000');
    await page.keyboard.press('Alt+PageDown');
    await expectActive(page, 'limits');
    await expect(input(page, 'limits')).toHaveValue(/\[limits\]/);
    await page.keyboard.press('Alt+PageUp');
    await expect.poll(() => focusedDocument(page)).toBe('main');
    await expect(input(page, 'main')).toHaveValue(/"listen": 7000/);
    expect(await caret(page, 'main')).toEqual([at + 4, at + 4]);
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', 'main:YAML');
    await expect(tab(page, 'main')).toHaveAttribute('data-dirty', 'false');
  });

  test('the find widget stays open with its query in its own document', async ({ page }) => {
    await input(page, 'main').click();
    await page.keyboard.press('Control+f');
    await page.keyboard.type('listen');
    const find = panel(page, 'main').locator('.config-editor__find');
    await expect(find).toBeVisible();
    await tab(page, 'limits').click();
    await expect(panel(page, 'limits').locator('.config-editor__find')).toBeHidden();
    await tab(page, 'main').click();
    await expect(find).toBeVisible();
    await expect(find.locator('.config-editor__find-input').first()).toHaveValue('listen');
  });

  test('the "+" is a button after the tab list that asks the host; its new document is selected and focused, the page stays', async ({ page }) => {
    const add = page.getByRole('button', { name: 'New file' });
    await expect(add).toHaveAttribute('title', 'New file');
    expect(await add.evaluate((element) => element.closest('[role="tablist"]') === null)).toBe(true);
    expect(await add.evaluate((element) => element.closest('.config-editor-documents__strip') !== null)).toBe(true);
    // As tall as a tab, right after the last one.
    const [plusBox, lastBox] = [await add.boundingBox(), await tab(page, 'clients').boundingBox()];
    expect(Math.abs(plusBox.height - lastBox.height)).toBeLessThanOrEqual(1);
    expect(Math.abs(plusBox.x - (lastBox.x + lastBox.width))).toBeLessThanOrEqual(1);

    // The arrows stay on the tabs; Tab from the tab list reaches the "+".
    await tab(page, 'clients').click();
    await page.keyboard.press('ArrowRight');
    await expect(tab(page, 'main')).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(add).toBeFocused();

    // Enter, Space and a click each ask once; the host's new document is
    // selected and its editor focused.
    await page.keyboard.press('Enter');
    await expect(page.locator('[role="tab"]')).toHaveCount(4);
    await expectActive(page, 'extra1');
    await expect.poll(() => focusedDocument(page)).toBe('extra1');
    await add.focus();
    await page.keyboard.press(' ');
    await expectActive(page, 'extra2');
    await expect.poll(() => focusedDocument(page)).toBe('extra2');
    await expect(page.locator('[role="tab"]')).toHaveCount(5);

    // A page taller than the window, scrolled: the focus moves without
    // scrolling it.
    await page.addStyleTag({ content: 'body { padding-bottom: 4000px; }' });
    await page.evaluate(() => window.scrollTo(0, 40));
    const before = await page.evaluate(() => window.scrollY);
    expect(before).toBeGreaterThan(0);
    await add.evaluate((element) => element.click());
    await expectActive(page, 'extra3');
    await expect.poll(() => focusedDocument(page)).toBe('extra3');
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100))));
    expect(await page.evaluate(() => window.scrollY)).toBe(before);
    await expect(status(page)).toHaveAttribute('data-selects', /,extra1,extra2,extra3$/);
  });

  test('with many tabs the "+" stays at the end of the strip', async ({ page }) => {
    for (let index = 0; index < 10; index += 1) await page.locator('.demo__add').click();
    const strip = page.locator('[role="tablist"]');
    const sizes = await strip.evaluate((element) => ({ scroll: element.scrollWidth, client: element.clientWidth }));
    expect(sizes.scroll).toBeGreaterThan(sizes.client);
    const edge = async () => {
      const [outer, box] = [await page.locator('.config-editor-documents__strip').boundingBox(), await plus(page).boundingBox()];
      return Math.round(outer.x + outer.width - (box.x + box.width));
    };
    // At the strip's right edge (inside its 1px border), with the tabs scrolled either way.
    await strip.evaluate((element) => (element.scrollLeft = 0));
    expect(await edge()).toBeLessThanOrEqual(1);
    await strip.evaluate((element) => (element.scrollLeft = element.scrollWidth));
    expect(await edge()).toBeLessThanOrEqual(1);
    // The last tab, selected, shows whole beside it.
    await tab(page, 'main').click();
    await page.keyboard.press('End');
    await expect(tab(page, 'extra10')).toBeFocused();
    await expect
      .poll(async () => {
        const [last, box] = [await tab(page, 'extra10').boundingBox(), await plus(page).boundingBox()];
        return last.x + last.width <= box.x + 1;
      })
      .toBe(true);
    await plus(page).click();
    await expectActive(page, 'extra11');
    await expect.poll(() => focusedDocument(page)).toBe('extra11');
  });

  test('only closable documents close: no close button, middle click or Delete on the others', async ({ page }) => {
    await expect(closeButton(page, 'main')).toHaveCount(1);
    await page.locator('.demo__toggle-closable').click();
    await expect(closeButton(page, 'main')).toHaveCount(0);
    await expect(tab(page, 'main').locator('.config-editor-documents__marker')).toHaveCount(1);
    await expect(tab(page, 'main')).toHaveAttribute('aria-label', 'burrow.yaml');
    await expect(closeButton(page, 'limits')).toHaveCount(1);
    await tab(page, 'main').click({ button: 'middle' });
    await tab(page, 'main').focus();
    await page.keyboard.press('Delete');
    await expect(status(page)).toHaveAttribute('data-open', 'main,limits,clients');
    // The closable ones still close.
    await tab(page, 'clients').click({ button: 'middle' });
    await expect(status(page)).toHaveAttribute('data-open', 'main,limits');
    await tab(page, 'limits').focus();
    await page.keyboard.press('Delete');
    await expect(status(page)).toHaveAttribute('data-open', 'main');
    // Closable again, live.
    await page.locator('.demo__toggle-closable').click();
    await expect(closeButton(page, 'main')).toHaveCount(1);
    await closeButton(page, 'main').click();
    await expect(status(page)).toHaveAttribute('data-open', '');
  });

  test('the tab colors come from the theme variables', async ({ page }) => {
    const colors = await page.locator('.config-editor-documents').evaluate((root) => {
      const style = getComputedStyle(root);
      const active = getComputedStyle(root.querySelector('.config-editor-documents__tab-active'));
      return {
        variable: style.getPropertyValue('--config-editor-tab-active-bg').trim(),
        editorBg: style.getPropertyValue('--config-editor-bg').trim(),
        active: active.backgroundColor,
      };
    });
    expect(colors.variable).toBe(colors.editorBg);
    expect(colors.active).toBe('rgb(30, 30, 30)');
  });
});

test.describe('documents at phone width', () => {
  for (const width of [390, 320]) {
    test(`${width}px: the tabs scroll sideways, the page does not`, async ({ page, baseURL }) => {
      await page.setViewportSize({ width, height: 700 });
      await page.goto(`${baseURL}?documents`);
      await page.locator('.config-editor__input').waitFor();
      for (let index = 0; index < 3; index += 1) await page.locator('.demo__add').click();
      const strip = page.locator('[role="tablist"]');
      const sizes = await strip.evaluate((element) => ({ scroll: element.scrollWidth, client: element.clientWidth }));
      expect(sizes.scroll).toBeGreaterThan(sizes.client);
      const pageWidth = await page.evaluate(() => document.documentElement.scrollWidth);
      expect(pageWidth).toBeLessThanOrEqual(width);
      // The last tab, selected from the keyboard, is scrolled into view.
      await tab(page, 'main').click();
      await page.keyboard.press('End');
      await expect(tab(page, 'extra3')).toBeFocused();
      await expect.poll(() => strip.evaluate((element) => element.scrollLeft)).toBeGreaterThan(0);
      const box = await tab(page, 'extra3').boundingBox();
      expect(box.x + box.width).toBeLessThanOrEqual(width + 1);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    });
  }
});

test.describe('the "+" on a phone', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
  setupEditor(test, '?documents');

  test('390px: visible at the strip\'s end and tappable', async ({ page }) => {
    for (let index = 0; index < 3; index += 1) await page.locator('.demo__add').click();
    await expect(plus(page)).toBeInViewport({ ratio: 1 });
    const box = await plus(page).boundingBox();
    expect(box.width).toBeGreaterThanOrEqual(24);
    expect(box.height).toBeGreaterThanOrEqual(24);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    await plus(page).tap();
    await expectActive(page, 'extra4');
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
});

test.describe('documents keep their scroll position', () => {
  setupEditor(test, '?documents=many');

  test('a scrolled large document is where it was after a switch', async ({ page }) => {
    await expect(page.locator('[role="tab"]')).toHaveCount(10);
    await input(page, 'doc0').hover();
    await page.mouse.wheel(0, 4000);
    await expect.poll(() => input(page, 'doc0').evaluate((element) => element.scrollTop)).toBeGreaterThan(1000);
    const scrolled = await input(page, 'doc0').evaluate((element) => element.scrollTop);
    const firstLine = async () =>
      Number(await panel(page, 'doc0').locator('.config-editor__gutter-content > div[data-line]').first().getAttribute('data-line'));
    const lineBefore = await firstLine();
    await tab(page, 'doc1').click();
    await expect(input(page, 'doc1')).toHaveValue(/"listen"/);
    await tab(page, 'doc0').click();
    expect(await input(page, 'doc0').evaluate((element) => element.scrollTop)).toBe(scrolled);
    await expect.poll(firstLine).toBe(lineBefore);
    await expect(panel(page, 'doc0').locator('.config-editor__gutter-content > div[data-line]').first()).toBeVisible();
  });
});

// The npm packages' example pages, built with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('JS example documents.html: the "+" and documents added later', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // QA 23: a "+" the host did not answer stayed armed, and a document a
  // server pushed later took the selection and the focus from a field of
  // the page.
  test('only the document added in answer to the "+" is selected; one pushed after the focus left is not', async ({ page }) => {
    await page.goto(example('documents.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').waitFor();
    await page.evaluate(() => {
      window.documentsView.update({ onAdd: () => {} });
      document.body.insertAdjacentHTML('beforeend', '<input id="other" aria-label="other">');
    });
    const other = page.locator('#other');
    await plus(page).click();
    await other.click();
    await page.keyboard.type('hel');
    await page.evaluate(() => window.addFile({ id: 'pushed', fileName: 'pushed.yaml', value: 'x: 1\n' }));
    await expect(tab(page, 'pushed')).toHaveCount(1);
    await page.keyboard.type('lo');
    await expect(tab(page, 'main')).toHaveAttribute('aria-selected', 'true');
    await expect(other).toBeFocused();
    await expect(other).toHaveValue('hello');

    // An answer a moment later, the focus still on the "+": selected and focused.
    await page.evaluate(() => {
      window.documentsView.update({
        onAdd: () => setTimeout(() => window.addFile({ id: 'answer', fileName: 'answer.yaml', value: '' }), 300),
      });
    });
    await plus(page).click();
    await expect(tab(page, 'answer')).toHaveAttribute('aria-selected', 'true');
    await expect.poll(() => focusedDocument(page)).toBe('answer');
  });

  // QA 24: clicking or typing in the document's editor after the "+" left
  // the request waiting: a document pushed then took the next keys.
  for (const how of ['click', 'type']) {
    test(`a "+" the user goes on from (${how} in the editor) selects no document added later`, async ({ page }) => {
      await page.goto(example('documents.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const input = page.locator('.config-editor__input').first();
      await input.waitFor();
      await page.evaluate(() => window.documentsView.update({ onAdd: () => {} }));
      if (how === 'click') {
        await plus(page).click();
        await input.click();
      } else {
        await input.focus();
        await plus(page).evaluate((element) => element.click());
        await expect(input).toBeFocused();
      }
      await page.keyboard.press('Control+End');
      await page.keyboard.type('# a');
      await page.evaluate(() => window.addFile({ id: 'pushed', fileName: 'pushed.yaml', value: 'x: 1\n' }));
      await expect(tab(page, 'pushed')).toHaveCount(1);
      await page.keyboard.type('b');
      await expect(tab(page, 'main')).toHaveAttribute('aria-selected', 'true');
      expect(await focusedDocument(page)).toBe('main');
      expect(await input.inputValue()).toMatch(/# ab$/);
    });
  }
});

// A new view on `#sized` (made by the example's own class), its container
// styled with `css`.
async function sizedView(page, css, documents) {
  await page.goto(example('documents.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(([css, documents]) => {
    const style = document.createElement('style');
    style.textContent = css;
    document.head.append(style);
    const host = document.createElement('div');
    host.id = 'sized';
    document.body.prepend(host);
    window.sized = new window.documentsView.constructor(host, { documents, showFooter: true });
  }, [css, documents]);
  await page.locator('#sized .config-editor__input').first().waitFor();
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100))));
}

const big = `${Array.from({ length: 200 }, (_, index) => `k${index}: ${index}`).join('\n')}\n`;

test.describe('JS example documents.html: sizing and focus from code', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  // QA 24: with `height: 100%` the strip collapsed to a pixel and the
  // editor ran past the container.
  for (const count of [1, 3]) {
    test(`height 100% fills a 60vh container: the strip shows, the editor fills the rest (${count} document${count > 1 ? 's' : ''})`, async ({ page }) => {
      const documents = Array.from({ length: count }, (_, index) => ({ id: `d${index}`, fileName: `d${index}.yaml`, value: big }));
      await sizedView(page, '#sized { height: 60vh; } #sized .config-editor-documents { height: 100%; }', documents);
      const boxes = await page.evaluate(() => {
        const box = (selector) => {
          const rect = document.querySelector(selector).getBoundingClientRect();
          return { top: Math.round(rect.top), bottom: Math.round(rect.bottom), height: Math.round(rect.height) };
        };
        const shown = '#sized .config-editor-documents__panel:not(.config-editor-documents__panel-hidden)';
        return {
          container: box('#sized'),
          strip: box('#sized .config-editor-documents__strip'),
          editor: box(`${shown} .config-editor`),
          footer: box(`${shown} .config-editor__status`),
        };
      });
      expect(boxes.strip.height).toBeGreaterThanOrEqual(35);
      expect(boxes.strip.top).toBe(boxes.container.top);
      expect(boxes.editor.top).toBe(boxes.strip.bottom);
      // The editor and its footer end at the container's end.
      expect(Math.abs(boxes.editor.bottom - boxes.container.bottom)).toBeLessThanOrEqual(1);
      expect(Math.abs(boxes.footer.bottom - boxes.container.bottom)).toBeLessThanOrEqual(2);
    });
  }

  test('without a height the view sizes itself, as an editor does', async ({ page }) => {
    await sizedView(page, '', [{ id: 'a', fileName: 'a.yaml', value: 'a: 1\n' }, { id: 'b', fileName: 'b.yaml', value: big }]);
    const heights = await page.evaluate(() => {
      const height = (selector) => Math.round(document.querySelector(selector).getBoundingClientRect().height);
      return { strip: height('#sized .config-editor-documents__strip'), surface: height('#sized .config-editor__surface') };
    });
    expect(heights.strip).toBeGreaterThanOrEqual(35);
    expect(heights.surface).toBeGreaterThanOrEqual(200);
  });

  // QA 24: at phone width long names sharing a start were cut alike.
  test('at phone width long names sharing a start keep the end that tells them apart', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 700 });
    const names = ['conf.d/service-config-production.yaml', 'conf.d/service-config-staging.yaml'];
    await sizedView(page, '', names.map((fileName, index) => ({ id: `d${index}`, fileName, value: 'a: 1\n' })));
    const ends = await page.evaluate(() =>
      [...document.querySelectorAll('#sized [role="tab"]')].map((tab) => {
        const end = tab.querySelector('.config-editor-documents__name-end');
        const name = tab.querySelector('.config-editor-documents__name').getBoundingClientRect();
        const box = end.getBoundingClientRect();
        return { text: end.textContent, whole: box.left >= name.left - 0.5 && box.right <= name.right + 0.5, cut: tab.querySelector('.config-editor-documents__name-start').scrollWidth > tab.querySelector('.config-editor-documents__name-start').clientWidth };
      }),
    );
    expect(ends.map(({ text }) => text)).toEqual(['duction.yaml', 'staging.yaml']);
    expect(ends.every(({ whole }) => whole)).toBe(true);
    expect(ends.some(({ cut }) => cut)).toBe(true);
  });

  // QA 24: on a document not shown they answered true and did nothing.
  for (const call of ['reveal', 'revealRange', 'focus']) {
    test(`${call} on the editor of a document not shown selects it first`, async ({ page }) => {
      await sizedView(page, '', [{ id: 'a', fileName: 'a.yaml', value: big }, { id: 'b', fileName: 'b.yaml', value: big }]);
      await page.evaluate(async () => {
        const frame = () => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50)));
        window.sized.select('b');
        await frame();
        window.sized.select('a');
        await frame();
      });
      const answer = await page.evaluate((call) => {
        const editor = window.sized.editor('b');
        if (call === 'reveal') return editor.reveal('/k150');
        if (call === 'revealRange') return editor.revealRange(editor.rangeOf('/k150'));
        editor.focus();
        return true;
      }, call);
      expect(answer).toBe(true);
      await expect.poll(() => page.evaluate(() => window.sized.activeId)).toBe('b');
      const b = page.locator('#sized [role="tabpanel"][data-document-id="b"] .config-editor__input');
      await expect(b).toBeFocused();
      if (call !== 'focus') {
        const caretLine = await b.evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
        expect(caretLine).toBe(151);
        await expect(page.locator('#sized [role="tabpanel"][data-document-id="b"] .config-editor__gutter-content > div[data-line="151"]')).toBeInViewport();
      }
    });
  }
});

for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
  test.describe(`JS example ${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('tabs switch, edits and saves carry the document id, a dirty close asks', async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.locator('.config-editor__input').waitFor();
      await expect(page.locator('[role="tab"]')).toHaveCount(3);

      await tab(page, 'limits').click();
      await input(page, 'limits').click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('# js edit');
      await expect(page.locator('#log')).toContainText('change limits');
      await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'true');
      await page.keyboard.press('Control+s');
      await expect(page.locator('#log')).toContainText('save limits toml');
      await expect(tab(page, 'limits')).toHaveAttribute('data-dirty', 'false');

      const caretBefore = await caret(page, 'limits');
      await page.keyboard.press('Alt+PageUp');
      await expect(tab(page, 'main')).toHaveAttribute('aria-selected', 'true');
      await page.keyboard.press('Alt+PageDown');
      await expect.poll(() => focusedDocument(page)).toBe('limits');
      expect(await caret(page, 'limits')).toEqual(caretBefore);

      await page.keyboard.type('# again');
      page.once('dialog', (dialog) => dialog.dismiss());
      await tab(page, 'limits').hover();
      await tab(page, 'limits').locator('.config-editor-documents__close').click();
      await expect(page.locator('[role="tab"]')).toHaveCount(3);
      page.once('dialog', (dialog) => dialog.accept());
      await tab(page, 'limits').locator('.config-editor-documents__close').click();
      await expect(page.locator('[role="tab"]')).toHaveCount(2);
      await expect(page.locator('#log')).toContainText('select clients');
      expect(errors).toEqual([]);
    });

    test('the "+" asks the host, whose document is selected and focused; only closable tabs close', async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.locator('.config-editor__input').waitFor();
      // main is not closable: no close button, and neither a middle click nor Delete asks.
      await expect(closeButton(page, 'main')).toHaveCount(0);
      await expect(closeButton(page, 'limits')).toHaveCount(1);
      await tab(page, 'main').click({ button: 'middle' });
      await tab(page, 'main').focus();
      await page.keyboard.press('Delete');
      await expect(page.locator('[role="tab"]')).toHaveCount(3);

      await page.addStyleTag({ content: 'body { padding-bottom: 4000px; }' });
      await page.evaluate(() => window.scrollTo(0, 20));
      const before = await page.evaluate(() => window.scrollY);
      const add = page.getByRole('button', { name: 'New file' });
      expect(await add.evaluate((element) => element.closest('[role="tablist"]') === null)).toBe(true);
      await add.evaluate((element) => element.click());
      await expect(page.locator('#log')).toContainText('add new1');
      await expect(tab(page, 'new1')).toHaveAttribute('aria-selected', 'true');
      await expect.poll(() => focusedDocument(page)).toBe('new1');
      await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 100))));
      expect(await page.evaluate(() => window.scrollY)).toBe(before);
      await expect(page.locator('#log')).toContainText('select new1');

      // With many tabs the "+" is still at hand.
      for (let index = 0; index < 8; index += 1) await add.click();
      await expect(tab(page, 'new9')).toHaveAttribute('aria-selected', 'true');
      await expect(add).toBeInViewport({ ratio: 1 });

      // Closable, live.
      await page.evaluate(() => window.updateFile('main', { closable: true }));
      await expect(closeButton(page, 'main')).toHaveCount(1);
      await closeButton(page, 'main').click();
      await expect(tab(page, 'main')).toHaveCount(0);
      expect(errors).toEqual([]);
    });
  });
}
