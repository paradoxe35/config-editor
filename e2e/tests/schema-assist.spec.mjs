import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Schema oneOf and const in suggestions and hover.

test.describe('oneOf options in suggestions and hover', () => {
  setupEditor(test, '?schema=help');

  test('a key of several options names each, a const key its values', async ({ page }) => {
    const e = editor(page);
    await e.paste('dns:\n  \n');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Control+Space');
    const provider = e.options.filter({ has: page.locator('.config-editor__completion-key', { hasText: /^provider$/ }) });
    await expect(provider.locator('.config-editor__completion-type')).toHaveText('"manual" | "static"');
    await expect(provider.locator('.config-editor__completion-detail')).toHaveText('Manual, Static file');
    await page.keyboard.press('Escape');

    await e.paste('dns:\n  provider: static\n  path: /x\n');
    const at = await pointAt(page, 1, 4);
    await page.mouse.move(at.x, at.y, { steps: 4 });
    const card = page.locator('.config-editor__hover');
    await expect(card.locator('.config-editor__hover-key')).toHaveText('provider');
    await expect(card.locator('.config-editor__hover-type')).toHaveText('"static"');
    await expect(card.locator('.config-editor__hover-choice-title')).toHaveText('Static file');
  });
});
