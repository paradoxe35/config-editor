import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Diagnostics found in documents: schema problems, typos, wrong types, deep nesting.

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

const markers = (page) =>
  page.locator('.config-editor__diagnostic-marker').evaluateAll((all) => all.map((marker) => marker.dataset.message));

test.describe('hcl: a single block for a list', () => {
  setupEditor(test, '?language=hcl');

  test('is a valid one-item list', async ({ page }) => {
    const e = editor(page);
    await e.paste('web {\n  certificates {\n    hosts = ["a"]\n    cert = "c"\n    key = "k"\n  }\n}\n');
    await page.waitForTimeout(400);
    await expect.poll(() => e.diagnosticCount()).toBe(0);
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
    expect(all).toContain('"bogus" is not allowed');
    expect(all).toContain('minimum');
  });
});

test.describe('diagnostics', () => {
  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('F8 visits problems top to bottom', async ({ page }) => {
      const e = await at(page, '|insecure_plaintext: maybe\nbogus: 1\nheartbeat: {interval_secs: -1}\n');
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(3);
      const lines = [];
      for (let index = 0; index < 3; index += 1) {
        await page.keyboard.press('F8');
        lines.push((await e.caret()).line);
      }
      expect(lines).toEqual([1, 2, 3]);
    });
  });

  test.describe('json', () => {
    setupEditor(test, '?language=json');

    test('Enter at column 0 of an indented closer keeps one indent', async ({ page }) => {
      const e = await at(page, '{\n  "pool": {\n    "min_idle": 1\n|  }\n}');
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('{\n  "pool": {\n    "min_idle": 1\n\n  }\n}');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('a number in a string field is a string', async ({ page }) => {
      const e = editor(page);
      await e.paste('<public_address>123</public_address>\n');
      await page.waitForTimeout(400);
      expect(await markers(page)).toEqual([]);
    });

    test('an empty element name is a syntax error', async ({ page }) => {
      const e = editor(page);
      await e.paste('<listen>1</listen>\n<></>\n');
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    });
  });

  test.describe('hcl edited as YAML', () => {
    setupEditor(test, '?language=hcl&edit=yaml');

    test('a template sequence in a value is escaped in the host', async ({ page }) => {
      const e = editor(page);
      await e.paste('public_address: "${host}"\n');
      await expect.poll(() => page.locator('.demo__output-text').textContent()).toContain('"$${host}"');
    });
  });
});

test.describe('xml: very deep nesting', () => {
  setupEditor(test, '?language=xml');

  test('is reported as an error and the editor keeps working', async ({ page }) => {
    // 40,000 tags pasted and read: a long test, not a speed claim.
    test.slow();
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    const e = editor(page);
    await e.paste(`${'<a>'.repeat(20000)}${'</a>'.repeat(20000)}`);
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    const marker = page.locator('.config-editor__diagnostic-marker');
    expect(await marker.getAttribute('data-message')).toBe(
      'Elements nest too deeply here, each inside the one before: the editor reads no deeper',
    );
    // Placed where it goes too deep (the 129th tag), not at the line's start.
    expect(await marker.evaluate((element) => parseFloat(element.style.left))).toBeGreaterThan(100);
    await e.paste('<listen>1</listen>\n');
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
    expect(errors).toEqual([]);
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
