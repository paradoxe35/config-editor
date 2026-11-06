import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// VS Code editing conventions the editor follows: indentation-aware
// Backspace, closer outdent, context-aware auto-closing, list refiltering,
// popup paging, and blank lines left alone by block indent.

async function caretAt(e, position) {
  await e.input.evaluate((element, at) => {
    element.focus();
    const offset = at < 0 ? element.value.length + at + 1 : at;
    element.setSelectionRange(offset, offset);
  }, position);
}

test.describe('indentation', () => {
  setupEditor(test);

  test('Backspace in leading indentation removes one indent unit', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('server:');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('server:\n  ');
    await page.keyboard.press('Backspace');
    await expect(e.input).toHaveValue('server:\n');
  });

  test('Tab over a selection leaves blank lines blank', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: 1\n\nb: 2\n');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, element.value.length - 1);
    });
    await page.keyboard.press('Tab');
    await expect(e.input).toHaveValue('  a: 1\n\n  b: 2\n');
  });
});

test.describe('closers', () => {
  setupEditor(test, '?language=json');

  test('typing } on a blank indented line aligns it with its opener', async ({ page }) => {
    const e = editor(page);
    await e.paste('{\n  "web": {\n    "http": 80,\n    \n}');
    await caretAt(e, '{\n  "web": {\n    "http": 80,\n    '.length);
    await page.keyboard.type('}');
    await expect(e.input).toHaveValue('{\n  "web": {\n    "http": 80,\n  }\n}');
  });
});

test.describe('auto-closing', () => {
  setupEditor(test);

  test('a quote after a word is an apostrophe, not a pair', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('msg: don');
    await page.keyboard.type("'t");
    await expect(e.input).toHaveValue("msg: don't");
  });

  test('an opening bracket before a word does not auto-close', async ({ page }) => {
    const e = editor(page);
    await e.paste('hosts: a\n');
    await caretAt(e, 'hosts: '.length);
    await page.keyboard.type('[');
    await expect(e.input).toHaveValue('hosts: [a\n');
  });

  test('an opening bracket before whitespace still auto-closes', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('hosts: ');
    await page.keyboard.type('[');
    await expect(e.input).toHaveValue('hosts: []');
  });
});

test.describe('the suggestion list', () => {
  setupEditor(test);

  test('Backspace keeps an open list open and refilters it', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.type('insecure_x');
    await page.keyboard.press('Control+Space');
    await page.keyboard.press('Backspace');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    expect(await e.isOpen()).toBe(true);
  });

  test('PageDown and PageUp move the highlight by a page', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.press('Control+Space');
    await expect.poll(async () => (await e.keys()).length).toBeGreaterThan(8);
    const keys = await e.keys();
    await page.keyboard.press('PageDown');
    await expect.poll(() => e.highlighted()).toEqual([keys[7]]);
    await page.keyboard.press('PageUp');
    await expect.poll(() => e.highlighted()).toEqual([keys[0]]);
  });
});
