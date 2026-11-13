import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setupEditor } from './helpers.mjs';

// ARIA state and screen-reader announcements (normal-bar round findings):
// what the editor says through its one polite live region
// (`.config-editor__announcement`), and which states its controls expose.

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const js = join(root, 'js');
const said = (page) => page.locator('.config-editor__announcement');
const input = (page) => page.locator('.config-editor__input');
const tip = (page) => page.locator('.config-editor__problem-tip');
const options = (page) => page.locator('.config-editor__completion');
const focusedClass = (page) =>
  page.evaluate(() => {
    const element = document.activeElement;
    return element ? element.className || element.tagName.toLowerCase() : '';
  });

/** Selects `length` characters of the text from `after` characters past `needle`'s start. */
async function selectIn(page, needle, after, length = 0) {
  await input(page).evaluate(
    (element, [text, count, size]) => {
      const at = element.value.indexOf(text) + count;
      element.focus();
      element.setSelectionRange(at, at + size);
    },
    [needle, after, length],
  );
}

test.describe('suggestions', () => {
  setupEditor(test, '?schema=help');

  // D13.a11y-1: Backspace out of a filter that matched nothing refilters.
  test('Backspace from an empty filter brings the list back (D13.a11y-1)', async ({ page }) => {
    await input(page).click();
    await page.keyboard.type('lev');
    await expect(options(page)).toHaveCount(1);
    await page.keyboard.type('zzzz');
    await expect(options(page)).toHaveCount(0);
    for (let step = 0; step < 4; step++) await page.keyboard.press('Backspace');
    await expect(input(page)).toHaveValue('lev');
    await expect(options(page)).toHaveCount(1);
  });

  // D4.a11y-3: the documented single-key shortcuts are exposed.
  test('the text exposes the shortcuts that work in it (D4.a11y-3)', async ({ page }) => {
    const shortcuts = (await input(page).getAttribute('aria-keyshortcuts')) ?? '';
    const list = shortcuts.split(' ');
    for (const key of ['Control+F', 'Control+H', 'Control+G', 'Control+Space', 'Control+S', 'F8', 'Shift+F8', 'Alt+F5']) {
      expect(list, key).toContain(key);
    }
    for (const key of ['Meta+F', 'Meta+G', 'Meta+S']) expect(list, key).toContain(key);
  });

  // D5.a11y-2: a first query with no matches says so (the count already read it).
  test('a find query that matches nothing is announced (D5.a11y-2)', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+f');
    await expect(page.locator('.config-editor__find-count')).toHaveText('No results');
    await page.keyboard.type('zzzz');
    await expect(said(page)).toHaveText(/No results/);
  });

  // D2.a11y-3: the tip closes when the focus moves on to the footer.
  test('the problems tip closes when focus leaves it for the footer (D2.a11y-3)', async ({ page }) => {
    await input(page).click();
    await page.keyboard.type('mode: bogus');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(tip(page)).toBeVisible();
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(tip(page)).toHaveAttribute('role', 'dialog');
    for (let step = 0; step < 4 && !(await focusedClass(page)).includes('diagnostic-status'); step++) {
      await page.keyboard.press('Tab');
    }
    expect(await focusedClass(page)).toContain('diagnostic-status');
    await expect(tip(page)).toBeHidden();
  });
});

test.describe('protected fields', () => {
  setupEditor(test, '?schema=protected');

  // D5.a11y-3: accepting a read-only key from the list says why it stays.
  test('accepting a read-only suggestion announces the reason (D5.a11y-3)', async ({ page }) => {
    await selectIn(page, 'id: srv-42', 2);
    await page.keyboard.press('Control+Space');
    await expect(options(page)).not.toHaveCount(0);
    await expect(options(page).first()).toContainText('Assigned by the server');
    await page.keyboard.press('Enter');
    await expect(input(page)).toHaveValue(/^id: srv-42\n/);
    await expect(said(page)).toHaveText(/Assigned by the server/);
  });

  // D5.a11y-3: a secret placeholder has no suggestions; Ctrl+Space says why.
  test('Ctrl+Space on a secret placeholder announces what it is (D5.a11y-3)', async ({ page }) => {
    await selectIn(page, '«secret 3f2a»', 0, '«secret 3f2a»'.length);
    await page.keyboard.press('Control+Space');
    await expect(options(page)).toHaveCount(0);
    await expect(said(page)).toHaveText(/Keeps the current secret/);
  });
});

test.describe('folds and the change preview', () => {
  setupEditor(test, '?language=yaml&sample=large');

  // D15.a11y-2: the chip says it is closed, as the chevron does.
  test('a fold chip exposes aria-expanded like its chevron (D15.a11y-2)', async ({ page }) => {
    const row = page.locator('.config-editor__gutter-content > div[data-line="7"]');
    await row.hover();
    await page.locator('.config-editor__fold-toggle[data-line="7"]').click();
    const chip = page.locator('.config-editor__fold-chip[data-line="7"]');
    await expect(chip).toHaveCount(1);
    await expect(chip).toHaveAttribute('aria-expanded', 'false');
    await expect(page.locator('.config-editor__fold-toggle[data-line="7"]')).toHaveAttribute('aria-expanded', 'false');
  });

  // D6.a11y-3: the preview sits between the text and the footer in both directions.
  test('Tab and Shift+Tab walk text, preview, footer in order (D6.a11y-3)', async ({ page }) => {
    await input(page).evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, 0);
    });
    await page.keyboard.type('x');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F3');
    const peek = page.locator('.config-editor__diff-peek');
    await expect(peek).toHaveCount(1);
    const inPeek = () => page.evaluate(() => !!document.activeElement?.closest('.config-editor__diff-peek'));
    expect(await inPeek()).toBe(true);
    // Forward: Revert, Close, then the footer's first control.
    await page.keyboard.press('Tab');
    expect(await inPeek()).toBe(true);
    await page.keyboard.press('Tab');
    expect(await inPeek()).toBe(false);
    const first = await focusedClass(page);
    expect(first).toMatch(/config-editor__(diagnostic|change)-status/);
    // Backward from it: the preview's last button, not the text.
    await page.keyboard.press('Shift+Tab');
    expect(await inPeek()).toBe(true);
    // Forward from the footer's last control leaves the editor, not into the preview.
    await page.locator('.config-editor__change-status').focus();
    await page.keyboard.press('Tab');
    expect(await inPeek()).toBe(false);
    expect(await page.evaluate(() => !!document.activeElement?.closest('.config-editor'))).toBe(false);
  });
});

test.describe('documents', () => {
  setupEditor(test, '?documents');

  // D2.a11y-2: the demo says it renamed a tab.
  test('renaming a tab is announced by the demo page (D2.a11y-2)', async ({ page }) => {
    const note = page.locator('.demo__announcement');
    await page.locator('.demo__rename').click();
    await expect(note).toHaveText(/conf\.d\/10-limits\.toml/);
  });

  // D7.a11y-1: a conflict in a document that is not shown is said, and said
  // again when that document is shown.
  test('a baseline conflict in a background document is announced (D7.a11y-1)', async ({ page }) => {
    const note = page.locator('.config-editor-documents__announcement');
    await input(page).first().click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# note');
    await page.locator('[role="tab"]').nth(1).click();
    await expect(note).toHaveText('');
    await page.locator('.demo__external-change').click();
    await expect(note).toContainText('changed outside the editor');
    const tab = page.locator('[role="tab"]').first();
    expect(await tab.getAttribute('aria-label')).toContain('changed outside the editor');
    // Said again on showing it: the region's text changes (a no-break space tells it from the same message).
    await page.evaluate(() => {
      window.__said = 0;
      new MutationObserver(() => window.__said++).observe(document.querySelector('.config-editor-documents__announcement'), {
        childList: true,
        characterData: true,
        subtree: true,
      });
    });
    await tab.click();
    await expect.poll(() => page.evaluate(() => window.__said)).toBeGreaterThan(0);
    await expect(note).toContainText('changed outside the editor');
  });
});

test.describe('the footer', () => {
  setupEditor(test, '?language=yaml');

  // D9.a11y-2: the problems count is a live region, the change count is not
  // (Alt+F5 announces each change), and the README says exactly that.
  test('the README names the live footer count as the problems (D9.a11y-2)', async ({ page }) => {
    await expect(page.locator('.config-editor__diagnostic-status')).toHaveAttribute('aria-live', 'polite');
    await page.locator('.config-editor__input').click();
    await page.keyboard.type('x');
    await expect(page.locator('.config-editor__change-status')).toHaveCount(1);
    expect(await page.locator('.config-editor__change-status').evaluate((count) => !!count.closest('[aria-live], [role="status"]'))).toBe(
      false,
    );
    const readme = readFileSync(join(root, 'crates', 'config-editor-leptos', 'README.md'), 'utf8');
    expect(readme).not.toMatch(/footer's counts\s+`aria-live`/);
    expect(readme).toMatch(/footer's problem count/);
  });
});

test.describe('the handle', () => {
  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
  test.skip(!built, 'run `npm run build` in js/ first');
  const page = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  // D11.a11y-1: foldAll() and unfoldAll() say what they did, as the keys do.
  test('foldAll() and unfoldAll() announce like the keyboard commands (D11.a11y-1)', async ({ page: browser }) => {
    await browser.goto(page());
    await browser.locator('.config-editor__input').waitFor();
    const value = 'clients:\n  web:\n    a: 1\n    b: 2\n  api:\n    a: 1\n    b: 2\nserver:\n  port: 1\n  host: x\n';
    await browser.evaluate((text) => window.editor.update({ value: text }), value);
    await expect(said(browser)).toHaveText('');
    await browser.evaluate(() => window.editor.foldAll());
    await expect(said(browser)).toHaveText(/Folded \d+ ranges/);
    await browser.evaluate(() => window.editor.unfoldAll());
    await expect(said(browser)).toHaveText(/Unfolded \d+ ranges/);
  });
});
