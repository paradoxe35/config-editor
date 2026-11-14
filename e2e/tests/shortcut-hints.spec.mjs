import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// Shortcut hints and where they are true: the footer's suggestions hint on a
// touch screen, `Cmd` named where macOS takes it (and not where it does not),
// the conflict notice saying where F6 works, and the find widget and the diff peek in a
// window a few lines tall.

const MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36';

const footerHint = (page) => page.locator('.config-editor__hint');
const input = (page) => page.locator('.config-editor__input');

test.describe('the suggestions hint on a touch screen (D23.ux-1)', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 740 } });
  setupEditor(test, '?language=yaml');

  test('names no key, and says suggestions come as you type', async ({ page }) => {
    await expect(footerHint(page)).toHaveText('suggestions appear as you type');
    await expect(footerHint(page).locator('kbd')).toHaveCount(0);
  });
});

test.describe('shortcut hints on macOS (D23.ux-2)', () => {
  test.use({ userAgent: MAC });
  setupEditor(test, '?language=yaml');

  test('name Cmd where Cmd works, Ctrl where it does not, and the keys agree', async ({ page }) => {
    // Ctrl+Space is the editor's with Control only: macOS keeps Cmd+Space.
    await expect(footerHint(page).locator('kbd')).toHaveText(['Ctrl', 'Space']);
    const shortcuts = await input(page).getAttribute('aria-keyshortcuts');
    expect(shortcuts).toContain('Meta+H');
    expect(shortcuts).toContain('Control+Space');
    expect(shortcuts).not.toContain('Meta+Space');
    expect(shortcuts).not.toContain('Meta+M');

    // The hint's key opens what it names.
    await input(page).click();
    await page.keyboard.press('Meta+h');
    const widget = page.locator('.config-editor__find');
    await expect(widget).toBeVisible();
    await expect(widget.locator('.config-editor__replace-input')).toBeVisible();
    await expect(widget.locator('.config-editor__find-expand')).toHaveAttribute('title', 'Toggle replace (Cmd+H)');
    await expect(widget.locator('.config-editor__find-text-button').last()).toHaveAttribute('title', 'Replace all (Cmd+Alt+Enter)');
  });
});

test.describe('shortcut hints elsewhere (D23.ux-2)', () => {
  setupEditor(test, '?language=yaml');

  test('name Ctrl', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    await expect(page.locator('.config-editor__find-expand')).toHaveAttribute('title', 'Toggle replace (Ctrl+H)');
    await expect(page.locator('.config-editor__find-text-button').last()).toHaveAttribute('title', 'Replace all (Ctrl+Alt+Enter)');
  });
});

test.describe('F6 and the conflict notice (D23.a11y-2)', () => {
  setupEditor(test, '?language=yaml');

  // The sentence says where F6 works: in the editor. The host's page keeps
  // its own F6 (a browser moves between its address bar and the page with it).
  test('says F6 works in the editor, where it reaches the choices and back, and leaves the host page its F6', async ({ page }) => {
    const focused = () => page.evaluate(() => document.activeElement?.className ?? '');
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('x');
    const external = page.locator('.demo__external-change');
    await external.evaluate((button) => button.focus());
    await external.click();
    const notice = page.locator('.config-editor__notice');
    await expect(notice).toBeVisible();
    await expect(notice).toContainText('In the editor, F6 moves to the choices and back.');
    await external.evaluate((button) => button.focus());
    await page.keyboard.press('F6');
    expect(await focused()).toContain('demo__external-change');
    await input(page).focus();
    await page.keyboard.press('F6');
    await expect.poll(focused).toContain('config-editor__notice-keep');
    await page.keyboard.press('F6');
    await expect.poll(focused).toContain('config-editor__input');
  });
});

test.describe('a window a few lines tall (D23.ux-5)', () => {
  test.use({ viewport: { width: 200, height: 150 } });
  setupEditor(test, '?language=yaml');

  test('keeps the find widget reachable and its field usable', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    const widget = page.locator('.config-editor__find');
    await expect(widget.locator('.config-editor__replace-input')).toBeVisible();
    const box = await widget.locator('.config-editor__find-input:not(.config-editor__replace-input)').boundingBox();
    expect(box.width, 'the find field is wide enough to type in').toBeGreaterThanOrEqual(60);
    // A match, so the replace buttons are not marked unavailable.
    await page.keyboard.type(':');
    await widget.locator('.config-editor__replace-input').click({ trial: true });
    await widget.locator('.config-editor__find-text-button').last().click({ trial: true });
  });

  test('keeps the diff peek reachable', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('x');
    await page.keyboard.press('Alt+F3');
    const peek = page.locator('.config-editor__diff-peek');
    await expect(peek).toBeVisible();
    await page.keyboard.press('Control+h');
    await expect(page.locator('.config-editor__find .config-editor__replace-input')).toBeVisible();
    await peek.locator('.config-editor__diff-peek-close').click({ trial: true });
    await peek.locator('.config-editor__diff-peek-revert').click({ trial: true });
  });
});
