import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 16, UX: the demo's Settings dropdown inside the window at every
// width and closing like a menu, the toolbar on one row while editing as
// another language, Alt+Enter with no docs link, and touch targets (the
// fold chip, the demo's checkboxes).

/**
 * The area a finger hits for `locator`: from its center outward, how far
 * the element (or its pseudo-elements) still takes the point.
 */
function hitArea(locator) {
  return locator.evaluate((element) => {
    const box = element.getBoundingClientRect();
    const [cx, cy] = [box.left + box.width / 2, box.top + box.height / 2];
    const hits = (x, y) => {
      const found = document.elementFromPoint(x, y);
      return found === element || element.contains(found);
    };
    const reach = (dx, dy) => {
      let distance = 0;
      while (distance < 60 && hits(cx + dx * (distance + 0.25), cy + dy * (distance + 0.25))) distance += 0.25;
      return distance;
    };
    return { width: reach(-1, 0) + reach(1, 0) + 0.5, height: reach(0, -1) + reach(0, 1) + 0.5 };
  });
}

async function expectFingerSized(locator, name) {
  const area = await hitArea(locator);
  expect(area.width, `${name} is 24px wide to a finger`).toBeGreaterThanOrEqual(24);
  expect(area.height, `${name} is 24px tall to a finger`).toBeGreaterThanOrEqual(24);
}

for (const [page_, query] of [['the demo', ''], ['the documents demo', '?documents']]) {
  for (const width of [320, 390, 800, 1200, 1440]) {
    test.describe(`${page_} at ${width}px`, () => {
      test.use({ viewport: { width, height: 800 } });
      setupEditor(test, query);

      test('the Settings dropdown opens inside the window', async ({ page }) => {
        await page.locator('.demo__settings-toggle').click();
        const list = page.locator('.demo__settings-list');
        await expect(list).toBeVisible();
        await expect
          .poll(async () => {
            const box = await list.boundingBox();
            return box.x >= 0 && box.x + box.width <= width;
          }, { message: 'the open list is within the window' })
          .toBe(true);
      });
    });
  }
}

test.describe('the Settings dropdown', () => {
  setupEditor(test);

  test('Escape closes it and gives the focus back to its button', async ({ page }) => {
    const toggle = page.locator('.demo__settings-toggle');
    await toggle.click();
    await page.locator('.demo__settings-list input').first().focus();
    await page.keyboard.press('Escape');
    await expect(page.locator('.demo__settings-list')).toBeHidden();
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
    await expect(toggle).toBeFocused();
  });

  test('a click outside closes it; a click on a setting does not', async ({ page }) => {
    await page.locator('.demo__settings-toggle').click();
    const list = page.locator('.demo__settings-list');
    await list.locator('input').first().click();
    await expect(list).toBeVisible();
    await editor(page).input.click();
    await expect(list).toBeHidden();
  });
});

test.describe('the toolbar while editing as another language', () => {
  test.use({ viewport: { width: 1200, height: 800 } });
  setupEditor(test, '?edit=json');

  test('stays on one row at 1200px', async ({ page }) => {
    const toolbar = page.locator('.demo__toolbar');
    await expect(toolbar).toContainText('editing as');
    // One row: every control starts above where the shortest one ends.
    const boxes = await toolbar.evaluate((element) =>
      [...element.children].map((child) => {
        const box = child.getBoundingClientRect();
        return [Math.round(box.top), Math.round(box.bottom)];
      }),
    );
    const lowestTop = Math.max(...boxes.map(([top]) => top));
    const firstBottom = Math.min(...boxes.map(([, bottom]) => bottom));
    expect(lowestTop, `toolbar controls (top, bottom): ${JSON.stringify(boxes)}`).toBeLessThan(firstBottom);
  });
});

test.describe('Alt+Enter', () => {
  setupEditor(test);

  test('does nothing without a docs link at the caret', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    const before = await e.value();
    await page.keyboard.press('Alt+Enter');
    expect(await e.value()).toBe(before);
  });
});

test.describe('touch targets in round 16', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
  setupEditor(test, '?sample=large');

  test('a folded block’s chip and the demo’s checkboxes are 24px to a finger', async ({ page }) => {
    await page.locator('.config-editor__fold-toggle').first().dispatchEvent('click');
    const chip = page.locator('.config-editor__fold-chip').first();
    await expect(chip).toBeVisible();
    await expectFingerSized(chip, 'the fold chip');
    await page.locator('.demo__settings-toggle').click();
    await expectFingerSized(page.locator('.demo__setting input').first(), 'a setting’s checkbox');
  });
});
