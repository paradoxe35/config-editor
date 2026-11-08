import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Typing a word that matches nothing leaves no list; an arrow key then moves
// the caret, also when the key is held down the way a person holds it (its
// keyup comes after the caret moved, or after several repeats). The caret
// lands inside a key on the next line, where a list could open: it must not,
// or the key would move through that list instead of the text.

const CASES = [
  ['yaml', 'tcp:\n  nodelay: ¦\n  keepalive_secs: 3\n'],
  ['toml', '[tcp]\nnodelay = ¦\nkeepalive_secs = 3\n'],
  ['json', '{\n  "tcp": {\n    "nodelay": ¦,\n    "keepalive_secs": 3\n  }\n}'],
  ['xml', '<tcp>\n  <nodelay>¦</nodelay>\n  <keepalive_secs>3</keepalive_secs>\n</tcp>'],
  ['hcl', 'tcp {\n  nodelay = ¦\n  keepalive_secs = 3\n}'],
];

// How long a person holds a key: the browser moves the caret on keydown.
const HOLD_MS = 80;

async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('¦');
  await e.paste(marked.replace('¦', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

for (const [language, marked] of CASES) {
  test.describe(`${language}: arrow keys after a word that matches nothing`, () => {
    setupEditor(test, `?language=${language}`);
    const line = marked.slice(0, marked.indexOf('¦')).split('\n').length;

    test('ArrowDown pressed as a person presses it moves the caret', async ({ page }) => {
      const e = await at(page, marked);
      await page.keyboard.type('yes', { delay: HOLD_MS });
      await page.keyboard.press('ArrowDown', { delay: HOLD_MS });
      await page.waitForTimeout(150);
      expect(await e.isOpen(), `list ${JSON.stringify(await e.keys())}`).toBe(false);
      expect((await e.caret()).line).toBe(line + 1);
    });

    test('a held ArrowDown keeps moving the caret', async ({ page }) => {
      const e = await at(page, `${marked}\n\n\n`);
      await page.keyboard.type('yes', { delay: HOLD_MS });
      await page.keyboard.down('ArrowDown');
      await page.waitForTimeout(HOLD_MS);
      await page.keyboard.down('ArrowDown');
      await page.waitForTimeout(HOLD_MS);
      await page.keyboard.up('ArrowDown');
      await page.waitForTimeout(150);
      expect(await e.isOpen(), `list ${JSON.stringify(await e.keys())}`).toBe(false);
      expect((await e.caret()).line).toBe(line + 2);
    });

    test('ArrowUp pressed as a person presses it moves the caret', async ({ page }) => {
      const e = await at(page, marked);
      await page.keyboard.type('yes', { delay: HOLD_MS });
      await page.keyboard.press('ArrowUp', { delay: HOLD_MS });
      await page.waitForTimeout(150);
      expect(await e.isOpen(), `list ${JSON.stringify(await e.keys())}`).toBe(false);
      expect((await e.caret()).line).toBe(line - 1);
    });
  });
}
