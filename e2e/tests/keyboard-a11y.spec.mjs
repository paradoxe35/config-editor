import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Keyboard and screen-reader access: leaving the editor with the keyboard
// (Ctrl+M, Escape then Tab), the caret's note (`aria-describedby`) and the
// announcements of the live region.

const announcement = (page) => page.locator('.config-editor__announcement');
const tabStatus = (page) => page.locator('.config-editor__tab-focus-status');
/** What has the focus: the editor's textarea, or another element's class or tag. */
const focused = (page) =>
  page.evaluate(() => {
    const element = document.activeElement;
    if (!element || element === document.body) return 'body';
    if (element.classList.contains('config-editor__input')) return 'textarea';
    return element.className || element.tagName.toLowerCase();
  });

/** Focuses the textarea with the caret at the start. */
async function focusText(page) {
  const e = editor(page);
  await e.input.click();
  await page.keyboard.press('Control+Home');
}

test.describe('leaving the editor with the keyboard', () => {
  setupEditor(test, '?language=yaml');

  test('Ctrl+M: Tab and Shift+Tab move the focus, the footer says so; Ctrl+M again indents', async ({ page }) => {
    const e = editor(page);
    await focusText(page);
    const before = await e.value();
    await expect(tabStatus(page)).toHaveCount(0);

    await page.keyboard.press('Control+m');
    await expect(tabStatus(page)).toHaveText('Tab moves focus');
    await expect(announcement(page)).toContainText('Tab moves focus');
    await page.keyboard.press('Tab');
    expect(await focused(page)).not.toBe('textarea');
    expect(await e.value()).toBe(before);

    await e.input.focus();
    await page.keyboard.press('Shift+Tab');
    expect(await focused(page)).not.toBe('textarea');
    expect(await e.value()).toBe(before);

    // Off again: Tab indents and the focus stays.
    await e.input.focus();
    await page.keyboard.press('Control+m');
    await expect(tabStatus(page)).toHaveCount(0);
    await expect(announcement(page)).toContainText('Tab indents');
    await page.keyboard.press('Tab');
    expect(await focused(page)).toBe('textarea');
    expect(await e.value()).not.toBe(before);
  });

  test('the footer indicator turns the mode off', async ({ page }) => {
    await focusText(page);
    await page.keyboard.press('Control+m');
    await tabStatus(page).click();
    await expect(tabStatus(page)).toHaveCount(0);
    const e = editor(page);
    await e.input.focus();
    const before = await e.value();
    await page.keyboard.press('Tab');
    expect(await focused(page)).toBe('textarea');
    expect(await e.value()).not.toBe(before);
  });

  test('Escape then Tab leaves once; the next Tab in the text indents again', async ({ page }) => {
    const e = editor(page);
    await focusText(page);
    const before = await e.value();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Tab');
    expect(await focused(page)).not.toBe('textarea');
    expect(await e.value()).toBe(before);

    // Shift+Tab after Escape leaves backwards.
    await e.input.focus();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Shift+Tab');
    expect(await focused(page)).not.toBe('textarea');

    // Only right after the Escape: a key between, and Tab indents.
    await e.input.focus();
    await page.keyboard.press('Escape');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Tab');
    expect(await focused(page)).toBe('textarea');
    expect(await e.value()).not.toBe(before);
  });

  test('an Escape that closes the suggestions does not leave; the next one arms Tab', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Escape');
    await expect.poll(() => e.isOpen()).toBe(false);
    await page.keyboard.press('Tab');
    expect(await focused(page)).toBe('textarea');

    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Tab');
    expect(await focused(page)).not.toBe('textarea');
  });

  test('in the Tab mode, Tab still accepts a suggestion while the list is open', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.press('Control+m');
    await page.keyboard.type('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Tab');
    expect(await focused(page)).toBe('textarea');
    expect(await e.value()).toContain('listen');
  });

  test('Escape closes the find widget first; Escape again then Tab leaves', async ({ page }) => {
    const e = editor(page);
    await focusText(page);
    await page.keyboard.press('Control+f');
    await expect(page.getByRole('textbox', { name: 'Find' })).toBeFocused();
    await page.keyboard.press('Escape');
    expect(await focused(page)).toBe('textarea');
    const before = await e.value();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Tab');
    expect(await focused(page)).not.toBe('textarea');
    expect(await e.value()).toBe(before);
  });

  test('read-only: Tab already moves the focus', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}?language=yaml&readonly`);
    await focusText(page);
    await page.keyboard.press('Tab');
    expect(await focused(page)).not.toBe('textarea');
  });
});

test.describe('the problem at the caret', () => {
  setupEditor(test, '?language=yaml');

  test('is the textarea’s description, follows the caret, and F8 announces "Error n of m"', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: true\npublic_address: tunnel.example.com\ninsecure_plaintext: 3\n');
    await expect.poll(() => e.diagnosticCount()).toBe(2);
    const messages = await page
      .locator('.config-editor__diagnostic-marker')
      .evaluateAll((markers) => markers.map((marker) => marker.dataset.message));

    await e.input.focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(announcement(page)).toHaveText(`Error 1 of 2: ${messages[0]}`);
    await expect(e.input).toHaveAccessibleDescription(`Error: ${messages[0]}`);
    await expect(announcement(page)).toMatchAriaSnapshot(`- status: ${JSON.stringify(`Error 1 of 2: ${messages[0]}`)}`);

    await page.keyboard.press('F8');
    await expect(announcement(page)).toHaveText(`Error 2 of 2: ${messages[1]}`);
    await expect(e.input).toHaveAccessibleDescription(`Error: ${messages[1]}`);

    // On a line without a problem the description is empty.
    await page.keyboard.press('ArrowUp');
    await expect(e.input).toHaveAccessibleDescription('');
    // Back onto the first problem with the arrows.
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    await expect(e.input).toHaveAccessibleDescription(`Error: ${messages[0]}`);
  });

  test('F8 again on the only problem announces it again', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: true\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
    await e.input.focus();
    await page.keyboard.press('F8');
    const first = await announcement(page).textContent();
    expect(first).toMatch(/^Error 1 of 1: /);
    await page.keyboard.press('F8');
    await expect.poll(() => announcement(page).textContent()).not.toBe(first);
    expect((await announcement(page).textContent()).trim()).toBe(first.trim());
  });
});

test.describe('protected text at the caret', () => {
  setupEditor(test, '?language=yaml&schema=protected');

  /** Clicks into `text` on its line. */
  async function clickInto(page, text, offset = 1) {
    const lines = (await editor(page).value()).split('\n');
    const line = lines.findIndex((candidate) => candidate.includes(text));
    const at = await pointAt(page, line, lines[line].indexOf(text) + offset);
    await page.mouse.click(at.x, at.y);
  }

  test('a read-only value gives its reason, a secret placeholder its hint', async ({ page }) => {
    const e = editor(page);
    await clickInto(page, 'srv-42', 2);
    await expect(e.input).toHaveAccessibleDescription('Read-only: Assigned by the server');
    await clickInto(page, 'edge', 2);
    await expect(e.input).toHaveAccessibleDescription('');
    await clickInto(page, '«secret 3f2a»', 0);
    await expect(e.input).toHaveAccessibleDescription(/^Secret placeholder: Keeps the current secret/);
  });
});

test.describe('leaving an editor among several documents', () => {
  setupEditor(test, '?documents');

  test('Tab out of the editor leaves the document; Shift+Tab reaches the tab strip side', async ({ page }) => {
    const panel = page.locator('[role="tabpanel"]:not(.config-editor-documents__panel-hidden)');
    const input = panel.locator('.config-editor__input');
    await input.click();
    await page.keyboard.press('Control+m');
    await page.keyboard.press('Tab');
    // Past the editor's text: its footer, or what follows the documents.
    expect(await focused(page)).not.toBe('textarea');
    const inside = await page.evaluate(() =>
      Boolean(document.activeElement?.closest('.config-editor__input-wrap')),
    );
    expect(inside).toBe(false);

    await input.focus();
    await page.keyboard.press('Shift+Tab');
    const before = await page.evaluate(() => {
      const element = document.activeElement;
      return {
        role: element?.getAttribute('role') ?? null,
        inEditor: Boolean(element?.closest('.config-editor__input-wrap')),
      };
    });
    expect(before.inEditor).toBe(false);
    // The other documents' editors are inert: the focus never lands in one.
    const inHidden = await page.evaluate(() =>
      Boolean(document.activeElement?.closest('.config-editor-documents__panel-hidden')),
    );
    expect(inHidden).toBe(false);
  });
});

// Host problems through the npm package's example page (built with
// `npm run build` in js/), which shows one on `listen`.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

test.describe('a host problem at the caret', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('is described with its source', async ({ page }) => {
    await page.goto(pathToFileURL(join(js, 'examples', 'vanilla.html')).href);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.evaluate(() =>
      window.editor.setHostDiagnostics([
        { path: '/listen', severity: 'warning', message: 'port in use', source: 'burrow' },
      ]),
    );
    const input = page.locator('.config-editor__input');
    await expect(page.locator('.config-editor__diagnostic-marker-host')).toHaveCount(1);
    await input.focus();
    await page.keyboard.press('F8');
    await expect(input).toHaveAccessibleDescription('Warning from burrow: port in use');
    await expect(announcement(page)).toHaveText('Warning 1 of 1 from burrow: port in use');
  });
});
