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

// Suggestions follow typing only: moving the caret is navigation and never
// opens the list (and closes an open one); typing, and accepting a key,
// open it.
test.describe('suggestions open only while typing', () => {
  setupEditor(test);

  const end = async (e) =>
    e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });

  test('arrow keys, Home, End, and PageUp/PageDown never open the list', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\npublic_address: tunnel.example.com\n');
    await end(e);
    for (const key of ['ArrowUp', 'ArrowUp', 'ArrowRight', 'ArrowLeft', 'End', 'Home', 'ArrowDown', 'PageUp', 'PageDown']) {
      await page.keyboard.press(key);
      await page.waitForTimeout(60);
      expect(await e.isOpen(), `after ${key}`).toBe(false);
    }
  });

  test('moving the caret closes an open list, and typing reopens it', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    await end(e);
    // ArrowUp/ArrowDown move inside an open list instead (tested below).
    for (const key of ['ArrowLeft', 'ArrowRight', 'Home', 'End']) {
      await page.keyboard.type('pub');
      await expect.poll(() => e.isOpen()).toBe(true);
      await page.keyboard.press(key);
      await expect.poll(() => e.isOpen(), `after ${key}`).toBe(false);
      await e.paste('listen: 7835\n');
      await end(e);
    }
  });

  test('a click elsewhere closes an open list', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    await end(e);
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    await e.input.click({ position: { x: 20, y: 10 } });
    await expect.poll(() => e.isOpen()).toBe(false);
  });

  test('ArrowUp and ArrowDown move inside an open list without closing it', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.type('i');
    await expect.poll(async () => (await e.keys()).length).toBeGreaterThan(1);
    const keys = await e.keys();
    await page.keyboard.press('ArrowDown');
    await expect.poll(() => e.highlighted()).toEqual([keys[1]]);
    await page.keyboard.press('ArrowUp');
    await expect.poll(() => e.highlighted()).toEqual([keys[0]]);
    expect(await e.isOpen()).toBe(true);
  });

  test('accepting a key continues into its children', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.type('pool');
    await expect.poll(() => e.keys()).toContain('pool');
    await page.keyboard.press('Enter');
    // The list reopens for the children of the accepted key.
    await expect.poll(() => e.keys()).toContain('min_idle');
  });

  test('typing where nothing can follow keeps the list closed', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('# a comment about pool');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

// Puts the caret at `|` in a pasted document.
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

test.describe('json: pairs and overtyping', () => {
  setupEditor(test, '?language=json');

  test('an opening brace before an outer closer still pairs', async ({ page }) => {
    const e = await at(page, '{"services": {"web": |}}');
    await page.keyboard.type('{');
    expect(await e.value()).toBe('{"services": {"web": {}}}');
  });

  test('a closer the editor inserted is typed over; one already there is not', async ({ page }) => {
    const e = await at(page, '{"a": |}');
    await page.keyboard.type('[1]');
    expect(await e.value()).toBe('{"a": [1]}');
    // The `}` was pasted, not inserted: typing one adds it.
    await page.keyboard.type('}');
    expect(await e.value()).toBe('{"a": [1]}}');
  });

  test('a quote after a backslash is text, not the end of the string', async ({ page }) => {
    const e = await at(page, '{"a": |}');
    await page.keyboard.type('"x\\"');
    expect(await e.value()).toBe('{"a": "x\\""}');
  });

  test('Enter before a closer puts it on its own line, aligned with its opener', async ({ page }) => {
    const e = await at(page, '{\n  "a": 1|}');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "a": 1\n}');
    expect(await e.caret()).toMatchObject({ line: 3, column: 1 });
  });
});

for (const [language, marked, expected] of [
  ['yaml', 'ports: [|]\n', 'ports: [\n  \n]\n'],
  ['toml', 'ports = [|]\n', 'ports = [\n  \n]\n'],
  ['hcl', 'ports = [|]\n', 'ports = [\n  \n]\n'],
]) {
  test.describe(`${language}: Enter between brackets`, () => {
    setupEditor(test, `?language=${language}`);

    test('opens an indented middle line', async ({ page }) => {
      const e = await at(page, marked);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe(expected);
      expect(await e.caret()).toMatchObject({ line: 2, column: 3 });
    });
  });
}

test.describe('toml: Enter in lines that cannot span', () => {
  setupEditor(test, '?language=toml');

  test('Enter at the end of an inline table steps out to the next line', async ({ page }) => {
    const e = await at(page, '[[web.letsencrypt.dns]]\ncloudflare = { token = "tok"| }\n');
    await page.keyboard.press('Enter');
    await page.keyboard.type('domains = ["a.example"]');
    expect(await e.value()).toBe(
      '[[web.letsencrypt.dns]]\ncloudflare = { token = "tok" }\ndomains = ["a.example"]\n',
    );
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('Enter after a typed table header leaves the header whole', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('[limits');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Enter');
    await page.keyboard.type('max_pending = 1');
    expect(await e.value()).toBe('[limits]\nmax_pending = 1');
  });
});

test.describe('xml: tags', () => {
  setupEditor(test, '?language=xml');

  test('typing > after a start tag writes its end tag', async ({ page }) => {
    const e = await at(page, '<clients>\n  |\n</clients>\n');
    await page.keyboard.type('<bob');
    await page.keyboard.press('Escape');
    await page.keyboard.type('>');
    expect(await e.value()).toBe('<clients>\n  <bob></bob>\n</clients>\n');
    expect(await e.caret()).toMatchObject({ line: 2, column: 8 });
  });

  test('Enter after the prolog does not indent', async ({ page }) => {
    const e = await at(page, '<?xml version="1.0"?>|\n');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('<?xml version="1.0"?>\n\n');
    expect(await e.caret()).toMatchObject({ line: 2, column: 1 });
  });
});
