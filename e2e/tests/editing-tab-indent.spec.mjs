import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Tab and Shift+Tab: tab stops, indenting selections, putting lines back.

// Pastes `marked` and puts the caret at `|`.
async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

test.describe('yaml: editing conventions', () => {
  setupEditor(test, '?language=yaml');
  test('Tab inserts spaces up to the next tab stop', async ({ page }) => {
    const e = await at(page, 'a:\n  b: 1\nabc|');
    await page.keyboard.press('Tab');
    expect(await e.value()).toBe('a:\n  b: 1\nabc ');
  });
});

test.describe('yaml: whole lines and selections', () => {
  setupEditor(test, '?language=yaml');
  test('Tab with a whole line selected indents it', async ({ page }) => {
    const e = await at(page, 'a:\n|b: 1\nc: 2');
    await page.keyboard.press('Shift+End');
    await page.keyboard.press('Tab');
    expect(await e.value()).toBe('a:\n  b: 1\nc: 2');
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  async function select(page, text, from, to) {
    const e = editor(page);
    await e.paste(text);
    await e.input.evaluate(
      (element, [start, end]) => {
        element.focus();
        element.setSelectionRange(start, end);
      },
      [from, to],
    );
    return e;
  }

  async function at(page, marked) {
    const position = marked.indexOf('¦');
    return select(page, marked.replace('¦', ''), position, position);
  }


  test.describe('Tab then Shift+Tab puts lines back', () => {
    for (const [language, text, first] of [
      ['yaml', 'x:\n  a: 1\n  b: 2', '  a: 1'],
      ['json', '{\n  "a": 1,\n  "b": 2\n}', '  "a": 1,'],
    ]) {
      test.describe(language, () => {
        setupEditor(test, `?language=${language}`);

        test('a selection of indented lines', async ({ page }) => {
          const start = text.indexOf(first);
          const end = language === 'yaml' ? text.length : text.indexOf('\n}');
          const e = await select(page, text, start, end);
          await page.keyboard.press('Tab');
          await page.keyboard.press('Shift+Tab');
          expect(await e.value()).toBe(text);
        });

        test('the caret on an indented line', async ({ page }) => {
          const at = text.indexOf(first) + 2;
          const e = await select(page, text, at, at);
          await page.keyboard.press('Tab');
          await page.keyboard.press('Shift+Tab');
          expect(await e.value()).toBe(text);
        });
      });
    }
  });
});

// A group of its own (its own helpers).
test.describe(() => {
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


  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('Tab on a selection starting inside the indentation keeps its column', async ({ page }) => {
      const e = await at(page, 'tcp:\n ¦ nodelay: true\n  keepalive_secs: 3¦0\n');
      await page.keyboard.press('Tab');
      const selection = await e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);
      expect(selection[0]).toBe(5 + 3);
      await page.keyboard.press('Shift+Tab');
      expect(await e.value()).toBe('tcp:\n  nodelay: true\n  keepalive_secs: 30\n');
      const back = await e.input.evaluate((element) => element.selectionStart);
      expect(back).toBe(5 + 1);
    });
  });
});
