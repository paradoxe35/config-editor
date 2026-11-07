import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Eleventh-round findings, replayed with real keys. The classes behind them
// are covered by the unit matrices (`completion_matrix`, `editing_matrix`).

async function at(page, marked) {
  const e = editor(page);
  const first = marked.indexOf('¦');
  const last = marked.lastIndexOf('¦');
  const [start, end] = first === last ? [first, first] : [first, last - 1];
  await e.paste(marked.replaceAll('¦', ''));
  await e.input.evaluate(
    (element, [from, to]) => {
      element.focus();
      element.setSelectionRange(from, to);
    },
    [start, end],
  );
  return e;
}

test.describe('xml', () => {
  setupEditor(test, '?language=xml');

  test('a closing tag typed closes the open element instead of renaming it', async ({ page }) => {
    const e = await at(page, '<websocket>\n  <path>/x</path>\n  ¦');
    await page.keyboard.type('</we');
    await expect.poll(() => e.keys()).toEqual(['websocket']);
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('<websocket>\n  <path>/x</path>\n  </websocket>');
  });
});

for (const [language, text, caret] of [
  ['yaml', 'a:\n  b: 1\n  c: 2', 4],
  ['json', '{\n  "b": 1\n}', 3],
  ['toml', '[a]\n  b = 1\n', 5],
  ['hcl', 'a {\n  b = 1\n}', 5],
  ['xml', '<a>\n  <b>1</b>\n</a>', 5],
]) {
  test.describe(language, () => {
    setupEditor(test, `?language=${language}`);

    test('Enter inside a line’s indentation keeps the line’s column', async ({ page }) => {
      const e = await at(page, text.slice(0, caret) + '¦' + text.slice(caret));
      await page.keyboard.press('Enter');
      const lineStart = text.lastIndexOf('\n', caret - 1) + 1;
      expect(await e.value()).toBe(text.slice(0, lineStart) + '\n' + text.slice(lineStart));
    });

    test('undo after text inserted without a paste event puts the caret back', async ({ page }) => {
      const e = await at(page, text.slice(0, caret) + '¦' + text.slice(caret));
      await e.input.evaluate((element) => document.execCommand('insertText', false, 'x\ny'));
      await page.keyboard.press('Control+z');
      expect(await e.value()).toBe(text);
      expect(await e.input.evaluate((element) => element.selectionStart)).toBe(caret);
    });
  });
}
