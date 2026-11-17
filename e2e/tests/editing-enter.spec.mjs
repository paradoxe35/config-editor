import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Enter, Ctrl+Enter and Ctrl+Shift+Enter: indentation, lists, block scalars, flow collections, column 0.

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

// Items moved in another view take their comments, an
// emptied YAML list keeps the gap before its comment, a YAML string over
// lines keeps its lines, the HCL view keeps whole floats, the XML view reads
// `0x50` as a number, Enter in a plain YAML scalar over lines, and the
// headless tools read a YAML comment line ending in a lone CR.

const saved = (page) => page.locator('.demo__output-text').textContent();

// Selects `needle` (or puts the caret after it with `collapse`).
async function select(e, needle, collapse = false) {
  await e.input.evaluate(
    (element, [find, end]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      const stop = start + find.length;
      element.setSelectionRange(end ? stop : start, stop);
    },
    [needle, collapse],
  );
}

// Puts the caret at `offset` of the text on screen.
const caretAt = (e, offset) =>
  e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);

// Puts the caret right after `needle`.
async function caretAfter(e, needle) {
  await e.input.evaluate((element, find) => {
    const at = element.value.indexOf(find);
    if (at < 0) throw new Error(`no ${find}`);
    element.focus();
    element.setSelectionRange(at + find.length, at + find.length);
  }, needle);
}

test.describe('yaml: editing conventions', () => {
  setupEditor(test, '?language=yaml');
  test('Ctrl+Enter opens a line below without splitting', async ({ page }) => {
    const e = await at(page, 'listen: 78|35\n');
    await page.keyboard.press('Control+Enter');
    expect(await e.value()).toBe('listen: 7835\n\n');
    expect(await e.caret()).toMatchObject({ line: 2 });
  });
  test('Enter on a line of only indentation leaves it empty', async ({ page }) => {
    const e = await at(page, 'a:\n  b: 1|');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('a:\n  b: 1\n\n  ');
  });
});

test.describe('yaml: editing', () => {
  setupEditor(test, '?language=yaml');
  test('Enter on an empty list item ends the list', async ({ page }) => {
    const e = await at(page, 'hosts:\n  - a|\n');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    // At the column of the key holding the list.
    expect(await e.value()).toBe('hosts:\n  - a\n\n');
  });
  test('Ctrl+Shift+Enter opens a line above', async ({ page }) => {
    const e = await at(page, 'x: 1\n  ab|c\n');
    await page.keyboard.press('Control+Shift+Enter');
    expect(await e.value()).toBe('x: 1\n  \n  abc\n');
    expect(await e.caret()).toMatchObject({ line: 2, column: 3 });
  });
  test('undo after Ctrl+Enter puts the caret back', async ({ page }) => {
    const e = await at(page, 'de|f: 2\n');
    await page.keyboard.press('Control+Enter');
    await page.keyboard.press('Control+z');
    expect(await e.value()).toBe('def: 2\n');
    expect(await e.caret()).toMatchObject({ line: 1, column: 3 });
  });
});

test.describe('json: editing', () => {
  setupEditor(test, '?language=json');
  test('Enter between brackets inside a string is a plain newline', async ({ page }) => {
    const e = await at(page, '{"p": "[|]"}');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{"p": "[\n]"}');
  });
});

test.describe('xml: typed values and end tags', () => {
  setupEditor(test, '?language=xml');
  test('Enter before a pasted end tag splits the line', async ({ page }) => {
    const e = await at(page, '<r>\n  <a>text|</a>\n</r>');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('<r>\n  <a>text\n  </a>\n</r>');
  });
});

// --- editing ---

test.describe('yaml: Enter at column 0 keeps the line where it is', () => {
  setupEditor(test, '?language=yaml');

  test('an indented line keeps its indentation', async ({ page }) => {
    const e = await at(page, 'server:\n  host: a\n|  port: 80\n');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('server:\n  host: a\n\n  port: 80\n');
  });

  test('a selected whole line is replaced and the next line keeps its indent', async ({ page }) => {
    const e = editor(page);
    await e.paste('b:\n  c: 2\n  d: 3\n');
    await page.locator('.config-editor__gutter-content > div[data-line="2"]').click();
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('b:\n\n  d: 3\n');
  });
});

test.describe('json: Enter at column 0 keeps the line where it is', () => {
  setupEditor(test, '?language=json');

  test('an indented member keeps its indentation', async ({ page }) => {
    const e = await at(page, '{\n  "a": 1,\n|  "b": 2\n}');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "a": 1,\n\n  "b": 2\n}');
  });

  test('a closer typed at column 0 of a blank line aligns', async ({ page }) => {
    const e = await at(page, '{\n  "a": {\n    "b": 1\n|');
    await page.keyboard.type('}');
    expect(await e.value()).toBe('{\n  "a": {\n    "b": 1\n  }');
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


  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');
    test('Enter in a one-line block opens it over lines', async ({ page }) => {
      const e = await at(page, 'tcp { nodelay = true¦ }\n');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('tcp {\n  nodelay = true\n  \n}\n');
    });
  });
});

// A group of its own (its own helpers).
test.describe(() => {
  // Further cases, replayed with real keys. The classes behind them
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
});

test.describe('YAML edited in other views', () => {
  setupEditor(test, '?language=yaml');
  test('Enter at the end of a plain scalar line continues it, before a comment keeps it one', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\nml: l1\n  l2  # mc\nn: 1\n');
    await expect.poll(() => saved(page)).toContain('# mc');
    await select(e, 'ml: l1', true);
    await page.keyboard.press('Enter');
    await page.keyboard.type('X');
    await expect.poll(() => e.value()).toBe('# c\nml: l1\n  X\n  l2  # mc\nn: 1\n');
    await select(e, '  l2', true);
    await page.keyboard.press('Enter');
    await page.keyboard.type('Y');
    await expect.poll(() => e.value()).toBe('# c\nml: l1\n  X\n  l2\n  Y  # mc\nn: 1\n');
  });
});

test.describe('YAML Enter', () => {
  setupEditor(test, '?language=yaml');

  test('on an empty item ends the list at its key', async ({ page }) => {
    const e = editor(page);
    await e.paste('l:\n  - a\n');
    await caretAt(e, 'l:\n  - a'.length);
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.keyboard.type('k: 1');
    await expect.poll(() => saved(page)).toBe('l:\n  - a\nk: 1\n');
  });

  test('inside a flow sequence goes one level deeper', async ({ page }) => {
    const e = editor(page);
    await e.paste('l: [a, b]\n');
    await caretAt(e, 'l: [a,'.length);
    await page.keyboard.press('Enter');
    await page.keyboard.type('z, ');
    await expect.poll(() => saved(page)).toBe('l: [a,\n  z, b]\n');
  });
});

// Enter after an opener with a
// trailing comment indents as without it, Enter on a blank line of a YAML
// block scalar keeps the scalar's indentation, and Enter before a closer
// after an entry's comma opens the brackets with no spacing left behind.
// (Every language: `enter_matrix` unit tests.)
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
