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

  test('a one-item list is a warning, because XML reads it back as a scalar', async ({ page }) => {
    const e = editor(page);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await e.paste(`${await e.value()}self:\n  allow:\n    - 127.0.0.1/32\n`);
    const warning = page.locator('.config-editor__diagnostic-marker-warning');
    await expect(warning).toHaveCount(1);
    await expect(warning).toHaveAttribute('data-message', /XML cannot keep this value exactly/);
    // Two items form a real XML array, so the value converts exactly.
    await e.paste(`${await e.value()}`.replace('    - 127.0.0.1/32\n', '    - 127.0.0.1/32\n    - 10.0.0.0/8\n'));
    await expect(warning).toHaveCount(0);
    await expect.poll(() => saved(page)).toContain('<allow>10.0.0.0/8</allow>');
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

test.describe('the edit-as picker', () => {
  setupEditor(test, '?language=yaml');

  test('switches the editing language and back, keeping the stored language', async ({ page }) => {
    await page.locator('.demo__edit-select').selectOption('toml');
    await page.waitForURL((url) => url.search.includes('edit=toml'));
    await page.locator('.config-editor__input').waitFor();
    expect(page.url()).toContain('language=yaml');
    expect(await editor(page).value()).toContain('listen = 7835');
    await page.locator('.demo__edit-select').selectOption('');
    await page.waitForURL((url) => !url.search.includes('edit='));
    await page.locator('.config-editor__input').waitFor();
    expect(await editor(page).value()).toContain('listen: 7835');
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
    expect(xml).toContain('<burrow config-root="true">');
  });
});
