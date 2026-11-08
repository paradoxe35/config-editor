import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 17, UX: the demo's Settings list closing when focus moves on and
// re-placed when the window resizes (Escape in the editor stays the
// editor's), secret placeholder warnings capped like the schema's problems,
// and Ctrl+/ over a mixed XML selection keeping the document well-formed.

test.describe('the Settings list', () => {
  test.use({ viewport: { width: 1440, height: 800 } });
  setupEditor(test);

  test('closes when Tab moves the focus past it', async ({ page }) => {
    await page.locator('.demo__settings-toggle').click();
    const list = page.locator('.demo__settings-list');
    await expect(list).toBeVisible();
    await list.locator('input').last().focus();
    await page.keyboard.press('Tab');
    await expect(list).toBeHidden();
    // Tab inside it keeps it open.
    await page.locator('.demo__settings-toggle').click();
    await list.locator('input').first().focus();
    await page.keyboard.press('Tab');
    await expect(list).toBeVisible();
  });

  test('Escape in the editor is the editor’s: the list stays and so does the focus', async ({ page }) => {
    const e = editor(page);
    // Opened without focusing the button (as Safari does on a click), then
    // the editor focused.
    await page.locator('.demo__settings-toggle').evaluate((button) => button.click());
    const list = page.locator('.demo__settings-list');
    await expect(list).toBeVisible();
    await e.input.focus();
    await page.keyboard.press('Escape');
    await expect(e.input).toBeFocused();
    await expect(list).toBeVisible();
  });

  test('is re-placed inside the window when it resizes (a phone rotated)', async ({ page }) => {
    const list = page.locator('.demo__settings-list');
    const fits = (width) => async () => {
      const box = await list.boundingBox();
      const pageWidth = await page.evaluate(() => document.documentElement.scrollWidth);
      return box.x >= 0 && box.x + box.width <= width && pageWidth <= width;
    };
    // Opened narrow (moved sideways to fit), then wide, then narrow again.
    for (const width of [320, 900, 1440, 390]) {
      await page.setViewportSize({ width, height: 700 });
      if (!(await list.isVisible())) await page.locator('.demo__settings-toggle').click();
      await expect
        .poll(fits(width), { message: `the open list is within the ${width}px window and the page no wider` })
        .toBe(true);
    }
  });
});

test.describe('thousands of unknown secret placeholders', () => {
  setupEditor(test, '?schema=protected&language=yaml');

  test('show as many warnings as the cap, and the footer says more follow', async ({ page }) => {
    const e = editor(page);
    const token = (index) => `«secret ${index.toString(16).padStart(4, '0')}»`;
    const clients = Array.from(
      { length: 600 },
      (_, index) => `  c${index}:\n    secret: "${token(index)}"\n`,
    ).join('');
    await e.input.evaluate((element, text) => {
      element.focus();
      const at = element.value.length;
      element.setRangeText(text, at, at, 'end');
      element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: text }));
    }, `${(await e.value()).endsWith('\n') ? '' : '\n'}clients:\n${clients}`);
    const status = page.locator('.config-editor__diagnostic-status');
    await expect(status).toContainText('20+ warnings');
    await expect(status).not.toContainText('warnings+');
    const shown = await page.locator('.config-editor__diagnostic-marker').count();
    expect(shown, 'underlined problems').toBeLessThanOrEqual(60);
  });
});

test.describe('Ctrl+/ over a mixed XML selection', () => {
  setupEditor(test, '?language=xml');

  test('comments whole lines, stays well-formed, and toggles back exactly', async ({ page }) => {
    const e = editor(page);
    const text = '<r>\n<a>1</a>\n<!-- <b>2</b> -->\n<c>3</c>\n<n>1</n>\n</r>\n';
    await e.paste(text);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('<a>');
      const end = element.value.indexOf('3</c>') + 1;
      element.setSelectionRange(start, end);
    });
    await page.keyboard.press('Control+/');
    const once = await e.value();
    expect(once).toContain('<c>3</c> -->');
    expect(await page.evaluate((xml) => {
      const parsed = new DOMParser().parseFromString(xml, 'application/xml');
      return parsed.getElementsByTagName('parsererror').length;
    }, once)).toBe(0);
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe(text);
  });
});
