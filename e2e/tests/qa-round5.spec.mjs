import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Fifth-round findings, replayed with real keys.

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

test.describe('a long backward selection keeps its caret in view', () => {
  setupEditor(test, '?language=yaml');

  const lines = ['root:', ...Array.from({ length: 200 }, (_, index) => `  key${index}: ${index}`)];
  const caretInView = (e) =>
    e.input.evaluate((element) => {
      const caret = element.selectionDirection === 'backward' ? element.selectionStart : element.selectionEnd;
      const line = element.value.slice(0, caret).split('\n').length;
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
      const top = (line - 1) * lineHeight;
      return top >= element.scrollTop - lineHeight && top <= element.scrollTop + element.clientHeight;
    });

  for (const [name, key] of [
    ['Tab', 'Tab'],
    ['Shift+Tab', 'Shift+Tab'],
    ['Ctrl+/', 'Control+/'],
  ]) {
    test(name, async ({ page }) => {
      const e = editor(page);
      await e.paste(`${lines.join('\n')}\n`);
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('  key80: ');
        element.focus();
        element.setSelectionRange(at, at);
      });
      for (let index = 0; index < 50; index += 1) await page.keyboard.press('Shift+ArrowUp');
      expect(await caretInView(e)).toBe(true);
      await page.keyboard.press(key);
      expect(await caretInView(e)).toBe(true);
    });
  }
});

test.describe('Tab at the end of the last empty value steps out of it', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('after the last string of an accepted table', async ({ page }) => {
      const e = await at(page, '[websocket]\nlisten = 1\n|');
      await page.keyboard.type('tls');
      await expect.poll(() => e.keys()).toContain('tls');
      await page.keyboard.press('Enter');
      await page.keyboard.type('a.pem');
      await page.keyboard.press('Tab');
      await page.keyboard.type('k.pem');
      await page.keyboard.press('Tab');
      const text = await e.value();
      expect(text).toContain('"k.pem"');
      expect(text).not.toContain('k.pem ');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('after the text of an element the editor closed', async ({ page }) => {
      const e = await at(page, '|');
      await page.keyboard.type('<listen>7836');
      await page.keyboard.press('Tab');
      const text = await e.value();
      expect(text).toBe('<listen>7836</listen>');
      expect((await e.caret()).column).toBe('<listen>7836</listen>'.length + 1);
    });
  });
});

// The caret position is given separately: `|` is YAML syntax here.
async function atOffset(page, text, offset) {
  const e = editor(page);
  await e.paste(text);
  await e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);
  return e;
}

test.describe('multi-line string bodies are text, not code', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test("Enter between brackets in a ''' string is a plain line break", async ({ page }) => {
      const text = "script = '''\necho []\n'''\n";
      const e = await atOffset(page, text, text.indexOf('[]') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe("script = '''\necho [\n]\n'''\n");
    });

    test('Enter in a """ string does not step out of braces', async ({ page }) => {
      const text = 'tpl = """\n{}\n"""\n';
      const e = await atOffset(page, text, text.indexOf('{}') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('tpl = """\n{\n}\n"""\n');
    });

    test('a key = value line in a string body is not colored as a key', async ({ page }) => {
      const e = editor(page);
      await e.paste("motd = '''\nkey = 1\n'''\n");
      const line = page.locator('.config-editor__code-line[data-line="2"]');
      await expect(line).toContainText('key = 1');
      await expect(line.locator('.config-editor__key')).toHaveCount(0);
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('Enter between braces in a heredoc keeps the line indentation', async ({ page }) => {
      const text = 'a = <<-EOF\n    if [ -f x ]; then {}\n  EOF\n';
      const e = await atOffset(page, text, text.indexOf('{}') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('a = <<-EOF\n    if [ -f x ]; then {\n    }\n  EOF\n');
    });
  });

  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('Enter after a colon in a block scalar keeps the indentation', async ({ page }) => {
      const text = 'script: |\n  run:\n';
      const e = await atOffset(page, text, text.indexOf('run:') + 4);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('script: |\n  run:\n  \n');
    });

    test('Enter after a dash line in a block scalar does not continue a list', async ({ page }) => {
      const text = 'a: |\n  - x\n';
      const e = await atOffset(page, text, text.indexOf('- x') + 3);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('a: |\n  - x\n  \n');
    });

    test('a ] typed in a block scalar is not moved', async ({ page }) => {
      const text = 'a: |\n  x [\n      \n';
      const e = await atOffset(page, text, text.indexOf('      ') + 6);
      await page.keyboard.type(']');
      expect(await e.value()).toBe('a: |\n  x [\n      ]\n');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('Enter between tags inside a comment is a plain line break', async ({ page }) => {
      const text = '<!-- <a></a> -->';
      const e = await atOffset(page, text, text.indexOf('</a>'));
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('<!-- <a>\n</a> -->');
    });
  });
});
