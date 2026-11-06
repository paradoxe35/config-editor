import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// XML coverage for `?language=xml` mode. XML shares `schema.json` with every
// other language. Only a top-level element marked `config-root="true"` is a
// transparent root; otherwise every top-level element is a document property.
// Element text and attribute values are schema-typed, and repeated siblings
// fold into arrays (how the schema's arrays are written).
setupEditor(test, '?language=xml');

test.describe('XML documents', () => {
  test('the demo document validates against the shared schema', async ({ page }) => {
    const e = editor(page);
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
  test('an element typed inside the root is suggested, and Enter accepts it', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true"></burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('</burrow>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('<lis');
    await expect.poll(() => e.keys()).toContain('listen');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('<burrow config-root="true"><listen></listen></burrow>');
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
      '<burrow config-root="true">\n  <clients>\n    <acme>\n      <secret>first</secret>\n      <secret>second</secret>\n    </acme>\n  </clients>\n</burrow>\n',
    );
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('attributes are ordinary schema properties', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true"><pool min_idle="1" max_idle="2"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    await e.paste('<burrow config-root="true"><pool min_idle="many"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('an unknown attribute is reported', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true"><pool bogus="1"/></burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('attributes are suggested from their element schema', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true"><pool/></burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('/>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(' min');
    await expect.poll(() => e.keys()).toContain('min_idle');
    await e.clickKey('min_idle');
    await expect(e.input).toHaveValue('<burrow config-root="true"><pool min_idle="4"/></burrow>');
  });

  test('Ctrl+/ toggles <!-- --> comments', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true">\n  <listen>7835</listen>\n</burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('<listen>') + 2;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('<burrow config-root="true">\n  <!-- <listen>7835</listen> -->\n</burrow>');
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('<burrow config-root="true">\n  <listen>7835</listen>\n</burrow>');
  });

  test('a schema violation is reported on its element', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true">\n  <bogus_key>1</bogus_key>\n</burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('the highlight overlay renders every character of the markup', async ({ page }) => {
    const e = editor(page);
    await e.paste(
      '<burrow config-root="true">\n  <listen>7835</listen>\n  <!-- a comment -->\n  <public_address>tunnel.example.com</public_address>\n</burrow>\n',
    );
    const lines = (await e.value()).split('\n');
    const rendered = await page.locator('.config-editor__code-line').allTextContents();
    expect(rendered).toEqual(lines);
  });
});

test.describe('XML config root', () => {
  test('a marked root is transparent whatever its name', async ({ page }) => {
    const e = editor(page);
    await e.paste('<anything config-root="true">\n  <listen>7835</listen>\n</anything>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('an unmarked wrapper is configuration data, so the schema rejects it', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow>\n  <listen>7835</listen>\n</burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
    // Marking it makes the same document valid.
    await e.paste('<burrow config-root="true">\n  <listen>7835</listen>\n</burrow>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('an element beside the config root is a parse error', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true">\n  <listen>7835</listen>\n</burrow>\n<pool/>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('two config roots are a parse error', async ({ page }) => {
    const e = editor(page);
    await e.paste('<a config-root="true"><listen>1</listen></a>\n<b config-root="true"></b>\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
  });

  test('nothing is suggested inside the config root start tag', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true"></burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(' li');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.keys()).toEqual([]);
  });

  test('elements complete against the document root under a config root', async ({ page }) => {
    const e = editor(page);
    await e.paste('<burrow config-root="true">\n  \n</burrow>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('\n  ') + 3;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('<pub');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue(
      '<burrow config-root="true">\n  <public_address></public_address>\n</burrow>',
    );
    expect((await e.caret()).line).toBe(2);
  });
});

test.describe('XML completion formatting', () => {
  test('a new element on its own line after </clients> stays on that line', async ({ page }) => {
    const e = editor(page);
    const demo = await e.value();
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('visi');
    await expect.poll(() => e.keys()).toContain('visitors');
    await page.keyboard.press('Enter');
    const value = await e.value();
    expect(value).toBe(`${demo}<visitors></visitors>`);
    expect(value).toContain('</clients>\n<visitors>');
  });

  test('an indented new element keeps its indentation', async ({ page }) => {
    const e = editor(page);
    await e.paste('<clients>\n  <acme>\n    <secret>x</secret>\n    \n  </acme>\n</clients>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('</secret>\n    ') + '</secret>\n    '.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('max');
    await expect.poll(() => e.keys()).toContain('max_connections');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue(
      '<clients>\n  <acme>\n    <secret>x</secret>\n    <max_connections></max_connections>\n  </acme>\n</clients>',
    );
  });

  test('a block element accepted after an open tag on its line is formatted', async ({ page }) => {
    const e = editor(page);
    await e.paste('<web></web>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('</web>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('<lets');
    await expect.poll(() => e.keys()).toContain('letsencrypt');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue(
      '<web>\n  <letsencrypt>\n    <email></email>\n  </letsencrypt>\n</web>',
    );
    const caret = await e.caret();
    expect(caret.line).toBe(3);
  });

  test('a scalar element between tags stays compact with the caret inside', async ({ page }) => {
    const e = editor(page);
    await e.paste('<web></web>');
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('</web>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('<http2');
    await expect.poll(() => e.keys()).toContain('http2');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('<web><http2></http2></web>');
    await page.keyboard.type('true');
    await expect(e.input).toHaveValue('<web><http2>true</http2></web>');
  });
});
