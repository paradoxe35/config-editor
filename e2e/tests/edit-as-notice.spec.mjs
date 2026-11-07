import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The notice while editing as another language, on the demo: it says the
// host file's comments are not shown (and are kept), and that comments
// typed in the edited language are not saved; it is a polite status, a
// click dismisses it, and a new reason brings it back. The demo's "Saved
// value" panel shows what the host receives.

const notice = (page) => page.locator('.config-editor__edit-as-notice');
const message = (page) => notice(page).locator('.config-editor__edit-as-notice-message');
const saved = (page) => page.locator('.demo__output-text').textContent();

test.describe('a YAML file with a comment, edited as TOML', () => {
  setupEditor(test, '?edit=toml');

  test('says the comments are not shown here, and a click dismisses it', async ({ page }) => {
    await expect(notice(page)).toBeVisible();
    await expect(notice(page)).toHaveAttribute('role', 'status');
    await expect(message(page)).toHaveText(
      "The YAML file's comments and formatting are not shown while editing as TOML; saving keeps them.",
    );
    // The comment is not in the TOML on screen.
    expect(await editor(page).value()).not.toContain('Burrow server configuration');
    const dismiss = notice(page).getByRole('button', { name: 'Dismiss' });
    await dismiss.click();
    await expect(notice(page)).toBeHidden();
  });

  test('a comment typed in TOML is not saved, and the notice says so again', async ({ page }) => {
    await notice(page).getByRole('button', { name: 'Dismiss' }).click();
    await expect(notice(page)).toBeHidden();
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# typed in TOML\n');
    // A value edit too, so a save happens.
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('6');
    await expect(notice(page)).toBeVisible();
    await expect(message(page)).toHaveText(
      'Comments typed in TOML are not saved: the YAML file keeps only its own comments.',
    );
    await expect.poll(() => saved(page)).toContain('listen: 7836');
    const host = await saved(page);
    expect(host).toContain('# Burrow server configuration');
    expect(host).not.toContain('typed in TOML');
  });

  test('the keyboard reaches the dismiss button', async ({ page }) => {
    const dismiss = notice(page).getByRole('button', { name: 'Dismiss' });
    await dismiss.focus();
    await page.keyboard.press('Enter');
    await expect(notice(page)).toBeHidden();
  });
});

test.describe('a JSON file edited as YAML', () => {
  setupEditor(test, '?language=json&edit=yaml');

  test('shows nothing until a comment is typed, then says JSON has none', async ({ page }) => {
    await expect(notice(page)).toBeHidden();
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# a note\n');
    await expect(message(page)).toHaveText('Comments typed in YAML are not saved: JSON has no comments.');
    // Deleting the comment takes the notice away.
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('End');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Delete');
    await expect(notice(page)).toBeHidden();
  });
});

test.describe('editing in the host language', () => {
  setupEditor(test, '?language=yaml');

  test('shows no notice, whatever the comments', async ({ page }) => {
    await page.waitForTimeout(400);
    await expect(notice(page)).toBeHidden();
  });
});
