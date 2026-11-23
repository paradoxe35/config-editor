import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// Find uses (`Shift+F12`, the hover card's Uses link, `findUses()`): the
// list of a name's uses under it, grouped by file, moved through with the
// keyboard (the focus stays in the text, which points at the row on), the
// host's uses joining it late, a chosen use gone to (a tab switch in a
// documents view), and the hover card's "Definition · Uses" links by
// mouse, keyboard and tap.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const ID = 'https://example.com/server';
const SCHEMA = JSON.stringify({
  $id: ID,
  type: 'object',
  properties: {
    clients: { type: 'object' },
    default_client: { type: 'string', 'x-config-editor-keys': '/clients' },
    members: { type: 'array', items: { type: 'string', 'x-config-editor-keys': '/clients' } },
    note: { type: 'string', examples: ['main'] },
  },
});
const VALUE = 'clients:\n  main: {}\n  lab: {}\ndefault_client: main\nmembers: [lab, main]\nnote: main\n';

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();
const list = (page) => page.locator('.config-editor__uses');
const rows = (page) => list(page).locator('[role="option"]');
const said = (page) => page.locator('.config-editor__announcement').first();

function collectErrors(page) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  return errors;
}

async function openEditor(page, extra = {}) {
  const errors = collectErrors(page);
  await page.goto(vanilla({ value: VALUE, schema: SCHEMA, ...extra }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await field(page).waitFor();
  await field(page).click();
  return errors;
}

/** Puts the caret `into` characters into the `nth` `word` of the text. */
const caretOn = (page, word, nth = 0, into = 1) =>
  field(page).evaluate(
    (element, [word, nth, into]) => {
      let at = -1;
      for (let index = 0; index <= nth; index += 1) at = element.value.indexOf(word, at + 1);
      element.setSelectionRange(at + into, at + into);
    },
    [word, nth, into],
  );

/** The text selected in the editor. */
const selected = (page) => field(page).evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));

/** The pixel position of the `nth` `word` of `line` (zero-based) in the text area, measured in its font. */
const placeOf = (page, line, before) =>
  field(page).evaluate(
    (element, [line, before]) => {
      const style = getComputedStyle(element);
      const probe = document.createElement('span');
      probe.style.font = style.font;
      probe.style.whiteSpace = 'pre';
      probe.textContent = before;
      document.body.append(probe);
      const width = probe.getBoundingClientRect().width;
      probe.remove();
      const height = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
      return { x: parseFloat(style.paddingLeft) + width + 4, y: parseFloat(style.paddingTop) + height * (line + 0.5) };
    },
    [line, before],
  );

test.describe('find uses in one editor, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Shift+F12 on a use lists the uses, the keyboard moves and goes, Escape closes', async ({ page }) => {
    const errors = await openEditor(page);
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    await expect(list(page).locator('.config-editor__uses-file')).toHaveText(['this document']);
    await expect(rows(page).locator('.config-editor__use-text')).toHaveText(['default_client: main', 'members: [lab, main]']);
    await expect(said(page)).toContainText('2 uses');
    // The focus stays in the text, which points at the row on.
    await expect(field(page)).toBeFocused();
    const listbox = list(page).locator('[role="listbox"]');
    await expect(field(page)).toHaveAttribute('aria-controls', await listbox.getAttribute('id'));
    await expect(field(page)).toHaveAttribute('aria-activedescendant', await rows(page).nth(0).getAttribute('id'));
    await expect(rows(page).nth(0)).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('ArrowDown');
    await expect(rows(page).nth(1)).toHaveAttribute('aria-selected', 'true');
    await expect(field(page)).toHaveAttribute('aria-activedescendant', await rows(page).nth(1).getAttribute('id'));
    await page.keyboard.press('ArrowDown');
    await expect(rows(page).nth(0)).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('Enter');
    await expect(list(page)).toHaveCount(0);
    await expect.poll(() => selected(page)).toBe('main');
    expect(await field(page).evaluate((element) => element.value.slice(0, element.selectionStart).endsWith('[lab, '))).toBe(
      true,
    );
    await expect(field(page)).not.toHaveAttribute('aria-activedescendant', /./);
    // Escape closes without moving the caret.
    await caretOn(page, 'main', 0);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    await page.keyboard.press('Escape');
    await expect(list(page)).toHaveCount(0);
    await expect(field(page)).toBeFocused();
    expect(errors).toEqual([]);
  });

  test('a click on a row goes there; a key that is no list key closes the list', async ({ page }) => {
    const errors = await openEditor(page);
    await caretOn(page, 'main', 0);
    await page.keyboard.press('Shift+F12');
    await rows(page).nth(1).click();
    await expect(list(page)).toHaveCount(0);
    await expect.poll(() => selected(page)).toBe('main');
    await expect(field(page)).toBeFocused();
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    await page.keyboard.press('ArrowRight');
    await expect(list(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('without a host search an empty list says "No uses in open files"; with one, its uses join late', async ({
    page,
  }) => {
    const errors = await openEditor(page, { value: 'clients:\n  main: {}\n  lab: {}\nnote: lab\n' });
    // `note: lab` is free text: no use.
    await caretOn(page, 'lab', 0);
    await page.keyboard.press('Shift+F12');
    await expect(list(page).locator('.config-editor__uses-empty')).toHaveText('No uses in open files');
    await expect(said(page)).toContainText('No uses in open files');
    await page.keyboard.press('Escape');
    // The host answers late: "Searching" first, then its uses join.
    await page.evaluate(() => {
      window.answerUses = ({ name }) =>
        new Promise((resolve) =>
          setTimeout(
            () => resolve([{ source: 'server.d/lab.yaml', path: '/routes/0/client', line: 3, text: `  client: ${name}` }]),
            300,
          ),
        );
    });
    await page.keyboard.press('Shift+F12');
    await expect(list(page).locator('.config-editor__uses-empty')).toHaveText('Searching other files…');
    await expect(rows(page)).toHaveCount(1);
    await expect(list(page).locator('.config-editor__uses-file')).toHaveText(['server.d/lab.yaml']);
    await expect(rows(page).locator('.config-editor__use-line')).toHaveText(['3']);
    await expect(said(page)).toContainText('1 use');
    expect(await page.evaluate(() => window.findUsesAsked.at(-1))).toEqual({ schema: ID, pointer: '/clients', name: 'lab' });
    await page.keyboard.press('Enter');
    await expect.poll(() => page.evaluate(() => window.wentToUse)).toEqual([
      { source: 'server.d/lab.yaml', path: '/routes/0/client', name: 'lab' },
    ]);
    // An answer with nothing: "No uses".
    await page.evaluate(() => (window.answerUses = () => Promise.resolve([])));
    await caretOn(page, 'lab', 0);
    await page.keyboard.press('Shift+F12');
    await expect(list(page).locator('.config-editor__uses-empty')).toHaveText('No uses');
    expect(errors).toEqual([]);
  });

  test('a stale host answer is dropped', async ({ page }) => {
    const errors = await openEditor(page);
    await page.evaluate(() => {
      let calls = 0;
      window.answerUses = () => {
        calls += 1;
        const late = calls === 1;
        return new Promise((resolve) =>
          setTimeout(() => resolve([{ source: late ? 'old.yaml' : 'new.yaml', path: '/x' }]), late ? 600 : 100),
        );
      };
    });
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Shift+F12');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Shift+F12');
    await expect(list(page).locator('.config-editor__uses-file')).toHaveText(['this document', 'new.yaml']);
    await page.waitForTimeout(800);
    await expect(list(page).locator('.config-editor__uses-file')).toHaveText(['this document', 'new.yaml']);
    expect(errors).toEqual([]);
  });

  test('goToDefinition() and findUses() act at the caret', async ({ page }) => {
    const errors = await openEditor(page);
    await caretOn(page, 'main', 1);
    expect(await page.evaluate(() => window.editor.findUses())).toBe(true);
    await expect(rows(page)).toHaveCount(2);
    await page.keyboard.press('Escape');
    await caretOn(page, 'main', 1);
    expect(await page.evaluate(() => window.editor.goToDefinition())).toBe(true);
    await expect
      .poll(() => field(page).evaluate((element) => element.value.slice(element.selectionStart).startsWith('main: {}')))
      .toBe(true);
    // No name at the caret: nothing.
    await caretOn(page, 'clients', 0);
    expect(await page.evaluate(() => [window.editor.findUses(), window.editor.goToDefinition()])).toEqual([false, false]);
    await expect(list(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('the hover card on a name links its definition and uses, by mouse and keyboard', async ({ page }) => {
    const errors = await openEditor(page);
    const card = page.locator('.config-editor__hover');
    const actions = card.locator('.config-editor__hover-action');
    // A use: both links, named in full, each a 24px target at least.
    await field(page).hover({ position: await placeOf(page, 3, 'default_client: ma') });
    await expect(actions).toHaveText(['Definition', 'Uses'], { timeout: 5000 });
    await expect(actions.nth(0)).toHaveAttribute('aria-label', 'Go to definition (F12)');
    await expect(actions.nth(1)).toHaveAttribute('title', 'Find uses (Shift+F12)');
    for (const box of [await actions.nth(0).boundingBox(), await actions.nth(1).boundingBox()]) {
      expect(box.width).toBeGreaterThanOrEqual(24);
      expect(box.height).toBeGreaterThanOrEqual(24);
    }
    await actions.nth(1).click();
    await expect(rows(page)).toHaveCount(2);
    await expect(card).toHaveCount(0);
    await page.keyboard.press('Escape');
    // Definition goes to the key.
    await field(page).hover({ position: await placeOf(page, 3, 'default_client: ma') });
    await actions.nth(0).click();
    await expect
      .poll(() => field(page).evaluate((element) => element.value.slice(element.selectionStart).startsWith('main: {}')))
      .toBe(true);
    // The definition itself: Uses only.
    await page.mouse.move(600, 500);
    await field(page).hover({ position: await placeOf(page, 1, '  ma') });
    await expect(actions).toHaveText(['Uses'], { timeout: 5000 });
    // Not a name: no links.
    await page.mouse.move(600, 500);
    await field(page).hover({ position: await placeOf(page, 5, 'note: ma') });
    await expect(card).toBeVisible({ timeout: 5000 });
    await expect(actions).toHaveCount(0);
    // The keyboard: the card at the caret, focused, Tab to its Uses link.
    await page.mouse.move(600, 500);
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+I');
    await expect(actions).toHaveText(['Definition', 'Uses']);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+I');
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await expect(actions.nth(1)).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(rows(page)).toHaveCount(2);
    expect(errors).toEqual([]);
  });

  // Where the schema names keys, F12 and Shift+F12 are the editor's keys:
  // finding nothing says so at the caret (and to screen readers), and the
  // browser never gets them (Chrome's F12 opens its developer tools).
  test('F12 and Shift+F12 with nothing to find say so at the caret, never reaching the browser', async ({ page }) => {
    const errors = await openEditor(page);
    await page.evaluate(() => {
      window.prevented = [];
      document.addEventListener('keydown', (event) => event.key === 'F12' && window.prevented.push(event.defaultPrevented));
    });
    const notice = page.locator('.config-editor__caret-notice');
    await caretOn(page, 'note', 0, 2);
    await page.keyboard.press('F12');
    await expect(notice).toHaveText("No definition for 'note'");
    await expect(notice).toBeVisible();
    await expect(said(page)).toHaveText(/^No definition for 'note'/);
    await page.keyboard.press('Shift+F12');
    await expect(notice).toHaveText("No uses of 'note'");
    await expect(list(page)).toHaveCount(0);
    // The next key puts it away.
    await page.keyboard.press('ArrowRight');
    await expect(notice).toHaveCount(0);
    // No word at the caret.
    await caretOn(page, 'default_client: ', 0, 'default_client:'.length);
    await page.keyboard.press('F12');
    await expect(notice).toHaveText('No definition here');
    expect(await page.evaluate(() => window.prevented)).toEqual([true, true, true]);
    expect(errors).toEqual([]);
  });

  test('without key names in the schema, F12 is left to the browser', async ({ page }) => {
    const errors = await openEditor(page, {
      schema: JSON.stringify({ type: 'object', properties: { note: { type: 'string' } } }),
    });
    await page.evaluate(() => {
      window.prevented = [];
      document.addEventListener('keydown', (event) => event.key === 'F12' && window.prevented.push(event.defaultPrevented));
    });
    await caretOn(page, 'note', 0, 2);
    await page.keyboard.press('F12');
    await page.keyboard.press('Shift+F12');
    expect(await page.evaluate(() => window.prevented)).toEqual([false, false]);
    await expect(page.locator('.config-editor__caret-notice')).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});

test.describe('find uses on a phone, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test.use({ viewport: { width: 390, height: 740 }, hasTouch: true, isMobile: true });

  test('a tap on a name shows its card; a tap on Uses lists them, a tap on a row goes there', async ({ page }) => {
    const errors = await openEditor(page);
    await field(page).tap({ position: await placeOf(page, 3, 'default_client: ma') });
    const actions = page.locator('.config-editor__hover-action');
    await expect(actions).toHaveText(['Definition', 'Uses'], { timeout: 5000 });
    await actions.nth(1).tap();
    await expect(rows(page)).toHaveCount(2);
    const box = await list(page).boundingBox();
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    await rows(page).nth(1).tap();
    await expect(list(page)).toHaveCount(0);
    await expect.poll(() => selected(page)).toBe('main');
    expect(errors).toEqual([]);
  });
});

// The documents pages' `?keys` setup (js/examples/key-names.js): a server
// file defining `main`, two drop-ins; the host's `onFindUses` answers late
// with a use in `server.d/office.yaml`, a file that is not open.
const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const panel = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"]`);
const input = (page, id) => panel(page, id).locator('.config-editor__input');

async function caretAfter(page, id, after) {
  await tab(page, id).click();
  await input(page, id).click();
  await input(page, id).evaluate((element, text) => {
    const at = element.value.indexOf(text) + text.length;
    element.setSelectionRange(at, at);
  }, after);
}

for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
  test.describe(`find uses across documents, ${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('the open documents are listed by file, the host joins late, a row switches tabs', async ({ page }) => {
      const errors = collectErrors(page);
      await page.goto(`${pathToFileURL(join(js, 'examples', name)).href}?keys`);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await expect(page.locator('[role="tab"]')).toHaveCount(3);
      // An unsaved use in a drop-in.
      await caretAfter(page, 'home', '    client: ');
      await page.keyboard.insertText('main');
      await page.keyboard.press('Escape');
      // On the definition, in the server file.
      await caretAfter(page, 'server', '  ma');
      await page.keyboard.press('Shift+F12');
      const uses = panel(page, 'server').locator('.config-editor__uses');
      await expect(uses.locator('.config-editor__uses-file')).toHaveText(['server.d/home.yaml', 'server.d/office.yaml']);
      await expect(uses.locator('.config-editor__use-text')).toHaveText(['client: main', 'client: main']);
      // The open document's use: its tab, the caret on it.
      await page.keyboard.press('Enter');
      await expect(tab(page, 'home')).toHaveAttribute('aria-selected', 'true');
      await expect
        .poll(() =>
          input(page, 'home').evaluate((element) =>
            element.value.slice(element.selectionStart, element.selectionEnd).includes('main'),
          ),
        )
        .toBe(true);
      // The host's use: the host hears it.
      await caretAfter(page, 'server', '  ma');
      await page.keyboard.press('Shift+F12');
      await expect(uses.locator('[role="option"]')).toHaveCount(2);
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      await expect(page.locator('#log')).toContainText('use server.d/office.yaml /routes/0/client');
      // The host opens that file as a tab, there.
      const office = 'server.d/office.yaml';
      await expect(tab(page, office)).toHaveAttribute('aria-selected', 'true');
      await expect
        .poll(() =>
          input(page, office).evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd)),
        )
        .toContain('main');
      expect(errors).toEqual([]);
    });
  });
}

test.describe('find uses methods on the documents view, documents.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('findUses() and goToDefinition() act at the active editor caret', async ({ page }) => {
    const errors = collectErrors(page);
    await page.goto(`${pathToFileURL(join(js, 'examples', 'documents.html')).href}?keys`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await caretAfter(page, 'home', '    client: ');
    await page.keyboard.insertText('lab');
    await page.keyboard.press('Escape');
    await page.keyboard.press('ArrowLeft');
    expect(await page.evaluate(() => window.documentsView.findUses())).toBe(true);
    await expect(panel(page, 'home').locator('.config-editor__use-text')).toHaveText(['client: lab']);
    await page.keyboard.press('Escape');
    expect(await page.evaluate(() => window.documentsView.goToDefinition())).toBe(true);
    await expect(tab(page, 'lab')).toHaveAttribute('aria-selected', 'true');
    expect(errors).toEqual([]);
  });
});
