import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// At 400% (a 320x225 CSS px window) what the keyboard moves to, or what a
// press changes under it, stays reachable: focus the editor carries to a
// control is scrolled into view (`D6.a11y-2`), the find row that wraps under
// the pressed button keeps it in view (`D21.a11y-2`).

test.use({ viewport: { width: 320, height: 225 } });

/** Whether the focused element is wholly inside the window. */
function focusInWindow(page) {
  return page.evaluate(() => {
    const box = document.activeElement.getBoundingClientRect();
    return {
      name: document.activeElement.className || document.activeElement.tagName,
      top: box.top,
      bottom: box.bottom,
      height: innerHeight,
    };
  });
}

function expectShown({ name, top, bottom, height }) {
  expect(top, `${name} top`).toBeGreaterThanOrEqual(0);
  expect(bottom, `${name} bottom`).toBeLessThanOrEqual(height);
}

test.describe('focus the editor moves stays in the window', () => {
  setupEditor(test, '?sample=large');

  test('Tab out of the change preview (D6.a11y-2)', async ({ page }) => {
    await editor(page).input.click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('x: 1\n');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('Alt+F3');
    await expect(page.locator('.config-editor__diff-peek-revert')).toBeFocused();
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await expect(page.locator('.config-editor__diff-peek button:focus')).toHaveCount(0);
    expectShown(await focusInWindow(page));
  });

  test('Ctrl+F in a page scrolled past the editor top (D6.a11y-2)', async ({ page }) => {
    await editor(page).input.click();
    await page.keyboard.press('Control+Home');
    await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
    await page.keyboard.press('Control+f');
    await expect(page.getByRole('textbox', { name: 'Find', exact: true })).toBeFocused();
    expectShown(await focusInWindow(page));
  });

  test('Replace all that wraps the find row (D21.a11y-2)', async ({ page }) => {
    await editor(page).input.click();
    await page.keyboard.press('Control+Home');
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.keyboard.press('Control+h');
    await page.keyboard.type('listen');
    await page.getByRole('textbox', { name: 'Replace', exact: true }).focus();
    await page.keyboard.type('port');
    await page.keyboard.press('Tab');
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: /Replace all/ })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.locator('.config-editor__find-count')).toContainText('Replaced');
    await page.waitForTimeout(150);
    expectShown(await focusInWindow(page));
  });
});
