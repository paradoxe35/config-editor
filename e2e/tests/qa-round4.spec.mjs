import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Leftovers from earlier rounds and fourth-round findings, replayed with real keys.

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

test.describe('yaml: commands keep a backward selection backward', () => {
  setupEditor(test, '?language=yaml');

  test('Tab indents and Shift+Up keeps extending from the start', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\nc: 3|\n');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Tab');
    expect(await e.value()).toBe('  a: 1\n  b: 2\n  c: 3\n');
    expect((await selection(e)).direction).toBe('backward');
  });

  test('Ctrl+/ comments and keeps the direction', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2|\n');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('# a: 1\n# b: 2\n');
    expect((await selection(e)).direction).toBe('backward');
  });
});

test.describe('xml: very deep nesting', () => {
  setupEditor(test, '?language=xml');

  test('is reported as an error and the editor keeps working', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    const e = editor(page);
    await e.paste(`${'<a>'.repeat(20000)}${'</a>'.repeat(20000)}`);
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    expect(await page.locator('.config-editor__diagnostic-marker').getAttribute('data-message')).toContain('deeper than 128');
    await e.paste('<listen>1</listen>\n');
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});

test.describe('a page stylesheet overrides theme colors', () => {
  setupEditor(test, '?language=yaml');

  test('a plain .config-editor rule beats every bundled theme', async ({ page }) => {
    await page.addStyleTag({ content: '.config-editor { --config-editor-bg: rgb(200, 0, 0); }' });
    const background = () =>
      page.locator('.config-editor').first().evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
    expect(await background()).toBe('rgb(200, 0, 0)');
    await page.locator('.demo__theme-select').selectOption('nord');
    await expect(page.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
    expect(await background()).toBe('rgb(200, 0, 0)');
  });

  test('themes still differ without a page rule', async ({ page }) => {
    const background = () =>
      page.locator('.config-editor').first().evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
    const dark = await background();
    await page.locator('.demo__theme-select').selectOption('nord');
    await expect(page.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
    expect(await background()).not.toBe(dark);
    expect(await background()).toBe('#2e3440');
  });
});

// --- Fourth-round findings: editing ---

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

test.describe('typed closers ignore brackets in strings and comments', () => {
  setupEditor(test, '?language=hcl');

  test('a brace in a comment is not an opener', async ({ page }) => {
    const e = await at(page, 'resource "x" "y" {\n  # TODO: {\n  a = 1\n  |');
    await page.keyboard.type('}');
    expect(await e.value()).toBe('resource "x" "y" {\n  # TODO: {\n  a = 1\n}');
  });
});

test.describe('keys keep their meaning with the suggestion list open', () => {
  setupEditor(test, '?language=yaml');

  test('Shift+Up selects instead of moving in the list', async ({ page }) => {
    const e = await at(page, 'listen: 1\n|');
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Shift+ArrowUp');
    const selected = await e.input.evaluate((element) => element.selectionStart !== element.selectionEnd);
    expect(selected).toBe(true);
  });

  test('Ctrl+Enter opens a line below instead of accepting', async ({ page }) => {
    const e = await at(page, 'listen: 1\n|');
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Control+Enter');
    expect(await e.value()).toBe('listen: 1\npub\n');
  });
});

test.describe('read-only: Escape collapses a selection', () => {
  setupEditor(test, '?language=yaml&readonly');

  test('after selecting a line by its number', async ({ page }) => {
    const e = editor(page);
    await page.locator('.config-editor__gutter-content > div[data-line="2"]').click();
    expect((await e.caret()).collapsed).toBe(false);
    await page.keyboard.press('Escape');
    expect((await e.caret()).collapsed).toBe(true);
  });
});

test.describe('keys the editor handles keep the caret in view', () => {
  setupEditor(test, '?language=yaml');

  test('a line number click does not move the view', async ({ page }) => {
    const e = editor(page);
    const lines = Array.from({ length: 400 }, (_, index) => `key${index}: ${index}`);
    await e.paste(`${lines.join('\n')}\n`);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, 0);
      element.blur();
      element.scrollTop = 4000;
    });
    await page.waitForTimeout(100);
    const row = page.locator('.config-editor__gutter-content > div[data-line]').nth(45);
    const line = Number(await row.getAttribute('data-line'));
    const before = await e.input.evaluate((element) => element.scrollTop);
    await row.click();
    const after = await e.input.evaluate((element) => element.scrollTop);
    expect(Math.abs(after - before)).toBeLessThan(2);
    const selected = await e.input.evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
    expect(selected).toBe(`key${line - 1}: ${line - 1}\n`);
  });
});

// --- Fourth-round findings: completion ---

test.describe('json: Enter after a value typed in suggested quotes, before a comma', () => {
  setupEditor(test, '?language=json');

  test('takes the comma along to the end of the line', async ({ page }) => {
    const e = await at(page, '{\n  |\n  "listen": 1\n}');
    await page.keyboard.type('"pub');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    await page.keyboard.type('foo');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "public_address": "foo",\n  \n  "listen": 1\n}');
  });
});

test.describe('xml: Enter after a value accepted from the list', () => {
  setupEditor(test, '?language=xml');

  test('steps past the end tag the editor wrote', async ({ page }) => {
    const e = await at(page, '<listen>1</listen>\n|');
    await page.keyboard.type('<insec');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    await page.keyboard.press('Enter');
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toContain('true');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('<insecure_plaintext>true</insecure_plaintext>\n');
  });
});

test.describe('Tab walks the empty values of a one-line insert', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test('inline table', async ({ page }) => {
      const e = await at(page, '[[web.letsencrypt.dns]]\n|');
      await page.keyboard.type('rfc');
      await expect.poll(() => e.keys()).toContain('rfc2136');
      await page.keyboard.press('Enter');
      await page.keyboard.type('a');
      await page.keyboard.press('Tab');
      await page.keyboard.type('b');
      expect(await e.value()).toContain('server = "a", key_name = "b"');
    });
  });
});

test.describe('renaming a key with Enter closes the list', () => {
  setupEditor(test, '?language=yaml');

  test('the next Enter does not split the key', async ({ page }) => {
    const e = await at(page, 'web:\n  htt|: 80\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('https');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('https'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('web:\n  https: 80\n');
    await expect.poll(() => e.isOpen()).toBe(false);
  });
});

test.describe('toml: a header with a typed prefix', () => {
  setupEditor(test, '?language=toml');

  test('leaves out tables that already exist', async ({ page }) => {
    const e = await at(page, '[web]\nhttp = 1\n\n[w|]');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('websocket');
    expect(await e.keys()).not.toContain('web');
  });
});

test.describe('edit as JSON, back to the host language during the typing pause', () => {
  setupEditor(test, '?language=yaml&edit=json');

  test('saves the edit', async ({ page }) => {
    const e = editor(page);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835') + 4;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('1');
    await page.locator('.demo__edit-select').selectOption('');
    await expect.poll(() => page.locator('.demo__output-text').textContent()).toContain('listen: 78351');
    await expect(e.input).toHaveValue(/listen: 78351/);
  });
});

// --- Fourth-round findings: Edit-as saving and diagnostics ---

const savedText = (page) => page.locator('.demo__output-text').textContent();

test.describe('edit a TOML host as YAML: deleting a table', () => {
  setupEditor(test, '?language=toml');

  test('removes only that table and its comment', async ({ page }) => {
    const e = editor(page);
    const host = '# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = "s1" # the secret\n\n# beta client\n[clients.beta]\nsecret = "s2"\n';
    await e.paste(host);
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/beta:/);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('  beta:');
      const end = element.value.indexOf('s2', start) + 3;
      element.focus();
      element.setSelectionRange(start, end);
    });
    await page.keyboard.press('Delete');
    await expect.poll(() => savedText(page)).not.toContain('beta');
    expect(await savedText(page)).toBe('# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = "s1" # the secret\n');
  });
});

test.describe('edit a YAML host as JSON: adding to a flow map', () => {
  setupEditor(test, '?language=yaml');

  test('rewrites only that map, on its line', async ({ page }) => {
    const e = editor(page);
    await e.paste('# head\ntcp: {nodelay: true} # tcp\nlisten: 1 # port\n');
    await page.locator('.demo__edit-select').selectOption('json');
    await expect(e.input).toHaveValue(/"nodelay": true/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"nodelay": true') + '"nodelay": true'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type(', "keepalive_secs": 3');
    await expect.poll(() => savedText(page)).toContain('keepalive_secs');
    expect(await savedText(page)).toBe('# head\ntcp: {nodelay: true, keepalive_secs: 3} # tcp\nlisten: 1 # port\n');
  });
});

test.describe('yaml: a wrong-type value written as a block', () => {
  setupEditor(test, '?language=yaml');

  test('underlines its key, not an empty spot after the colon', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen:\n  port: 1\n');
    const marker = page.locator('.config-editor__diagnostic-marker');
    await expect(marker).toHaveCount(1);
    expect(await marker.textContent()).toBe('listen');
  });
});

test.describe('a TOML date is text', () => {
  setupEditor(test, '?language=toml');

  test('no internal key leaks into messages or the YAML view', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen = 1\npublic_address = 1979-05-27\n');
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/public_address: "?1979-05-27"?/);
    expect(await e.value()).not.toContain('toml_private');
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

test.describe('every key the editor handles reveals the caret', () => {
  setupEditor(test, '?language=yaml');

  const lines = ['root:', ...Array.from({ length: 700 }, (_, index) => `  key${index}: ${index}`)];
  const caretInView = (e) =>
    e.input.evaluate((element) => {
      const line = element.value.slice(0, element.selectionStart).split('\n').length;
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
      const top = (line - 1) * lineHeight;
      return top >= element.scrollTop - lineHeight && top <= element.scrollTop + element.clientHeight;
    });

  for (const [name, offsetInLine, key] of [
    ['Ctrl+/', 4, 'Control+/'],
    ['Shift+Tab', 4, 'Shift+Tab'],
    ['smart Home', 12, 'Home'],
    ['Backspace in indentation', 2, 'Backspace'],
    ['an opening bracket', 13, '['],
  ]) {
    test(name, async ({ page }) => {
      const e = editor(page);
      await e.paste(`${lines.join('\n')}\n`);
      await e.input.evaluate(
        (element, offset) => {
          const at = element.value.indexOf('  key600: ') + offset;
          element.focus();
          element.setSelectionRange(at, at);
          element.scrollTop = 0;
        },
        offsetInLine,
      );
      await page.keyboard.press(key);
      expect(await caretInView(e)).toBe(true);
    });
  }
});

test.describe('yaml: Tab walks the empty values of a flow map', () => {
  setupEditor(test, '?language=yaml');

  test('accepted into a one-line map', async ({ page }) => {
    const e = await at(page, 'websocket: {listen: 1, |}\n');
    await page.keyboard.type('tls');
    await expect.poll(() => e.keys()).toContain('tls');
    await page.keyboard.press('Enter');
    await page.keyboard.type('a');
    await page.keyboard.press('Tab');
    await page.keyboard.type('b');
    expect(await e.value()).toMatch(/cert: a, key: b/);
  });
});

test.describe('a YAML host viewed as XML: values XML cannot show', () => {
  setupEditor(test, '?language=yaml');

  test('are flagged as warnings instead of silently changed', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 1\nx:\n  - [1, 2]\n  - [3]\nallow: []\n');
    await page.locator('.demo__edit-select').selectOption('xml');
    await expect(e.input).toHaveValue(/<listen>1<\/listen>/);
    const warnings = page.locator('.config-editor__diagnostic-marker-warning');
    await expect(warnings).toHaveCount(2);
    const messages = await warnings.evaluateAll((all) => all.map((marker) => marker.dataset.message).join('\n'));
    expect(messages).toContain('/x');
    expect(messages).toContain('/allow');
    expect(messages).toContain('cannot show this value exactly');
  });
});
