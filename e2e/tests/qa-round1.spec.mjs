import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Regressions found by the first hands-on QA round, replayed with real keys.

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

test.describe('json: suggestions', () => {
  setupEditor(test, '?language=json');

  test('Enter on an inserted default keeps it and starts a new line', async ({ page }) => {
    const e = await at(page, '{\n  "tcp": {\n    |\n  }\n}');
    await page.keyboard.type('keep');
    await expect.poll(() => e.keys()).toContain('keepalive_secs');
    await page.keyboard.press('Enter');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.press('Enter');
    const text = await e.value();
    expect(text).toContain('"keepalive_secs": 30\n');
    expect(text).not.toContain('3030');
  });

  test('Tab on an inserted default steps past it', async ({ page }) => {
    const e = await at(page, '{\n  "tcp": {\n    |\n  }\n}');
    await page.keyboard.type('keep');
    await expect.poll(() => e.keys()).toContain('keepalive_secs');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Tab');
    expect(await e.value()).toContain('"keepalive_secs": 30\n');
  });

  test('Enter after typing a whole enum value inside quotes does not split it', async ({ page }) => {
    const e = await at(page, '{\n  "services": {\n    "ssh": {\n      "target": 22,\n      |\n    }\n  }\n}');
    await page.keyboard.type('mode');
    await expect.poll(() => e.keys()).toContain('mode');
    await page.keyboard.press('Enter');
    await page.keyboard.type('balance');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"mode": "balance"\n');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a new key after a value without a comma puts the comma after the value', async ({ page }) => {
    const e = await at(page, '{\n  "listen": 7835|\n}');
    await page.keyboard.press('Enter');
    await page.keyboard.type('tcp');
    await expect.poll(() => e.keys()).toContain('tcp');
    await page.keyboard.press('Enter');
    expect(await e.value()).toContain('"listen": 7835,\n  "tcp": {');
  });
});

test.describe('yaml: typing your own value', () => {
  setupEditor(test, '?language=yaml');

  test('Enter keeps a typed value instead of swapping in an example', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('public_address: box');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('public_address: box\n');
  });
});

test.describe('toml: a table before another table', () => {
  setupEditor(test, '?language=toml');

  test('accepting a table then a key keeps them on separate lines', async ({ page }) => {
    const e = await at(page, 'listen = 1\n|\n[tcp]\nnodelay = true\n');
    await page.keyboard.type('pool');
    await expect.poll(() => e.keys()).toContain('pool');
    await page.keyboard.press('Enter');
    await expect.poll(() => e.keys()).toContain('max_idle');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Tab');
    expect(await e.value()).toContain('[pool]\nmax_idle = 64');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});

test.describe('xml: child names typed inside an element', () => {
  setupEditor(test, '?language=xml');

  test('typing in an empty object element filters its children', async ({ page }) => {
    const e = await at(page, '|');
    await page.keyboard.type('<web');
    await expect.poll(() => e.keys()).toContain('web');
    await page.keyboard.press('Enter');
    await page.keyboard.type('certif');
    await expect.poll(() => e.keys()).toContain('certificates');
  });
});

test.describe('hcl: a single block for a list', () => {
  setupEditor(test, '?language=hcl');

  test('is a valid one-item list', async ({ page }) => {
    const e = editor(page);
    await e.paste('web {\n  certificates {\n    hosts = ["a"]\n    cert = "c"\n    key = "k"\n  }\n}\n');
    await page.waitForTimeout(400);
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});

test.describe('yaml: editing conventions', () => {
  setupEditor(test, '?language=yaml');

  test('Tab inserts spaces up to the next tab stop', async ({ page }) => {
    const e = await at(page, 'a:\n  b: 1\nabc|');
    await page.keyboard.press('Tab');
    expect(await e.value()).toBe('a:\n  b: 1\nabc ');
  });

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

  test('a line cut with nothing selected pastes above the caret line', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nb: |2\nc: 3');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1\nc: 3');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Control+v');
    expect(await e.value()).toBe('a: 1\nb: 2\nc: 3');
  });

  test('cutting the last line leaves no empty line', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nb: |2');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1');
  });
});

test.describe('edit as: structural edits keep host comments', () => {
  setupEditor(test, '?language=yaml&edit=json');

  test('adding a key while editing as JSON keeps the YAML comment', async ({ page }) => {
    const e = editor(page);
    const text = await e.value();
    await e.paste(text.replace('{\n', '{\n  "heartbeat": {"interval_secs": 5},\n'));
    await expect
      .poll(() => page.locator('.demo__output-text').textContent())
      .toContain('heartbeat');
    const saved = await page.locator('.demo__output-text').textContent();
    expect(saved).toContain('# Tunnel server configuration');
  });
});

test.describe('a phone-width footer', () => {
  test.use({ viewport: { width: 390, height: 800 } });
  setupEditor(test, '?language=yaml');

  test('keeps the line count visible with a long error', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: true\nbogus_key_with_a_very_long_name: 1\nanother_unknown_key: 2\n');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
    const count = page.locator('.config-editor__line-count');
    const box = await count.boundingBox();
    expect(box.x + box.width).toBeLessThanOrEqual(390);
  });
});
