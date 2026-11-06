import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Regressions found by the second hands-on QA round, replayed with real keys.

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

test.describe('yaml: editing', () => {
  setupEditor(test, '?language=yaml');

  test('Enter on an empty list item ends the list', async ({ page }) => {
    const e = await at(page, 'hosts:\n  - a|\n');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('hosts:\n  - a\n  \n');
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

  test('a whole-line cut keeps the caret column', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nbb|b: 2\ncccc: 3\n');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1\ncccc: 3\n');
    expect(await e.caret()).toMatchObject({ line: 2, column: 3 });
  });

  test('Ctrl+/ on a blank line starts a comment', async ({ page }) => {
    const e = await at(page, 'a: 1\n|');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('a: 1\n# ');
  });

  test('Home on a long scrolled line brings the caret back into view', async ({ page }) => {
    const e = await at(page, `key: ${'x'.repeat(400)}|`);
    await page.keyboard.press('End');
    await page.keyboard.press('Home');
    const scrollLeft = await e.input.evaluate((element) => element.scrollLeft);
    expect(scrollLeft).toBeLessThan(50);
  });

  test('the gutter stays above code scrolled sideways', async ({ page }) => {
    const e = await at(page, `key: ${'x'.repeat(400)}|`);
    await page.keyboard.press('End');
    const covered = await page.evaluate(() => {
      const gutter = document.querySelector('.config-editor__gutter').getBoundingClientRect();
      const top = document.elementFromPoint(gutter.left + gutter.width / 2, gutter.top + 12);
      return top.closest('.config-editor__gutter') !== null;
    });
    expect(covered).toBe(true);
  });
});

test.describe('json: editing', () => {
  setupEditor(test, '?language=json');

  test('Enter between brackets inside a string is a plain newline', async ({ page }) => {
    const e = await at(page, '{"p": "[|]"}');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{"p": "[\n]"}');
  });

  test('Backspace between a pair the editor did not insert deletes one character', async ({ page }) => {
    const e = await at(page, '{"a": [|]}');
    await page.keyboard.press('Backspace');
    expect(await e.value()).toBe('{"a": ]}');
  });

  test('nothing is suggested after the root object closes', async ({ page }) => {
    const e = await at(page, '{\n  "listen": 1\n}|');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
  });

  test('Enter after a value typed in suggested quotes steps past them', async ({ page }) => {
    const e = await at(page, '{\n  "self": {\n    |\n  }\n}');
    await page.keyboard.type('"al');
    await expect.poll(() => e.keys()).toContain('allow');
    await page.keyboard.press('Enter');
    await page.keyboard.type('10.0.0.0/8');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"allow": ["10.0.0.0/8"]\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('completing a key that has a value keeps the value', async ({ page }) => {
    const e = await at(page, '{\n  "publ|": "my.host"\n}');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('{\n  "public_address": "my.host"\n}');
  });
});

test.describe('toml: headers', () => {
  setupEditor(test, '?language=toml');

  test('a table header completes its segment with tables', async ({ page }) => {
    const e = await at(page, 'listen = 1\n|');
    await page.keyboard.type('[hea');
    await expect.poll(() => e.keys()).toEqual(['heartbeat']);
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('listen = 1\n[heartbeat]');
  });
});

test.describe('hcl: end of file', () => {
  setupEditor(test, '?language=hcl');

  test('a new line after the last attribute suggests keys', async ({ page }) => {
    const e = await at(page, 'listen = 7835\n|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('limits');
    expect(await e.keys()).not.toContain('7835');
  });
});

test.describe('xml: values and end tags', () => {
  setupEditor(test, '?language=xml');

  test('after an end tag the parent is completed', async ({ page }) => {
    const e = await at(page, '<pool>\n  <min_idle>4</min_idle>\n</pool>|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('limits');
    expect(await e.keys()).not.toContain('max_idle');
  });

  test('Enter after a typed value steps past the end tag', async ({ page }) => {
    // The end tag must be the editor's (auto-closed); a pasted one splits.
    const e = await at(page, '<services>\n  <web>\n    <target>80</target>\n    |\n  </web>\n</services>');
    await page.keyboard.type('<mode');
    await page.keyboard.press('Escape');
    await page.keyboard.type('>balance');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('<mode>balance</mode>\n    \n');
  });
});

const saved = (page) => page.locator('.demo__output-text').textContent();

test.describe('edit as XML keeps untouched host values', () => {
  setupEditor(test, '?language=yaml&edit=xml');

  test('changing one value leaves null and quoted values alone', async ({ page }) => {
    const e = editor(page);
    await page.locator('.demo__edit-select').selectOption('');
    await e.paste('public_address: null\ninsecure_plaintext: false\n');
    await page.locator('.demo__edit-select').selectOption('xml');
    await expect(e.input).toHaveValue(/<insecure_plaintext>false<\/insecure_plaintext>/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('>false<') + 1;
      element.focus();
      element.setSelectionRange(at, at + 5);
    });
    await page.keyboard.type('true');
    await expect.poll(() => saved(page)).toContain('insecure_plaintext: true');
    expect(await saved(page)).toContain('public_address: null');
  });
});

test.describe('edit as JSON keeps comments on structural edits', () => {
  setupEditor(test, '?language=yaml');

  test('a key added after a line with a trailing comment keeps every comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('# Burrow server configuration\nlisten: 7835 # main port\ninsecure_plaintext: false\n');
    await page.locator('.demo__edit-select').selectOption('json');
    await expect(e.input).toHaveValue(/"listen": 7835/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"listen": 7835,') + '"listen": 7835,'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"public_address": "x",');
    await expect.poll(() => saved(page)).toContain('public_address: x');
    const text = await saved(page);
    expect(text).toContain('# Burrow server configuration');
    expect(text).toContain('# main port');
  });
});

test.describe('the footer counts warnings apart from errors', () => {
  setupEditor(test, '?language=xml&edit=yaml');

  test('a value the XML host cannot keep is a warning, not an error', async ({ page }) => {
    const e = editor(page);
    await e.paste(`${await e.value()}self:\n  allow: []\n`);
    await expect(page.locator('.config-editor__diagnostic-marker-warning')).toHaveCount(1);
    await expect(page.locator('.config-editor__diagnostic-status')).toContainText('1 warning');
    await expect(page.locator('.config-editor__diagnostic-status')).not.toContainText('error');
  });
});

test.describe('tooltips on a phone', () => {
  test.use({ viewport: { width: 390, height: 800 } });
  setupEditor(test, '?language=yaml');

  test('a long message stays inside the editor', async ({ page }) => {
    const e = editor(page);
    await e.paste('heartbeat: {interval_secs: 99999}\n');
    const marker = page.locator('.config-editor__diagnostic-marker');
    await expect(marker).toHaveCount(1);
    await marker.hover();
    const overflow = await marker.evaluate((element) => {
      const tip = getComputedStyle(element, '::after');
      const right = element.getBoundingClientRect().left + parseFloat(tip.left) + parseFloat(tip.width);
      return right - document.querySelector('.config-editor__input').getBoundingClientRect().right;
    });
    expect(overflow).toBeLessThanOrEqual(0);
  });
});
