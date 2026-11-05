import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// XML coverage for `?language=xml` mode. XML shares `schema.json` with every
// other language: a wrapper element is transparent, top-level elements are the
// document properties, element text and attribute values are schema-typed, and
// repeated siblings fold into arrays (how the schema's arrays are written).
setupEditor(test, '?language=xml');

test.describe('XML documents', () => {
  test('the demo document validates against the shared schema', async ({ page }) => {
    const e = editor(page);
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
  test('an element typed inside the root is suggested, and Enter accepts it', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow></burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('</burrow>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('<lis');
    await expect.poll(() => e.keys()).toContain('listen');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('<burrow><listen></listen></burrow>');
  });

  test('a document without a wrapper validates against the shared schema', async ({ page }) => {
    const e = editor(page);
    await e.paste('<listen>7835</listen>\n<insecure_plaintext>false</insecure_plaintext>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a lone element is a document property and its text is typed', async ({ page }) => {
    const e = editor(page);
    // `false` parses as a boolean, so the schema's boolean property accepts it.
    await e.paste('<insecure_plaintext>false</insecure_plaintext>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await e.paste('<insecure_plaintext>maybe</insecure_plaintext>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('repeated siblings fold into an array instead of being duplicates', async ({ page }) => {
    const e = editor(page);
    // Secret is a string or a list of strings (rotation), so two <secret>
    // elements are a valid array, not a duplicate-key error.
    await e.paste(
      '<burrow>\n  <clients>\n    <acme>\n      <secret>first</secret>\n      <secret>second</secret>\n    </acme>\n  </clients>\n</burrow>\n',
    );
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('attributes are ordinary schema properties', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow><pool min_idle="1" max_idle="2"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await e.paste('<burrow><pool min_idle="many"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('an unknown attribute is reported', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow><pool bogus="1"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('attributes are suggested from their element schema', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow><pool/></burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('/>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(' min');
    await expect.poll(() => e.keys()).toContain('min_idle');
    await e.clickKey('min_idle');
    await expect(e.input).toHaveValue('<burrow><pool min_idle="4"/></burrow>');
  });

  test('Ctrl+/ toggles <!-- --> comments', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow>\n  <listen>7835</listen>\n</burrow>');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(11, 11);
    });
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('<burrow>\n  <!-- <listen>7835</listen> -->\n</burrow>');
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('<burrow>\n  <listen>7835</listen>\n</burrow>');
  });

  test('a schema violation is reported on its element', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow>\n  <bogus_key>1</bogus_key>\n</burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('the highlight overlay renders every character of the markup', async ({ page }) => {
    const e = editor(page);
    await e.paste(
      '<burrow>\n  <listen>7835</listen>\n  <!-- a comment -->\n  <public_address>tunnel.example.com</public_address>\n</burrow>\n',
    );
    const lines = (await e.value()).split('\n');
    const rendered = await page.locator('.config-editor__code-line').allTextContents();
    expect(rendered).toEqual(lines);
  });
});
