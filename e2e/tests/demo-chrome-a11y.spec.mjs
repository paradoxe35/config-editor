import { expect, test } from '@playwright/test';

// The demo pages give a screen reader something to orient by: a heading, a
// `main` landmark and a skip link to the editor (hard round 28's
// `D28.a11y-1`), their pickers named with their visible label (hard round
// 23's `D23.a11y-1`), and the settings switches named in words (hard round
// 23's `D23.a11y-3`).

for (const query of ['', '?documents']) {
  test(`demo${query}: a heading, a main landmark and a skip link`, async ({ page }) => {
    await page.goto(query);
    await page.locator('.config-editor__input').first().waitFor();
    await expect(page.locator('h1')).toHaveCount(1);
    await expect(page.locator('main')).toHaveCount(1);
    await page.keyboard.press('Tab');
    await expect(page.locator('.demo__skip')).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.locator('main')).toBeFocused();
  });
}

test('demo: the pickers are named with their visible label', async ({ page }) => {
  await page.goto('');
  await page.locator('.config-editor__input').waitFor();
  for (const [select, visible] of [
    ['.demo__language-select', 'File'],
    ['.demo__edit-select', 'Edit as'],
  ]) {
    const name = await page.locator(select).evaluate((element) => element.getAttribute('aria-label') ?? element.labels[0].textContent);
    expect(name).toContain(visible);
  }
});

test('demo: the settings switches are named in words', async ({ page }) => {
  await page.goto('');
  await page.locator('.config-editor__input').waitFor();
  await page.locator('.demo__settings-toggle').click();
  const labels = await page.locator('.demo__settings-list label').allTextContents();
  expect(labels.length).toBeGreaterThan(5);
  for (const label of labels) expect(label.trim()).not.toMatch(/[a-z][A-Z]/);
  expect(labels.map((label) => label.trim())).toContain('Show toolbar');
});
