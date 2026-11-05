import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

setupEditor(test, '?language=json');

test.describe('JSON documents', () => {
  test('a key typed inside an object is suggested from the schema, and Enter accepts it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  ""\n}');
    // Caret between the two quotes of the empty key.
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(5, 5);
    });
    await page.keyboard.type('lis');
    // `limits` also matches "lis" as a subsequence, so rank is the contract.
    await expect.poll(() => e.keys()).toContain('listen');
    expect((await e.keys())[0]).toBe('listen');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('{\n  "listen": \n}');
  });

  test('a boolean value is suggested after its colon', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "insecure_plaintext": \n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length - 2, element.value.length - 2);
    });
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toEqual(['true']);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('{\n  "insecure_plaintext": true\n}');
  });

  test('a key inside an inline object is suggested', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "acme": {""}\n  }\n}');
    const caret = (await e.value()).indexOf('{""}') + 2;
    await e.input.evaluate((element, position) => {
      element.focus();
      element.setSelectionRange(position, position);
    }, caret);
    await page.keyboard.type('sec');
    await expect.poll(() => e.keys()).toEqual(['secret']);
  });

  test('a repeated key is reported on its second occurrence', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 1,\n  "listen": 2\n}');
    await expect.poll(() => e.diagnosticCount(), 'duplicate key is an error').toBe(1);
  });

  test('Ctrl+/ does nothing, because JSON has no comments', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 7835\n}');
    await e.input.click();
    await page.keyboard.press('Control+/');
    await page.waitForTimeout(100);
    await expect(e.input).toHaveValue('{\n  "listen": 7835\n}');
  });

  test('a typed opening quote opens the key list at once, as in VS Code', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  \n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(4, 4);
    });
    await page.keyboard.type('"');
    await expect.poll(() => e.isOpen()).toBe(true);
    expect(await e.keys()).toEqual(expect.arrayContaining(['listen', 'clients']));
  });

  test('Enter between braces opens an indented middle line with the caret on it', async ({ page }) => {
    const e = editor(page);
    await e.paste('{}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(1, 1);
    });
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('{\n  \n}');
    expect(await e.caret()).toMatchObject({ line: 2, column: 3 });
  });

  test('a new entry after a value without a comma gets the comma', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "listen": 1\n}');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.indexOf('1') + 1, element.value.indexOf('1') + 1);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('ins');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    await e.clickKey('insecure_plaintext');
    await expect(e.input).toHaveValue('{\n  "listen": 1\n  , "insecure_plaintext": \n}');
  });

  test('a key typed after a complete client entry gets its comma and is accepted', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "clients": {\n    "alex": {"secret": "x"}\n  }\n}');
    const caret = (await e.value()).indexOf('"x"}') + 3;
    await e.input.evaluate((element, position) => {
      element.focus();
      element.setSelectionRange(position, position);
    }, caret);
    await page.keyboard.type('"max');
    await expect.poll(() => e.keys()).toContain('max_connections');
    await e.clickKey('max_connections');
    await expect(e.input).toHaveValue('{\n  "clients": {\n    "alex": {"secret": "x", "max_connections": }\n  }\n}');
  });
});
