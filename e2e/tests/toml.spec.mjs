import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// TOML coverage for `?language=toml` mode: key/value completion, table
// headers, duplicate keys, `#` comments, hover, and diagnostics. Shared
// behaviors (diff gutter, viewport, themes) are covered by the YAML/JSON
// suites; TOML-only mechanics (tables, `=`, dotted keys) live here.
setupEditor(test, '?language=toml');

test.describe('TOML documents', () => {
  test('a key typed at the root is suggested, and Enter accepts it', async ({ page }) => {
    const e = editor(page);
    await e.paste('');
    await e.input.click();
    await page.keyboard.type('lis');
    await expect.poll(() => e.keys()).toContain('listen');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen = ');
  });

  test('a boolean value is suggested after its equals sign', async ({ page }) => {
    const e = editor(page);
    await e.paste('insecure_plaintext = ');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toEqual(['true']);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('insecure_plaintext = true');
  });

  test('a repeated key is reported as an error', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 1\nlisten = 2\n');
    await expect.poll(() => e.diagnosticCount(), 'duplicate key is an error').toBe(1);
  });

  test('Ctrl+/ toggles # comments', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 7835');
    await e.input.click();
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('# listen = 7835');
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('listen = 7835');
  });

  test('table members are suggested under their header', async ({ page }) => {
    const e = editor(page);
    await e.paste('[pool]\n');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('min');
    await expect.poll(() => e.keys()).toContain('min_idle');
  });

  test('accepting an object key expands to a table header', async ({ page }) => {
    const e = editor(page);
    await e.paste('');
    await e.input.click();
    await page.keyboard.type('poo');
    await expect.poll(() => e.keys()).toContain('pool');
    await e.clickKey('pool');
    await expect(e.input).toHaveValue('[pool]\n');
  });

  test('a schema violation is reported on its value', async ({ page }) => {
    const e = editor(page);
    await e.paste('bogus_key = 1\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });
});
