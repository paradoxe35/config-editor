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
