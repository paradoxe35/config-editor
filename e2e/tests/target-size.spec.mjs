import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The small controls are at least 24x24 CSS px, by their own box or through
// the invisible hit area their `::before`/`::after` gives them (WCAG 2.5.8;
// hard-bar `D5.a11y-1`: the chevrons were 14x22, the change bars 11x22, the
// find buttons 22x22 and the settings rows 19 px tall, with hit areas that
// met their neighbours', for a mouse: only a coarse pointer had the larger
// areas).

const MINIMUM = 24 - 0.5;

/** Each element's hit area: its box joined with its pseudo-elements' (positioned boxes), as [name, width, height]. */
function hitAreas(page, selector) {
  return page.locator(selector).evaluateAll((elements) =>
    elements.map((element, index) => {
      const box = element.getBoundingClientRect();
      let [left, top, right, bottom] = [box.left, box.top, box.right, box.bottom];
      for (const pseudo of ['::before', '::after']) {
        const style = getComputedStyle(element, pseudo);
        if (style.content === 'none' || style.position !== 'absolute') continue;
        const x = box.left + parseFloat(style.left);
        const y = box.top + parseFloat(style.top);
        left = Math.min(left, x);
        top = Math.min(top, y);
        right = Math.max(right, x + parseFloat(style.width));
        bottom = Math.max(bottom, y + parseFloat(style.height));
      }
      return [`${element.className.split(' ')[0]} ${element.getAttribute('aria-label') || index}`, right - left, bottom - top];
    }),
  );
}

async function expectSized(page, selector) {
  const areas = await hitAreas(page, selector);
  expect(areas.length, `${selector} found`).toBeGreaterThan(0);
  for (const [name, width, height] of areas) {
    expect(width, `${name} wide`).toBeGreaterThanOrEqual(MINIMUM);
    expect(height, `${name} tall`).toBeGreaterThanOrEqual(MINIMUM);
  }
}

test.describe('target size of the editor controls with a mouse', () => {
  setupEditor(test, '?sample=large');

  test('fold chevrons and change bars', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('x: 1\n');
    await expect(page.locator('.config-editor__diff-marker-added').first()).toBeVisible();
    await expectSized(page, '.config-editor__fold-toggle');
    await expectSized(page, '.config-editor__diff-marker-added');
  });

  test('the fold chip', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+g');
    await page.keyboard.type('5\n');
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(page.locator('.config-editor__fold-chip').first()).toBeVisible();
    await expectSized(page, '.config-editor__fold-chip');
  });

  test('the find buttons and toggles', async ({ page }) => {
    await editor(page).input.click();
    await page.keyboard.press('Control+h');
    await expect(page.getByRole('textbox', { name: 'Find', exact: true })).toBeFocused();
    await expectSized(page, '.config-editor__find-button, .config-editor__find-toggle');
  });

  test('the demo settings rows', async ({ page }) => {
    await page.locator('.demo__settings-toggle').click();
    await expectSized(page, '.demo__setting');
  });
});
