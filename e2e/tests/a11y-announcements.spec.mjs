import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// What the editor says through its live region: refused saves and edits, F8 and Alt+F5 with nothing to
// walk, reverts, reloads, change and fold commands, and the problem summary.

const panel = (page, id) =>
  page.locator(`[role="tabpanel"][data-document-id="${id}"]`);

const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];

test.describe("a refused save is announced (D2.a11y-1)", () => {
  setupEditor(test, "?language=yaml");

  test("Ctrl+S on text the host language cannot hold says why", async ({
    page,
  }) => {
    const e = editor(page);
    await page.locator(".demo__edit-select").selectOption("json");
    await expect.poll(() => e.value()).toContain("{");
    await e.paste("{");
    const announcement = page.locator(".config-editor__announcement");
    await expect(announcement).toHaveText("");
    await page.keyboard.press("Control+s");
    await expect(announcement).not.toHaveText("");
    expect(await announcement.textContent()).toMatch(/\S/);
    // Nothing was sent.
    await expect(page.locator(".demo__output-text")).not.toContainText("{");
  });
});

test.describe("a refused save in the documents view is announced (D2.a11y-1)", () => {
  setupEditor(test, "?documents");

  test("Ctrl+S in an unconvertible Edit-as view says why", async ({ page }) => {
    await page.locator(".demo__edit-as").click();
    const input = panel(page, "main").locator(".config-editor__input");
    await expect(input).toHaveValue(/^\{/);
    await input.click();
    await page.keyboard.press("Control+a");
    await page.keyboard.type("{");
    const announcement = panel(page, "main").locator(
      ".config-editor__announcement",
    );
    await page.waitForTimeout(700);
    await expect(announcement).toHaveText("");
    await page.keyboard.press("Control+s");
    await expect(announcement).not.toHaveText("");
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const announcement = (page) => page.locator('.config-editor__announcement').first();

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('L25-A4: change and fold commands are announced', () => {
    test('Alt+F5 and Shift+Alt+F5 say which change, where', async ({ page }) => {
      const value = 'name: a\nport: 2\nmode: fast\nextra: 1\n';
      const errors = await open(page, { value, baseline: 'name: a\nport: 1\nmode: fast\n' });
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Alt+F5');
      await expect(announcement(page)).toHaveText(/Modified line, line 2.*1 of 2/);
      await page.keyboard.press('Alt+F5');
      await expect(announcement(page)).toHaveText(/Added line, line 4.*2 of 2/);
      await page.keyboard.press('Shift+Alt+F5');
      await expect(announcement(page)).toHaveText(/Modified line, line 2.*1 of 2/);
      expect(errors).toEqual([]);
    });

    test('folding and unfolding say which lines', async ({ page }) => {
      const value = 'a: 1\nb:\n  c: 2\n  d: 3\ne:\n  f: 4\n';
      const errors = await open(page, { value });
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Control+Shift+BracketLeft');
      await expect(announcement(page)).toHaveText(/Folded lines 2–4/);
      await page.keyboard.press('Control+Shift+BracketRight');
      await expect(announcement(page)).toHaveText(/Unfolded lines 2–4/);
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+0');
      await expect(announcement(page)).toHaveText(/Folded 2 ranges/);
      await page.keyboard.press('Control+k');
      await page.keyboard.press('Control+j');
      await expect(announcement(page)).toHaveText(/Unfolded 2 ranges/);
      // A gutter chevron too.
      await page.locator('.config-editor__fold-toggle').first().click();
      await expect(announcement(page)).toHaveText(/Folded lines 2–4/);
      expect(errors).toEqual([]);
    });
  });
});

// QA round 27, accessibility: why an edit was refused is said through the
// editor's live region (one there from the start), not a region made with
// its text; Escape closes the hover card or the problem tip with the
// pointer on the text it shows for and the focus nowhere (WCAG 1.4.13).
// L27-A1: a refused edit on a read-only field showed its message in a
// `role="status"` region made together with it, and made again for each
// refused key: screen readers may never say it.
for (const language of LANGUAGES) {
  test.describe(`L27-A1 ${language}`, () => {
    setupEditor(test, `?schema=protected&language=${language}`);

    test('a refused edit is said through the live region', async ({ page }) => {
      const e = editor(page);
      const before = await e.value();
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('srv-42') + 2;
        element.focus();
        element.setSelectionRange(at, at);
      });
      const said = page.locator('.config-editor__announcement');
      const tip = page.locator('.config-editor__protect-tip--refused');
      for (const key of ['x', 'y', 'Backspace']) {
        await page.keyboard.press(key);
        await expect(tip).toBeVisible();
        const message = (await tip.textContent()).trim();
        expect(message.length).toBeGreaterThan(0);
        await expect.poll(async () => (await said.textContent()).replace(/ /g, '').trim()).toBe(message);
        // The tip itself is no live region (it would be said twice, or not
        // at all).
        await expect(tip).not.toHaveAttribute('role', 'status');
        await expect(tip).not.toHaveAttribute('aria-live', /.+/);
      }
      expect(await e.value()).toBe(before);
    });
  });
}

// A group of its own (needs the built npm packages).
test.describe(() => {
  // QA round 30, accessibility: results that were silent are said through the
  // live region (a revert, F8 with no problems, Alt+F5 / Alt+F3 with no
  // changes, the notice's Reload), and `aria-keyshortcuts` names F8 only while
  // the problems are on. Through the npm package's vanilla page (run
  // `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const said = (page) => page.locator('.config-editor__announcement').first();

  async function open(page, options) {
    await page.goto(pageUrl(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    await input(page).click();
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test('F8 with no problems says "No problems"; Shift+F8 too', async ({ page }) => {
    await open(page);
    await page.keyboard.press('F8');
    await expect(said(page)).toHaveText('No problems');
    await page.keyboard.press('Shift+F8');
    await expect(said(page)).toHaveText(/^No problems ?$/);
  });

  test('Alt+F5 and Alt+F3 with no changes say "No changes"', async ({ page }) => {
    await open(page);
    for (const key of ['Alt+F5', 'Alt+F3', 'Shift+Alt+F5']) {
      await page.keyboard.press(key);
      await expect(said(page), key).toHaveText(/^No changes ?$/);
    }
  });

  test('reverting a change says "Reverted"', async ({ page }) => {
    await open(page);
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# note\n');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F3');
    await expect(page.locator('.config-editor__diff-peek')).toHaveCount(1);
    await page.keyboard.press('r');
    await expect(page.locator('.config-editor__diff-peek')).toHaveCount(0);
    await expect(said(page)).toHaveText(/^Reverted ?$/);
  });

  test('Reload in the changed-baseline notice says "Reloaded"', async ({ page }) => {
    await open(page);
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# mine\n');
    await page.locator('#external-change').click();
    await expect(page.locator('.config-editor__notice')).toBeVisible();
    await page.locator('.config-editor__notice-reload').click();
    await expect(said(page)).toHaveText(/^Reloaded ?$/);
  });
});

test.describe('the problem summary live region (A35-4)', () => {
  setupEditor(test, '?language=json');

  test('is not written again while typing leaves its text as it is', async ({ page }) => {
    const input = page.locator('.config-editor__input');
    await input.click();
    // The sample is YAML, so as JSON it is broken: errors stay while typing.
    await page.keyboard.press('Control+End');
    await page.keyboard.type('a');
    await expect(page.locator('.config-editor__diagnostic-status')).toContainText('error');
    await page.waitForTimeout(500);
    await page.locator('.config-editor__diagnostic-status').evaluate((status) => {
      window.__writes = [];
      let last = status.textContent;
      new MutationObserver(() => {
        window.__writes.push([last, status.textContent]);
        last = status.textContent;
      }).observe(status, { subtree: true, childList: true, characterData: true });
    });
    await page.keyboard.type('abcdefghij', { delay: 30 });
    await page.waitForTimeout(500);
    const writes = await page.evaluate(() => window.__writes);
    expect(writes, 'no write while the summary says the same').toEqual([]);
  });
});
