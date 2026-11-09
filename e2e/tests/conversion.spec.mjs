import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// `edit_as`: the host stores one language (the demo's `?language=`), the user
// edits another (`?edit=`). The demo's "Saved value" panel shows exactly what
// `on_change` sends the host, so these tests read it to check the round trip.

const saved = (page) => page.locator('.demo__output-text').textContent();

// Replaces `from` with `to` in the document the way a user edits it.
async function replaceText(page, e, from, to) {
  await e.input.evaluate(
    (element, [find, replacement]) => {
      const start = element.value.indexOf(find);
      element.focus();
      element.setSelectionRange(start, start + find.length);
      element.setRangeText(replacement, start, start + find.length, 'end');
      element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: replacement }));
    },
    [from, to],
  );
}

test.describe('XML edited as HCL', () => {
  setupEditor(test, '?language=xml&edit=hcl');

  test('the XML document is shown as valid HCL and the host value is untouched', async ({ page }) => {
    const e = editor(page);
    const text = await e.value();
    expect(text).toContain('listen = 7835');
    expect(text).toContain('clients {');
    expect(text).not.toContain('<');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
    expect(await saved(page)).toContain('<listen>7835</listen>');
    await expect(page.locator('.config-editor__file')).toContainText('editing as HCL');
  });

  test('an HCL edit reaches the host as XML after a pause', async ({ page }) => {
    const e = editor(page);
    await replaceText(page, e, 'listen = 7835', 'listen = 9000');
    await expect.poll(() => saved(page)).toContain('<listen>9000</listen>');
    expect(await saved(page)).not.toContain('listen =');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('invalid HCL is never sent to the host', async ({ page }) => {
    const e = editor(page);
    await replaceText(page, e, 'listen = 7835', 'listen = = 9000');
    await page.waitForTimeout(600);
    expect(await saved(page)).toContain('<listen>7835</listen>');
    await expect.poll(() => e.diagnosticCount()).toBeGreaterThan(0);
  });

  test('typing completes schema keys in the editing language', async ({ page }) => {
    const e = editor(page);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    // The sample has no `heartbeat` yet (present keys are not offered again).
    await page.keyboard.type('heart');
    await expect.poll(() => e.keys()).toContain('heartbeat');
  });
});

test.describe('values the host language cannot carry', () => {
  setupEditor(test, '?language=xml&edit=yaml');

  test('a one-item list saves as one element, because the schema says it is a list', async ({ page }) => {
    const e = editor(page);
    await e.paste(`${await e.value()}self:\n  allow:\n    - 127.0.0.1/32\n`);
    await expect.poll(() => saved(page)).toContain('<allow>127.0.0.1/32</allow>');
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
  });

  test('an empty list is a warning, because XML cannot write one', async ({ page }) => {
    const e = editor(page);
    await e.paste(`${await e.value()}self:\n  allow: []\n`);
    const warning = page.locator('.config-editor__diagnostic-marker-warning');
    await expect(warning).toHaveCount(1);
    await expect(warning).toHaveAttribute('data-message', /XML cannot keep this value exactly/);
  });
});

test.describe('a document the host language cannot express', () => {
  setupEditor(test, '?language=toml&edit=json');

  test('a null value is reported and not saved as TOML', async ({ page }) => {
    const e = editor(page);
    await replaceText(page, e, '"listen": 7835', '"listen": null');
    const marker = page.locator('.config-editor__diagnostic-marker', { hasText: '' });
    await expect
      .poll(() =>
        page
          .locator('.config-editor__diagnostic-marker')
          .evaluateAll((markers) => markers.map((m) => m.dataset.message).join('\n')),
      )
      .toContain('Not saved as TOML');
    expect(await marker.count()).toBeGreaterThan(0);
    expect(await saved(page)).toContain('listen = 7835');
  });
});

test.describe('switching the editing language live', () => {
  setupEditor(test, '?language=yaml');

  const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

  test('converts the open document in place without reloading the page', async ({ page }) => {
    const e = editor(page);
    await page.evaluate(() => {
      window.__stillHere = true;
    });
    await pick(page, 'toml');
    await expect.poll(() => e.value()).toContain('listen = 7835');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"listen": 7835');
    expect(await page.evaluate(() => window.__stillHere)).toBe(true);
    await expect(page.locator('.config-editor__file')).toContainText('editing as JSON');
    // The choice is remembered in the URL for reloads.
    await expect.poll(() => page.url()).toContain('edit=json');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });

  test('switching back without edits restores the original text, comments included', async ({ page }) => {
    const e = editor(page);
    const original = await e.value();
    expect(original).toContain('# Tunnel server configuration');
    await pick(page, 'hcl');
    await expect.poll(() => e.value()).toContain('listen = 7835');
    await pick(page, '');
    await expect.poll(() => e.value()).toBe(original);
  });

  test('unsaved edits are carried into the new language and back', async ({ page }) => {
    const e = editor(page);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('7835');
      element.focus();
      element.setSelectionRange(start, start + 4);
    });
    // Switch before the edit has been sent to the host.
    await page.keyboard.type('9000');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"listen": 9000');
    await pick(page, 'toml');
    await expect.poll(() => e.value()).toContain('listen = 9000');
    await pick(page, '');
    await expect.poll(() => e.value()).toContain('listen: 9000');
    // The host received the edit in its own language.
    await expect.poll(() => saved(page)).toContain('listen: 9000');
  });

  test('a document that does not parse stays put and says why', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: [\n');
    await pick(page, 'json');
    await expect.poll(() =>
      page
        .locator('.config-editor__diagnostic-marker')
        .evaluateAll((markers) => markers.map((marker) => marker.dataset.message).join('\n')),
    ).toContain('Cannot switch to JSON');
    expect(await e.value()).toBe('listen: [\n');
  });

  test('suggestions and highlighting follow the new language', async ({ page }) => {
    const e = editor(page);
    await pick(page, 'toml');
    await expect.poll(() => e.value()).toContain('listen = 7835');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('heart');
    await expect.poll(() => e.keys()).toContain('heartbeat');
    await page.keyboard.press('Enter');
    await expect.poll(() => e.value()).toContain('[heartbeat]');
    await expect(page.locator('.config-editor__code-line .config-editor__key').first()).toHaveText('listen');
  });
});

test.describe('the large config converted', () => {
  setupEditor(test, '?language=xml&edit=hcl&sample=large');

  test('loads valid and an edit keeps the XML config root and declaration', async ({ page }) => {
    const e = editor(page);
    expect((await e.value()).split('\n').length).toBeGreaterThan(1500);
    await expect.poll(() => e.diagnosticCount(), { timeout: 10000 }).toBe(0);
    await replaceText(page, e, 'interval_secs = 15', 'interval_secs = 20');
    await expect.poll(() => saved(page), { timeout: 10000 }).toContain('<interval_secs>20</interval_secs>');
    const xml = await saved(page);
    expect(xml.startsWith('<?xml version="1.0" encoding="UTF-8"?>')).toBe(true);
    expect(xml).toContain('<tunnel config-root="true">');
  });
});

// A value edit made through another language changes only that value in the
// stored document: comments, formatting, and key order are kept.
for (const [stored, editAs] of [
  ['hcl', 'yaml'],
  ['yaml', 'json'],
  ['toml', 'hcl'],
  ['json', 'toml'],
  ['xml', 'yaml'],
]) {
  test.describe(`large ${stored} edited as ${editAs}`, () => {
    setupEditor(test, `?language=${stored}&edit=${editAs}&sample=large`);

    test('changing one value changes exactly one line of the stored document', async ({ page }) => {
      const e = editor(page);
      const original = await saved(page);
      await expect.poll(() => e.value()).toContain('7835');
      await replaceText(page, e, '7835', '7999');
      await expect.poll(() => saved(page), { timeout: 10000 }).toContain('7999');
      const before = original.split('\n');
      const after = (await saved(page)).split('\n');
      expect(after.length).toBe(before.length);
      const changed = after.filter((line, index) => line !== before[index]);
      expect(changed).toHaveLength(1);
      expect(changed[0]).toContain('7999');
    });
  });
}

test.describe('a switch that cannot convert yet', () => {
  setupEditor(test, '?language=yaml');

  test('is reported on the blocking value and completes once it is fixed', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\npublic_address: null\n');
    await page.locator('.demo__edit-select').selectOption('toml');
    const marker = page.locator('.config-editor__diagnostic-marker', { hasText: 'null' });
    await expect(marker).toHaveAttribute('data-message', /Cannot switch to TOML yet/);
    expect(await e.value()).toContain('public_address: null');
    // Fixing the value lets the pending switch finish on its own.
    await replaceText(page, e, 'null', 'a.example');
    await expect.poll(() => e.value()).toContain('public_address = "a.example"');
    await expect.poll(() => e.diagnosticCount()).toBe(0);
  });
});
