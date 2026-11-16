import { expect, test } from '@playwright/test';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// P1 and P4: the host gives `readOnlyRanges` for the text it just heard,
// right after an edit that wrote text above the lock. The editor locks
// only those ranges, where they are: no stray copy moved again by the edit
// before (which refused typing on the line below).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const example = (name) => pathToFileURL(join(root, 'js', 'examples', name)).href;
const START = `name: web\nid: srv-42\nnote: ${'x'.repeat(30)}\n`;

test('ranges given right after an edit above them lock only their own text', async ({ page }) => {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (start) => {
    window.editor?.destroy();
    const module = await import('../config-editor/index.js');
    const ranges = (text) => {
      const id = text.indexOf('id: ');
      return id < 0 ? [] : [{ start: id, end: text.indexOf('\n', id) }];
    };
    const element = document.createElement('div');
    document.body.prepend(element);
    const editor = new module.ConfigEditor(element, {
      value: start,
      echoes: true,
      readOnlyRanges: ranges(start),
      // The host echoes each text with the ranges of that text.
      onChange: (text) => setTimeout(() => editor.update({ value: text, readOnlyRanges: ranges(text) }), 0),
    });
    window.shown = () => editor.value;
  }, START);
  const input = page.locator('.config-editor__input');
  await input.focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.insertText('# a long comment line here\n');
  await page.waitForTimeout(800);
  const typedAt = async (at) => {
    const before = await page.evaluate(() => window.shown());
    await page.evaluate((at) => document.querySelector('.config-editor__input').setSelectionRange(at, at), at);
    await page.keyboard.type('Q');
    await page.waitForTimeout(50);
    const after = await page.evaluate(() => window.shown());
    return after !== before;
  };
  const text = await page.evaluate(() => window.shown());
  const id = text.indexOf('srv-42');
  const note = text.indexOf('xxxx');
  // Inside the locked `id` value: refused.
  expect(await typedAt(id + 2)).toBe(false);
  // In the `note` value below it: taken.
  expect(await typedAt(note + 2)).toBe(true);
  // Every Backspace at the end of the `note` value is taken.
  await page.keyboard.press('Control+End');
  await page.keyboard.press('ArrowLeft');
  for (let index = 0; index < 10; index += 1) await page.keyboard.press('Backspace');
  await expect.poll(() => page.evaluate(() => (window.shown().match(/x/g) || []).length)).toBe(30 - 10 + 0);
});
