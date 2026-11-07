import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Go to line (Ctrl+G) with real keys: `line` or `line:column`, in view,
// Escape cancels, read-only editors too, phone widths.

const field = (page) => page.getByRole('textbox', { name: 'Go to line' });

// Whether the caret's line is inside the textarea's visible area.
const caretInView = (e) =>
  e.input.evaluate((element) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    const line = element.value.slice(0, element.selectionStart).split('\n').length - 1;
    const top = parseFloat(style.paddingTop) + line * lineHeight;
    return top >= element.scrollTop && top + lineHeight <= element.scrollTop + element.clientHeight;
  });

test.describe('go to line', () => {
  setupEditor(test, '?sample=large');

  test('Ctrl+G moves the caret to a line far down and scrolls it into view', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+g');
    await expect(field(page)).toBeFocused();
    await expect(page.locator('.config-editor__goto-hint')).toContainText('Line 1–');
    await page.keyboard.type('1500');
    await page.keyboard.press('Enter');
    await expect(field(page)).toBeHidden();
    await expect(e.input).toBeFocused();
    expect(await e.caret()).toEqual({ line: 1500, column: 1, collapsed: true });
    expect(await caretInView(e)).toBe(true);
    await expect(page.locator('.config-editor__code-line[data-line="1500"]')).toHaveClass(/code-line-active/);
    // Typing goes on at the new place.
    await page.keyboard.type('#');
    const lines = (await e.value()).split('\n');
    expect(lines[1499].startsWith('#')).toBe(true);
  });

  test('line:column places the caret on the column; past the end is clamped', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+g');
    await page.keyboard.type('3:4');
    await page.keyboard.press('Enter');
    expect(await e.caret()).toMatchObject({ line: 3, column: 4 });

    const total = (await e.value()).split('\n').length;
    await page.keyboard.press('Control+g');
    await page.keyboard.type('999999');
    await page.keyboard.press('Enter');
    expect((await e.caret()).line).toBe(total);
    expect(await caretInView(e)).toBe(true);
  });

  test('Escape cancels and leaves the caret where it was', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await e.input.evaluate((element) => element.setSelectionRange(5, 5));
    await page.keyboard.press('Control+g');
    await page.keyboard.type('40');
    await page.keyboard.press('Escape');
    await expect(field(page)).toBeHidden();
    await expect(e.input).toBeFocused();
    expect(await e.input.evaluate((element) => element.selectionStart)).toBe(5);
  });

  test('text that is not a position says so and stays open', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+g');
    await page.keyboard.type('abc');
    await page.keyboard.press('Enter');
    await expect(field(page)).toBeVisible();
    await expect(page.locator('.config-editor__goto-hint')).toContainText('Type a line number between 1 and');
    await expect(field(page)).toHaveAttribute('aria-describedby', /config-editor-goto-hint-/);
  });
});

test.describe('go to line in a read-only editor', () => {
  setupEditor(test, '?readonly');

  test('works and does not edit', async ({ page }) => {
    const e = editor(page);
    const before = await e.value();
    await e.input.click();
    await page.keyboard.press('Control+g');
    await page.keyboard.type('2');
    await page.keyboard.press('Enter');
    expect((await e.caret()).line).toBe(2);
    expect(await e.value()).toBe(before);
  });
});

for (const width of [390, 320]) {
  test.describe(`go to line at ${width}px`, () => {
    test.use({ viewport: { width, height: 700 } });
    setupEditor(test, '?sample=large');

    test('the field fits on screen', async ({ page }) => {
      const e = editor(page);
      await e.input.click();
      await page.keyboard.press('Control+g');
      const box = await page.locator('.config-editor__goto').boundingBox();
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      await page.keyboard.type('120');
      await page.keyboard.press('Enter');
      expect((await e.caret()).line).toBe(120);
      expect(await caretInView(e)).toBe(true);
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(overflow).toBeLessThanOrEqual(0);
    });
  });
}
