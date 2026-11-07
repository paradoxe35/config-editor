import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Twelfth-round findings, replayed with real keys. The classes behind them
// are covered by the unit tests (`save_tests`, `editing_matrix`).

test.describe('toml', () => {
  setupEditor(test, '?language=toml');

  test('redo restores a paste after a refused Edit-as switch waited through the undo', async ({ page }) => {
    const e = editor(page);
    const sample = await e.value();
    await e.paste('[clients."acmé✓"]\nsecret = "s"\n');
    const pasted = await e.value();
    await page.locator('.demo__edit-select').selectOption('xml');
    await page.waitForTimeout(800);
    await e.input.click();
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(sample);
    await page.waitForTimeout(800);
    await page.keyboard.press('Control+Shift+z');
    await expect.poll(() => e.value()).toBe(pasted);
  });
});

test.describe('yaml', () => {
  setupEditor(test, '?language=yaml');

  test('a cut problem list says 20+ and does not count its note', async ({ page }) => {
    const e = editor(page);
    await e.paste(Array.from({ length: 30 }, (_, index) => `unknown_${index}: 1`).join('\n') + '\n');
    await expect(page.locator('.config-editor__status')).toContainText('20+ errors', { timeout: 5000 });
    await expect(page.locator('.config-editor__status')).not.toContainText('warning');
  });
});
