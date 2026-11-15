import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Typed pairs and closers: brackets, quotes, end tags, heredoc and template openers, never doubled.

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

const selection = (e) =>
  e.input.evaluate((element) => ({
    start: element.selectionStart,
    end: element.selectionEnd,
    direction: element.selectionDirection,
  }));

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

// QA round 21 typing: a YAML block scalar holding an empty line froze the
// tab (loading it, Enter in it, Edit as YAML of a string with a blank line);
// copy and cut on a folded header take the whole block; Enter on an empty
// `- ` item and inside a flow sequence; XML's `</` and an attribute quote
// before `/>`; HCL's `"${`; a find pattern matching empty text (`^`).

const saved = (page) => page.locator('.demo__output-text').textContent();

// Puts the caret at `offset` of the text on screen.
const caretAt = (e, offset) =>
  e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);

test.describe('json: editing', () => {
  setupEditor(test, '?language=json');
  test('Backspace between a pair the editor did not insert deletes one character', async ({ page }) => {
    const e = await at(page, '{"a": [|]}');
    await page.keyboard.press('Backspace');
    expect(await e.value()).toBe('{"a": ]}');
  });
});

test.describe('xml: end tags typed on their own line', () => {
  setupEditor(test, '?language=xml');

  test('move out to the element they close', async ({ page }) => {
    const e = await at(page, '<limits>\n  <max_pending>\n    5\n    |');
    await page.keyboard.type('</max_pending>');
    expect(await e.value()).toBe('<limits>\n  <max_pending>\n    5\n  </max_pending>');
  });
});

test.describe('xml: typed values and end tags', () => {
  setupEditor(test, '?language=xml');
  test('a tag in a comment is not closed', async ({ page }) => {
    const e = await at(page, '<!-- <b| -->');
    await page.keyboard.type('>');
    expect(await e.value()).toBe('<!-- <b> -->');
  });
});

test.describe('typed closers ignore brackets in strings and comments', () => {
  setupEditor(test, '?language=hcl');

  test('a brace in a comment is not an opener', async ({ page }) => {
    const e = await at(page, 'resource "x" "y" {\n  # TODO: {\n  a = 1\n  |');
    await page.keyboard.type('}');
    expect(await e.value()).toBe('resource "x" "y" {\n  # TODO: {\n  a = 1\n}');
  });
});

test.describe('wrapping a backward selection keeps it backward', () => {
  setupEditor(test, '?language=yaml');

  test('quotes around text selected with Shift+Left', async ({ page }) => {
    const e = await at(page, 'name: abc|\n');
    for (let index = 0; index < 3; index += 1) await page.keyboard.press('Shift+ArrowLeft');
    await page.keyboard.type('"');
    expect(await e.value()).toBe('name: "abc"\n');
    expect((await selection(e)).direction).toBe('backward');
  });
});

test.describe('toml: typing a multi-line string', () => {
  setupEditor(test, '?language=toml');

  for (const quote of ['"', "'"]) {
    test(`types over the closing ${quote.repeat(3)}`, async ({ page }) => {
      const e = await at(page, 'motd = |');
      await page.keyboard.type(`${quote.repeat(3)}Welcome${quote.repeat(3)}`);
      expect(await e.value()).toBe(`motd = ${quote.repeat(3)}Welcome${quote.repeat(3)}`);
      expect((await e.caret()).column).toBe(`motd = ${quote.repeat(3)}Welcome${quote.repeat(3)}`.length + 1);
    });
  }

  test('Backspace right after the opening """ removes a quote on each side', async ({ page }) => {
    const e = await at(page, 'motd = |');
    await page.keyboard.type('"""');
    expect(await e.value()).toBe('motd = """"""');
    await page.keyboard.press('Backspace');
    expect(await e.value()).toBe('motd = """"');
  });
});

test.describe('xml: a typed end tag lines up with its opening tag', () => {
  setupEditor(test, '?language=xml');

  for (const [name, text] of [
    ['after Enter on a text line', '<r>\n  <a>text\n  '],
    ['after an opening tag with attributes', '<r>\n  <a x="1">\n  '],
    ['at column 0 of a blank line', '<r>\n  <a>\n'],
    ['on a deeper line', '<r>\n  <a>\n        '],
  ]) {
    test(name, async ({ page }) => {
      const e = await select(page, text, text.length, text.length);
      await page.keyboard.type('</a>');
      const lines = (await e.value()).split('\n');
      expect(lines[lines.length - 1]).toBe('  </a>');
    });
  }
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


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');
    test('> typed before a tag’s own > moves over it', async ({ page }) => {
      const e = await at(page, '<tcp¦></tcp>');
      await page.keyboard.type('>');
      expect(await e.value()).toBe('<tcp></tcp>');
    });
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
    test('a closing brace typed above the one written for the block is not doubled', async ({ page }) => {
      const e = await at(page, '¦');
      await page.keyboard.type('tcp {');
      await page.keyboard.press('Enter');
      await page.keyboard.type('nodelay = true');
      await page.keyboard.press('Enter');
      await page.keyboard.type('}');
      expect(await e.value()).toBe('tcp {\n  nodelay = true\n}');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('an end tag typed where the editor already wrote it is not doubled', async ({ page }) => {
      const e = await at(page, '¦');
      await page.keyboard.type('<tcp>');
      await page.keyboard.press('Enter');
      await page.keyboard.type('<nodelay>true');
      await page.keyboard.press('End');
      await page.keyboard.press('Enter');
      await page.keyboard.type('</tcp>');
      expect((await e.value()).match(/<\/tcp>/g)).toHaveLength(1);
    });

    test('an end tag typed right before its written copy is not doubled', async ({ page }) => {
      const e = await at(page, '¦');
      await page.keyboard.type('<tcp></tcp>');
      expect(await e.value()).toBe('<tcp></tcp>');
    });
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


  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('a closing tag typed closes the open element instead of renaming it', async ({ page }) => {
      const e = await at(page, '<websocket>\n  <path>/x</path>\n  ¦');
      // `</` writes the open element's end tag, lined up with its start tag
      // (round 21); the name typed after it types over it.
      await page.keyboard.type('</we');
      expect(await e.value()).toBe('<websocket>\n  <path>/x</path>\n</websocket>');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('<websocket>\n  <path>/x</path>\n</websocket>\n');
    });
  });
});

test.describe('XML typing', () => {
  setupEditor(test, '?language=xml');

  test('</ writes the end tag of the open element', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <a>x\n</r>\n');
    await caretAt(e, '<r>\n  <a>x'.length);
    await page.keyboard.type('</');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n</r>\n');
    // Enter steps past it.
    await page.keyboard.press('Enter');
    await page.keyboard.type('<b>1');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n  <b>1</b>\n</r>\n');
  });

  test('an end tag typed whole after </ is written once', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <a>x\n</r>\n');
    await caretAt(e, '<r>\n  <a>x'.length);
    await page.keyboard.type('</a>');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n</r>\n');
  });

  test('an attribute quote closes before />', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r/>\n');
    await caretAt(e, 2);
    await page.keyboard.type(' a="1');
    await expect.poll(() => saved(page)).toBe('<r a="1"/>\n');
    await page.keyboard.type('"');
    await expect.poll(() => e.value()).toBe('<r a="1"/>\n');
  });
});

test.describe('HCL typing', () => {
  setupEditor(test, '?language=hcl');

  test('"${ writes its }', async ({ page }) => {
    const e = editor(page);
    await e.paste('n = 1\n');
    await caretAt(e, 'n = 1\n'.length);
    await page.keyboard.type('a = "${var.x}"');
    await expect.poll(() => saved(page)).toBe('n = 1\na = "${var.x}"');
  });
});

// QA round 23 editing: HCL's `${` and `%{` close inside a heredoc body as
// in a quoted string (the key handler skipped what the language closes
// in every multi-line string body), typed over and deleted with their
// opener; brackets there still do not pair.
test.describe('HCL heredoc templates', () => {
  setupEditor(test, '?language=hcl');

  const host = 'msg = <<EOT\nhi \nEOT\n';
  const body = 'msg = <<EOT\nhi '.length;

  for (const sigil of ['$', '%']) {
    test(`${sigil}{ writes its } in a heredoc, typed over`, async ({ page }) => {
      const e = editor(page);
      await e.paste(host);
      await caretAt(e, body);
      await page.keyboard.type(`${sigil}{`);
      await expect.poll(() => e.value()).toBe(`msg = <<EOT\nhi ${sigil}{}\nEOT\n`);
      await page.keyboard.type('x}');
      await expect.poll(() => saved(page)).toBe(`msg = <<EOT\nhi ${sigil}{x}\nEOT\n`);
    });
  }

  test('Backspace deletes the pair, and brackets do not pair', async ({ page }) => {
    const e = editor(page);
    await e.paste(host);
    await caretAt(e, body);
    await page.keyboard.type('${');
    await page.keyboard.press('Backspace');
    await expect.poll(() => e.value()).toBe('msg = <<EOT\nhi $\nEOT\n');
    await page.keyboard.type(' ( [');
    await expect.poll(() => saved(page)).toBe('msg = <<EOT\nhi $ ( [\nEOT\n');
  });
});
