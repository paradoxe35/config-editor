import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Gaps earlier rounds (16-18) left: each replayed in the demo with real
// keys, failing before its fix.

for (const query of ['?language=yaml', '?documents']) {
  test.describe(`the demo page ${query}`, () => {
    setupEditor(test, query);

    test('takes the theme variables without being styled as an editor', async ({ page }) => {
      const wrapper = page.locator('.demo').first();
      await expect(wrapper).not.toHaveClass(/(^|\s)config-editor(\s|$)/);
      await expect(wrapper).toHaveClass(/config-editor-theme/);
      const variable = (locator) =>
        locator.evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
      const editorRoot = page.locator('.config-editor:has(.config-editor__input)').first();
      expect(await variable(wrapper)).toBe(await variable(editorRoot));
      await page.locator('.demo__theme-select').selectOption('nord');
      await expect(wrapper).toHaveAttribute('data-color-theme', 'nord');
      await expect.poll(() => variable(wrapper)).toBe('#2e3440');
      expect(await variable(editorRoot)).toBe('#2e3440');
    });
  });
}

test.describe('an Edit-as switch that waits', () => {
  setupEditor(test, '?language=xml');

  test('the picker shows the language on screen and the name says it waits', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <mixed>text <b>bold</b> tail</mixed>\n</r>\n');
    await page.locator('.demo__edit-select').selectOption('yaml');
    const marker = page.locator('.config-editor__diagnostic-marker[data-message*="Cannot switch to YAML yet"]');
    await expect(marker).toHaveCount(1);
    expect(await e.value()).toContain('<mixed>');
    await expect(page.locator('.demo__edit-select')).toHaveValue('');
    await expect(page.locator('.config-editor__file')).toContainText('waiting to edit as YAML');
    await expect(page.locator('.config-editor__file')).not.toContainText('editing as');
    // Once the document converts, the switch completes and both follow.
    await e.paste('<r>\n  <mixed>text</mixed>\n</r>\n');
    await expect.poll(() => e.value()).toContain('mixed: text');
    await expect(page.locator('.demo__edit-select')).toHaveValue('yaml');
    await expect(page.locator('.config-editor__file')).toContainText('editing as YAML');
  });
});

test.describe('the page sample', () => {
  test('is named as what it is in the Sample picker', async ({ page, baseURL }) => {
    await page.addInitScript(() => {
      window.configEditorSample = 'a: 1\nb: 2\nc: 3\n';
    });
    await page.goto(`${baseURL}?language=yaml&sample=page`);
    await page.locator('.config-editor__input').waitFor();
    const picker = page.locator('.demo__sample-select');
    await expect(picker.locator('option:checked')).toHaveText(/Page sample \(3 lines\)/);
  });
});
