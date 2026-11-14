import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setupEditor } from './helpers.mjs';

// QA round 35, accessibility: a press on a settings label's words toggles it
// and keeps the list open (A35-1); the checkboxes keep their size in wrapping
// rows (A35-2); the example pages have a heading, a main landmark and 24px
// controls (A35-3); the problem summary's live region is written only when
// its text changes (A35-4).

const list = (page) => page.locator('.demo__settings-list');
const labels = (page) => page.locator('label.demo__setting');

async function openList(page) {
  await page.locator('.demo__settings-toggle').click();
  await expect(list(page)).toBeVisible();
  // The demo places the open list in the next frame.
  await expect
    .poll(async () => {
      const box = await list(page).boundingBox();
      return box !== null && box.x >= 0;
    })
    .toBe(true);
}

/** The point on a label's words (its right end, away from the checkbox). */
async function onWords(label) {
  const box = await label.boundingBox();
  return [box.x + box.width - 6, box.y + box.height / 2];
}

for (const [how, touch] of [['click', false], ['tap', true]]) {
  test.describe(`a ${how} on a setting's words (A35-1)`, () => {
    test.use({ hasTouch: touch, viewport: { width: 900, height: 800 } });
    setupEditor(test, '?language=json');

    test('toggles each setting and keeps the list open', async ({ page }) => {
      await openList(page);
      const count = await labels(page).count();
      expect(count).toBeGreaterThan(3);
      for (let index = 0; index < count; index += 1) {
        const label = labels(page).nth(index);
        const box = label.locator('input');
        const before = await box.isChecked();
        const [x, y] = await onWords(label);
        if (touch) await page.touchscreen.tap(x, y);
        else await page.mouse.click(x, y);
        await expect(list(page), `the list stays open after setting ${index}`).toBeVisible();
        await expect(box, `setting ${index} toggled`).toBeChecked({ checked: !before });
      }
    });

    test('the skip link still lands on the editor', async ({ page }) => {
      await page.keyboard.press('Tab');
      await page.locator('.demo__skip').press('Enter');
      await expect(page.locator('.config-editor__input')).toBeFocused();
    });
  });
}

test.describe('the settings checkboxes (A35-2)', () => {
  test.use({ viewport: { width: 320, height: 800 } });
  setupEditor(test, '?language=json');

  test('keep one size in every row, wrapping or not', async ({ page }) => {
    await openList(page);
    const widths = await page
      .locator('.demo__setting input')
      .evaluateAll((boxes) => boxes.map((box) => box.getBoundingClientRect().width));
    expect(new Set(widths.map((width) => Math.round(width))).size, `widths ${widths}`).toBe(1);
    expect(widths[0]).toBeGreaterThanOrEqual(13);
    // A wrapping row squeezes a box that may shrink (the text is longer than the row).
    const shrink = await page.locator('.demo__setting input').evaluateAll((boxes) => boxes.map((box) => getComputedStyle(box).flexShrink));
    expect(new Set(shrink)).toEqual(new Set(['0']));
  });
});

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const built = existsSync(join(root, 'js', 'config-editor', 'pkg', 'config_editor_bg.wasm'));

for (const name of ['vanilla.html', 'react.html', 'vue.html', 'svelte.html']) {
  test.describe(`${name} (A35-3)`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('has a heading, a main landmark and controls of 24px', async ({ page }) => {
      await page.goto(pathToFileURL(join(root, 'js', 'examples', name)).href);
      await page.locator('header button').first().waitFor();
      await expect(page.locator('h1')).toHaveCount(1);
      await expect(page.locator('main')).toHaveCount(1);
      const small = await page.evaluate(() =>
        [...document.querySelectorAll('header button, header select, header input, header label')]
          .map((element) => [element.id || element.tagName, element.getBoundingClientRect()])
          .filter(([, box]) => box.width > 0 && box.height < 24)
          .map(([id, box]) => `${id} ${box.width}x${box.height}`),
      );
      expect(small, 'controls under 24px').toEqual([]);
    });
  });
}

test.describe('the problem summary live region (A35-4)', () => {
  setupEditor(test, '?language=json');

  test('is not written again while typing leaves its text as it is', async ({ page }) => {
    const input = page.locator('.config-editor__input');
    await input.click();
    // The sample is YAML, so as JSON it is broken: errors stay while typing.
    await page.keyboard.press('Control+End');
    await page.keyboard.type('a');
    await expect(page.locator('.config-editor__diagnostic-status')).toContainText('error');
    await page.waitForTimeout(500);
    await page.locator('.config-editor__diagnostic-status').evaluate((status) => {
      window.__writes = [];
      let last = status.textContent;
      new MutationObserver(() => {
        window.__writes.push([last, status.textContent]);
        last = status.textContent;
      }).observe(status, { subtree: true, childList: true, characterData: true });
    });
    await page.keyboard.type('abcdefghij', { delay: 30 });
    await page.waitForTimeout(500);
    const writes = await page.evaluate(() => window.__writes);
    expect(writes, 'no write while the summary says the same').toEqual([]);
  });
});
