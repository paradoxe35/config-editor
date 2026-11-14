import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// QA round 32, accessibility and UX: a read-only editor has no `Ctrl+M`
// (A32-1); a fold key with nothing to do says so (A32-2); the find
// widget's stepping and replacing buttons say they are unavailable, and do
// nothing, while the query finds nothing (UX32-4); the query field never
// narrows as the window widens (UX32-5); the demo's own toolbar
// (`ConfigEditorToolbar`) keeps the page's look under the host shield
// (UX32-2).

const input = (page) => page.locator('.config-editor__input');
const said = (page) => page.locator('.config-editor__announcement').first();

test.describe('Ctrl+M in a read-only editor (A32-1)', () => {
  setupEditor(test, '?readonly');

  test('is not taken, said or named', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+m');
    await page.waitForTimeout(200);
    await expect(said(page)).not.toHaveText(/Tab/);
    await expect(page.locator('.config-editor__tab-focus-status')).toHaveCount(0);
    const named = ((await input(page).getAttribute('aria-keyshortcuts')) ?? '').split(' ');
    expect(named).not.toContain('Control+M');
  });
});

test.describe('Ctrl+M in an editable editor (A32-1)', () => {
  setupEditor(test, '');

  test('switches Tab and is named', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+m');
    await expect(said(page)).toHaveText(/Tab/);
    await expect(page.locator('.config-editor__tab-focus-status')).toBeVisible();
    const named = ((await input(page).getAttribute('aria-keyshortcuts')) ?? '').split(' ');
    expect(named).toContain('Control+M');
  });
});

test.describe('fold keys with nothing to do (A32-2)', () => {
  setupEditor(test, '?sample=large');

  /** Puts the caret on 1-based `line`. */
  async function goTo(page, line) {
    await input(page).evaluate((element, line) => {
      const lines = element.value.split('\n');
      const at = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
      element.focus();
      element.setSelectionRange(at, at);
    }, line);
  }

  test('say "Nothing to fold" and "Nothing to unfold"', async ({ page }) => {
    await input(page).click();
    // A line that starts no range and is in none: the last.
    const last = await input(page).evaluate((element) => element.value.replace(/\n$/, '').split('\n').length);
    const ranged = await input(page).evaluate((element) => {
      const lines = element.value.split('\n');
      return lines.findIndex((line, index) => /:\s*$/.test(line) && /^\s/.test(lines[index + 1] ?? '')) + 1;
    });
    expect(ranged).toBeGreaterThan(0);
    await page.keyboard.press('Control+End');
    await page.keyboard.type('zz: 1');
    await goTo(page, last + 1);
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(said(page)).toHaveText(/^Nothing to fold\s*$/);
    await page.keyboard.press('Control+Shift+BracketRight');
    await expect(said(page)).toHaveText(/^Nothing to unfold\s*$/);
    // Folded once: the same key again finds nothing more to fold there.
    await goTo(page, ranged);
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(said(page)).toHaveText(/^Folded lines/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(said(page)).toHaveText(/^Unfolded lines/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(said(page)).toHaveText(/^Nothing to unfold\s*$/);
  });
});

test.describe('the find widget with no match (UX32-4)', () => {
  setupEditor(test, '');

  test('marks stepping and replacing unavailable, and they do nothing', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    const buttons = page.locator(
      '.config-editor__find-actions .config-editor__find-button:not([aria-label="Close"]), .config-editor__replace-row .config-editor__find-button',
    );
    await expect(buttons).toHaveCount(4);
    await page.keyboard.type('listen');
    for (const button of await buttons.all()) await expect(button).not.toHaveAttribute('aria-disabled', 'true');
    await page.keyboard.press('Control+a');
    await page.keyboard.type('nothing-matches-this');
    await expect(page.locator('.config-editor__find-count')).toHaveText('No results');
    for (const button of await buttons.all()) await expect(button).toHaveAttribute('aria-disabled', 'true');
    const before = await input(page).inputValue();
    await page.locator('.config-editor__replace-input').fill('x');
    for (const button of await buttons.all()) {
      await button.click({ force: true });
      await button.focus();
      await page.keyboard.press('Enter');
      // Unavailable, not gone: the focus stays.
      await expect(button).toBeFocused();
    }
    expect(await input(page).inputValue()).toBe(before);
    await expect(page.locator('.config-editor__find-count')).toHaveText('No results');
  });

  test('"All" leaves the focus on the button it disables', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+h');
    await page.keyboard.type('listen');
    await page.locator('.config-editor__replace-input').fill('port');
    const all = page.locator('.config-editor__replace-row .config-editor__find-button').last();
    await all.focus();
    await page.keyboard.press('Enter');
    await expect(all).toHaveAttribute('aria-disabled', 'true');
    await expect(all).toBeFocused();
  });
});

for (const query of ['', '?readonly']) {
  test.describe(`the query field as the window widens${query ? ', read-only' : ''} (UX32-5)`, () => {
    setupEditor(test, query);

    for (const typed of ['nothing-matches-this', 'listen']) {
      test(`never narrows: "${typed}"`, async ({ page }) => {
        await input(page).click();
        await page.keyboard.press('Control+f');
        await page.keyboard.type(typed);
        const widths = [];
        for (let width = 280; width <= 1100; width += 10) {
          await page.setViewportSize({ width, height: 620 });
          widths.push([width, await page.locator('.config-editor__find-field .config-editor__find-input').evaluate((element) => element.getBoundingClientRect().width)]);
        }
        const narrower = widths.filter(([, field], index) => index > 0 && field < widths[index - 1][1] - 0.5);
        expect(narrower, 'window widths where the field got narrower').toEqual([]);
        // At its widest it fills its row.
        expect(widths.at(-1)[1]).toBeGreaterThan(140);
      });
    }
  });
}

test.describe("the demo's own toolbar under the host shield (UX32-2)", () => {
  setupEditor(test, '');

  test("keeps the page's rules for its contents", async ({ page }) => {
    await page.addStyleTag({ content: 'span { letter-spacing: 3px; }' });
    const spacing = (selector) => page.locator(selector).first().evaluate((element) => getComputedStyle(element).letterSpacing);
    // The host's toolbar (`ConfigEditorToolbar`): the page's.
    expect(await spacing('.demo__toolbar .demo__picker-label')).toBe('3px');
  });
});
