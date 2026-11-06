import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Completion insert formatting, per language, driven by real keystrokes:
// accepting a nested object right after a container opened on the same line
// must come out formatted the way that language is written, and inline-only
// containers (TOML inline tables, YAML flow mappings) must stay one line.

// Pastes `marked`, puts the caret at its `|`, and types `text`.
async function typeAt(page, e, marked, text) {
  const at = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, position) => {
    element.focus();
    element.setSelectionRange(position, position);
  }, at);
  await page.keyboard.type(text);
}

// Accepts `label` from the open list with Enter (the user's flow).
async function acceptWithEnter(page, e, label) {
  await expect.poll(() => e.keys()).toContain(label);
  await expect.poll(() => e.highlighted()).toEqual([label]);
  await page.keyboard.press('Enter');
  await page.waitForTimeout(120);
}

test.describe('JSON', () => {
  setupEditor(test, '?language=json');

  test('a nested object accepted after an inline brace is formatted as a block', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, '{\n  "web": {|}\n}', '"letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue(
      '{\n  "web": {\n    "letsencrypt": {\n      "email": ""\n    }\n  }\n}',
    );
    // The caret waits on the first value to fill.
    expect(await e.caret()).toMatchObject({ line: 4, column: 17 });
    await page.keyboard.type('ops@example.com');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a scalar accepted after an inline brace stays inline', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, '{\n  "web": {|}\n}', '"http2');
    await acceptWithEnter(page, e, 'http2');
    expect(await e.value()).toBe('{\n  "web": {"http2": }\n}');
  });

  test('a nested object on its own line is indented from that line', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, '{\n  "web": {\n    |\n  }\n}', '"letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue(
      '{\n  "web": {\n    "letsencrypt": {\n      "email": ""\n    }\n  }\n}',
    );
  });
});

test.describe('HCL', () => {
  setupEditor(test, '?language=hcl');

  test('a nested object accepted inside an inline map is formatted as a block', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, 'web = {|}\n', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('web = {\n  letsencrypt = {\n    email = ""\n  }\n}\n');
    expect(await e.caret()).toMatchObject({ line: 3, column: 14 });
    await page.keyboard.type('ops@example.com');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('a nested object inside a block body is indented from its line', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, 'web {\n  |\n}\n', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('web {\n  letsencrypt {\n    email = ""\n  }\n}\n');
  });
});

test.describe('TOML', () => {
  setupEditor(test, '?language=toml');

  test('an object accepted inside an inline table stays a valid inline table', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, 'web = { | }\n', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('web = { letsencrypt = { email = "" } }\n');
    await page.keyboard.type('ops@example.com');
    await expect.poll(() => e.value()).toContain('email = "ops@example.com"');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('an object accepted under a table header opens the nested table path', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, '[web]\nhttp = 80\n|', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('[web]\nhttp = 80\n[web.letsencrypt]\nemail = ""\n');
  });
});

test.describe('YAML', () => {
  setupEditor(test);

  test('an object accepted inside a flow mapping stays in flow style', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, 'web: { | }\n', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('web: { letsencrypt: { email:  } }\n');
    await page.keyboard.type('ops@example.com');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('an object accepted in a block mapping still expands to a block', async ({ page }) => {
    const e = editor(page);
    await typeAt(page, e, 'web:\n  |', 'letsenc');
    await acceptWithEnter(page, e, 'letsencrypt');
    await expect(e.input).toHaveValue('web:\n  letsencrypt:\n    email: \n');
  });
});
