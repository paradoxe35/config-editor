import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// Pointer, key and layout rules of the shell around the text, in the shipped demo:
// a press on the gutter keeps the focus in the text (a drag that starts on a line number or a diff marker
// ends over the text, and the keys after it are the editor's); the documents' switch keys page no document;
// a renamed tab is shown whole again; the change preview stays above the find widget.

const input = (page) => page.locator('.config-editor__input');
const focusedClass = (page) => page.evaluate(() => document.activeElement?.className ?? '');

// Types `text` at the start of the first line, leaving the text focused.
async function edit(page, text) {
  await input(page).click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type(text);
}

test.describe('a drag from the gutter', () => {
  setupEditor(test);

  for (const [name, selector] of [
    ['a line number', '.config-editor__gutter-content > div[data-line="3"]'],
    ['a diff marker', '.config-editor__diff-marker-modified'],
  ]) {
    test(`from ${name} leaves the focus in the text, so the keys after it type`, async ({ page }) => {
      await edit(page, 'X');
      await page.evaluate(() => document.activeElement.blur());
      expect(await focusedClass(page)).not.toContain('config-editor__input');
      const from = await page.locator(selector).first().boundingBox();
      const to = await input(page).boundingBox();
      await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
      await page.mouse.down();
      await page.mouse.move(to.x + 120, from.y + from.height / 2, { steps: 6 });
      await page.mouse.up();
      expect(await focusedClass(page)).toContain('config-editor__input');
      await page.keyboard.type('Q');
      await expect(input(page)).toHaveValue(/Q/);
    });
  }
});

test.describe('the documents view', () => {
  setupEditor(test, '?documents');

  test('Alt+PageDown and Alt+PageUp page no document, so a document comes back where it was left', async ({ page }) => {
    const main = page.locator('[role="tabpanel"][data-document-id="main"] .config-editor__input');
    await main.fill(Array.from({ length: 200 }, (_, index) => `key${index}: value`).join('\n'));
    await main.evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, 0);
      element.scrollTop = 400;
    });
    await expect.poll(() => main.evaluate((element) => element.scrollTop)).toBe(400);
    await page.keyboard.press('Alt+PageDown');
    await expect(page.locator('[role="tab"][data-document-id="limits"]')).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('Alt+PageUp');
    await expect(page.locator('[role="tab"][data-document-id="main"]')).toHaveAttribute('aria-selected', 'true');
    expect(await main.evaluate((element) => element.scrollTop)).toBe(400);
  });
});

test.describe('a renamed tab on a narrow screen', () => {
  test.use({ viewport: { width: 320, height: 700 } });
  setupEditor(test, '?documents');

  test('the active tab is scrolled back whole into the strip', async ({ page }) => {
    for (let added = 0; added < 3; added += 1) await page.locator('.demo__add').click();
    const limits = page.locator('[role="tab"][data-document-id="limits"]');
    await limits.click();
    await page.locator('.config-editor-documents__tabs').evaluate((strip) => {
      const tab = strip.querySelector('[data-document-id="limits"]');
      strip.scrollLeft = tab.offsetLeft + tab.offsetWidth - strip.clientWidth;
    });
    await page.locator('.demo__rename').click();
    await expect(limits.locator('.config-editor-documents__name')).toHaveText('conf.d/10-limits.toml');
    await expect
      .poll(async () => {
        const tab = await limits.boundingBox();
        const strip = await page.locator('.config-editor-documents__tabs').boundingBox();
        return tab.x >= strip.x - 0.5 && tab.x + tab.width <= strip.x + strip.width + 0.5;
      })
      .toBe(true);
  });
});

test.describe('on a narrow screen', () => {
  test.use({ viewport: { width: 320, height: 700 } });
  setupEditor(test);

  test('the change preview stays above the open find widget, so its buttons take the pointer', async ({ page }) => {
    await edit(page, 'X');
    await page.keyboard.press('Alt+F3');
    await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
    await page.keyboard.press('Control+h');
    await expect(page.locator('.config-editor__find')).toBeVisible();
    const toggle = page.locator('.config-editor__find button[aria-label="Toggle replace"]').first();
    if (await toggle.count()) await toggle.click();
    for (const label of ['Revert', 'Close']) {
      const button = page
        .locator('.config-editor__diff-peek button', { hasText: label })
        .or(page.locator(`.config-editor__diff-peek button[aria-label*="${label}"]`))
        .first();
      const hit = await button.evaluate((element) => {
        const box = element.getBoundingClientRect();
        const at = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
        return element === at || element.contains(at);
      });
      expect(hit, `${label} takes the pointer`).toBe(true);
    }
  });
});
