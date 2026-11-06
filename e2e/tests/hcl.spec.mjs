import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// HCL coverage for `?language=hcl` mode: attribute completion, duplicate
// attributes, `#` comments, and schema diagnostics. Shared behaviors are covered
// by the YAML/JSON suites; HCL-only mechanics live here.
setupEditor(test, '?language=hcl');

test.describe('HCL documents', () => {
  test('a key typed at the root is suggested, and Enter accepts it', async ({ page }) => {
    const e = editor(page);
    await e.paste('');
    await e.input.click();
    await page.keyboard.type('lis');
    await expect.poll(() => e.keys()).toContain('listen');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen = ');
  });

  test('a repeated attribute is reported as an error', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 1\nlisten = 2\n');
    await expect.poll(() => e.diagnosticCount(), 'duplicate attribute is an error').toBe(1);
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

  test('a schema violation is reported on its attribute', async ({ page }) => {
    const e = editor(page);
    await e.paste('bogus_key = 1\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('keys inside a nested value map are suggested', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients = {\n  acme = {\n    \n  }\n}\n');
    await e.input.evaluate((element) => {
      // The blank line inside the `acme` map, after its four-space indent.
      const at = element.value.indexOf('\n    \n') + 5;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('se');
    await expect.poll(() => e.keys()).toContain('secret');
  });

  test('accepting a key inside a nested value map inserts it at its depth', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients = {\n  acme = {\n    \n  }\n}\n');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('\n    \n') + 5;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('ma');
    await expect.poll(() => e.keys()).toContain('max_connections');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('clients = {\n  acme = {\n    max_connections = \n  }\n}\n');
  });

  test('an error inside a nested value map is reported on its own line', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients = {\n  acme = {\n    secret = 1\n  }\n}\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('the highlight overlay renders every character of the value', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 7835\ninsecure_plaintext = false\n');
    const lines = (await e.value()).split('\n');
    const rendered = await page.locator('.config-editor__code-line').allTextContents();
    expect(rendered).toEqual(lines);
  });
});

test.describe('hcl: multi-line object values', () => {
  setupEditor(test, '?language=hcl');

  test('keys are offered after a sibling entry', async ({ page }) => {
    const e = editor(page);
    await e.paste('web = {\n  hsts = true\n  \n}\n');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('true\n') + 7;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('le');
    await expect.poll(() => e.keys()).toContain('letsencrypt');
    expect(await e.keys()).not.toContain('hsts');
  });
});
