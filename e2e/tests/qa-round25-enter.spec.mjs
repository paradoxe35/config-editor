import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 25 editing LOWs, with real keys: Enter after an opener with a
// trailing comment indents as without it, Enter on a blank line of a YAML
// block scalar keeps the scalar's indentation, and Enter before a closer
// after an entry's comma opens the brackets with no spacing left behind.
// (Every language: `enter_matrix` unit tests.)

// Puts the caret right after `needle`.
async function caretAfter(e, needle) {
  await e.input.evaluate((element, find) => {
    const at = element.value.indexOf(find);
    if (at < 0) throw new Error(`no ${find}`);
    element.focus();
    element.setSelectionRange(at + find.length, at + find.length);
  }, needle);
}

test.describe('Enter in YAML', () => {
  setupEditor(test, '?language=yaml');

  test('after an opener with a trailing comment, the new line is indented', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: # c\nl:\n  - b: # d\n');
    await caretAfter(e, 'a: # c');
    await page.keyboard.press('Enter');
    await page.keyboard.type('x: 1');
    await caretAfter(e, '- b: # d');
    await page.keyboard.press('Enter');
    await page.keyboard.type('y: 2');
    await expect.poll(() => e.value()).toBe('a: # c\n  x: 1\nl:\n  - b: # d\n      y: 2\n');
  });

  test('on a blank line of a block scalar, the scalar goes on', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: |\n  one\n\n  two\nb: 1\n');
    await caretAfter(e, 'one\n');
    await page.keyboard.press('Enter');
    await page.keyboard.type('mid');
    await expect.poll(() => e.value()).toBe('a: |\n  one\n\n  mid\n  two\nb: 1\n');
  });

  test('before a flow closer after a comma, the entries open one level deeper', async ({ page }) => {
    const e = editor(page);
    await e.paste('l: [1, 2, ]\n');
    await caretAfter(e, '2, ');
    await page.keyboard.press('Enter');
    await page.keyboard.type('3');
    await expect.poll(() => e.value()).toBe('l: [1, 2,\n  3\n]\n');
  });
});

test.describe('Enter in HCL', () => {
  setupEditor(test, '?language=hcl');

  test('after a block opener with a trailing comment, the new line is indented', async ({ page }) => {
    const e = editor(page);
    await e.paste('b { # c\n}\n');
    await caretAfter(e, '# c');
    await page.keyboard.press('Enter');
    await page.keyboard.type('x = 1');
    await expect.poll(() => e.value()).toBe('b { # c\n  x = 1\n}\n');
  });
});
