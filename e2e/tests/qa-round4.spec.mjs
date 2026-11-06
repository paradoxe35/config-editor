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
