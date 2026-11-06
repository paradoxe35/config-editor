import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Features deferred from the second QA round, replayed with real keys.

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

test.describe('yaml: Tab through an accepted block', () => {
  setupEditor(test, '?language=yaml');

  test('Tab moves from one required value to the next', async ({ page }) => {
    const e = await at(page, 'web:\n  letsencrypt:\n    email: a@b.c\n    dns:\n      - domains: [a.example]\n        |\n');
    await page.keyboard.type('rfc');
    await expect.poll(() => e.keys()).toContain('rfc2136');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Escape');
    await page.keyboard.type('ns1.example');
    await page.keyboard.press('Tab');
    await page.keyboard.type('key1');
    const text = await e.value();
    expect(text).toContain('server: ns1.example');
    expect(text).toMatch(/: key1/);
    expect(text).not.toContain('ns1.example  ');
  });
});

test.describe('toml: one-line objects', () => {
  setupEditor(test, '?language=toml');

  test('a key after a value without a comma gets the comma', async ({ page }) => {
    const e = await at(page, 'pool = { min_idle = 4 | }\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('max_idle');
    await e.clickKey('max_idle');
    expect(await e.value()).toMatch(/^pool = \{ min_idle = 4, max_idle = /);
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

test.describe('yaml: every problem in an optional object', () => {
  setupEditor(test, '?language=yaml');

  test('a typo is reported next to the other error', async ({ page }) => {
    const e = editor(page);
    await e.paste('services:\n  web:\n    target: 80\n    health_check: {interval_secs: 0, bogus: 1}\n');
    const messages = page.locator('.config-editor__diagnostic-marker');
    await expect(messages).toHaveCount(2);
    const all = await messages.evaluateAll((markers) => markers.map((marker) => marker.dataset.message).join('\n'));
    expect(all).toContain("'bogus' is not allowed");
    expect(all).toContain('minimum');
  });
});

test.describe('hcl: a space after a value', () => {
  setupEditor(test, '?language=hcl');

  test('ends the value, so Enter starts a new line', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('insecure_plaintext = false ');
    await page.keyboard.press('Enter');
    expect(await e.value()).not.toContain('false true');
    expect((await e.caret()).line).toBe(2);
  });

  test('completing a block name keeps its body', async ({ page }) => {
    const e = await at(page, 'pol| {\n  max_idle = 1\n}\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('pool');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('pool {\n  max_idle = 1\n}\n');
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

  test('Escape collapses a selection', async ({ page }) => {
    const e = await at(page, 'listen: 7835|');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Escape');
    expect((await e.caret()).collapsed).toBe(true);
  });

  test('Ctrl+A closes an open list', async ({ page }) => {
    const e = await at(page, 'listen: 1\npu|');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Control+a');
    await expect.poll(() => e.isOpen()).toBe(false);
  });

  test('a line number selects its line', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\n');
    await page.locator('.config-editor__gutter-content > div[data-line="2"]').click();
    const selected = await e.input.evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
    expect(selected).toBe('b: 2\n');
  });

  test('an indentless list completes its items', async ({ page }) => {
    const e = await at(page, 'web:\n  certificates:\n  - |');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('cert');
  });
});

test.describe('xml: typed values and end tags', () => {
  setupEditor(test, '?language=xml');

  test('Enter before a pasted end tag splits the line', async ({ page }) => {
    const e = await at(page, '<r>\n  <a>text|</a>\n</r>');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('<r>\n  <a>text\n  </a>\n</r>');
  });

  test('a tag in a comment is not closed', async ({ page }) => {
    const e = await at(page, '<!-- <b| -->');
    await page.keyboard.type('>');
    expect(await e.value()).toBe('<!-- <b> -->');
  });
});

test.describe('toml: headers by bracket kind', () => {
  setupEditor(test, '?language=toml');

  test('[[ ]] offers arrays of tables and the list closes after accepting', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('[[web.');
    await expect.poll(() => e.keys()).toEqual(['certificates']);
    await page.keyboard.press('Enter');
    await expect.poll(() => e.isOpen()).toBe(false);
    expect(await e.value()).toBe('[[web.certificates]]');
  });
});
