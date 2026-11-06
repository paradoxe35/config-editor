import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Hover docs for keys nested on one line (inline tables, flow mappings, one-line
// objects and elements), in every language.

const SECRET_DOC = '256-bit secret (base64 or hex)';
const MAX_DOC = 'Maximum concurrent forwarded connections';

// Hovers the first occurrence of `key` on 0-based `line` and returns the card.
async function hoverKey(page, line, key) {
  const text = (await editor(page).value()).split('\n')[line];
  const column = text.indexOf(key) + 1;
  const at = await pointAt(page, line, column);
  await page.mouse.move(at.x, at.y, { steps: 4 });
  return page.locator('.config-editor__hover');
}

async function expectDoc(page, line, key, doc) {
  const card = await hoverKey(page, line, key);
  await expect(card).toHaveCount(1);
  await expect(card.locator('.config-editor__hover-key')).toHaveText(key);
  await expect(card).toContainText(doc);
}

test.describe('TOML', () => {
  setupEditor(test, '?language=toml');

  test('keys inside an inline table show their docs', async ({ page }) => {
    await editor(page).paste('[clients]\nparadoxe = { secret = [], max_connections = 123 }\n');
    await expectDoc(page, 1, 'secret', SECRET_DOC);
    await page.mouse.move(0, 0);
    await expectDoc(page, 1, 'max_connections', MAX_DOC);
  });

  test('keys inside a nested inline table show their docs', async ({ page }) => {
    await editor(page).paste('clients = { acme = { secret = "x" } }\n');
    await expectDoc(page, 0, 'secret', SECRET_DOC);
  });

  test('keys under a table header show their docs', async ({ page }) => {
    await editor(page).paste('[clients.acme]\nsecret = "x"\n');
    await expectDoc(page, 1, 'secret', SECRET_DOC);
  });
});

test.describe('YAML', () => {
  setupEditor(test);

  test('keys inside a flow mapping show their docs', async ({ page }) => {
    await editor(page).paste('clients:\n  acme: { secret: x, max_connections: 3 }\n');
    await expectDoc(page, 1, 'secret', SECRET_DOC);
    await page.mouse.move(0, 0);
    await expectDoc(page, 1, 'max_connections', MAX_DOC);
  });
});

test.describe('JSON', () => {
  setupEditor(test, '?language=json');

  test('keys inside a one-line object show their docs', async ({ page }) => {
    await editor(page).paste('{\n  "clients": { "acme": { "secret": "x", "max_connections": 3 } }\n}\n');
    await expectDoc(page, 1, 'secret', SECRET_DOC);
    await page.mouse.move(0, 0);
    await expectDoc(page, 1, 'max_connections', MAX_DOC);
  });
});

test.describe('HCL', () => {
  setupEditor(test, '?language=hcl');

  test('keys inside a value map show their docs', async ({ page }) => {
    await editor(page).paste('clients = { acme = { secret = "x", max_connections = 3 } }\n');
    await expectDoc(page, 0, 'secret', SECRET_DOC);
    await page.mouse.move(0, 0);
    await expectDoc(page, 0, 'max_connections', MAX_DOC);
  });
});

test.describe('XML', () => {
  setupEditor(test, '?language=xml');

  test('elements and attributes nested on one line show their docs', async ({ page }) => {
    await editor(page).paste('<clients><acme max_connections="3"><secret>x</secret></acme></clients>\n');
    await expectDoc(page, 0, 'secret', SECRET_DOC);
    await page.mouse.move(0, 0);
    await expectDoc(page, 0, 'max_connections', MAX_DOC);
  });
});
