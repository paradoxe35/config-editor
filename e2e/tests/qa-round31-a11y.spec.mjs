import { expect, test } from '@playwright/test';
import { setupEditor } from './helpers.mjs';

// QA round 31, accessibility: `Ctrl+Space` where nothing is offered says
// "No suggestions" in every language (A31-1); `aria-keyshortcuts` names the
// documented single chords (A31-2, the table itself is checked in
// `editor/a11y/keyboard_tests.rs`); every pointer target the editor draws
// is at least 24×24 px (A31-3).

const input = (page) => page.locator('.config-editor__input');
const said = (page) => page.locator('.config-editor__announcement').first();

// [language, what to type: a key nothing in the schema starts with]
const NOTHING_TO_OFFER = [
  ['yaml', 'bindzzq'],
  ['json', '{"bindzzq'],
  ['toml', 'bindzzq'],
  ['hcl', 'bindzzq'],
  ['xml', '<bindzzq'],
];

for (const [language, typed] of NOTHING_TO_OFFER) {
  test.describe(`${language}: Ctrl+Space with nothing to offer (A31-1)`, () => {
    setupEditor(test, `?language=${language}&schema=help`);

    test('says "No suggestions"', async ({ page }) => {
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type(typed);
      await page.keyboard.press('Escape');
      await page.keyboard.press('Control+Space');
      await expect(page.locator('.config-editor__completion')).toHaveCount(0);
      await expect(said(page)).toHaveText(/^No suggestions ?$/);
    });
  });
}

test.describe('Ctrl+Space with something to offer (A31-1)', () => {
  setupEditor(test, '?language=yaml&schema=help');

  test('shows the list and says no "No suggestions"', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+Space');
    await expect(page.locator('.config-editor__completion').first()).toBeVisible();
    await expect(said(page)).not.toHaveText(/No suggestions/);
  });
});

test.describe('aria-keyshortcuts (A31-2)', () => {
  setupEditor(test, '?language=yaml');

  test('names the line, comment, fold and move chords the README lists', async ({ page }) => {
    const named = ((await input(page).getAttribute('aria-keyshortcuts')) ?? '').split(' ');
    for (const chord of ['Control+Shift+[', 'Control+Shift+]', 'Control+/', 'Control+Shift+K', 'Control+]', 'Control+[',
      'Control+Enter', 'Control+Shift+Enter', 'Alt+ArrowUp', 'Alt+ArrowDown', 'Shift+Alt+ArrowUp', 'Control+Z', 'Alt+F3', 'F3']) {
      expect(named, chord).toContain(chord);
    }
    // Keys of the widgets and lists, not of the text, are not named.
    for (const chord of ['Alt+C', 'Control+Alt+Enter']) expect(named, chord).not.toContain(chord);
  });
});

// --- Every pointer target is at least 24×24 px (A31-3). ----------------------

const MINIMUM = 24 - 0.5;

/**
 * Each pointer target's hit area (its box joined with positioned
 * pseudo-elements'), as [name, width, height]. Targets: buttons, links,
 * fields, tabs, options and what shows a pointer cursor, not inside
 * another target. Left out, as WCAG 2.5.8 leaves out targets in a line of
 * text: the text area itself, the gutter's line numbers and the underlined
 * problems in the text (each a line of the text tall), and links within a
 * sentence.
 */
function targets(page) {
  return page.evaluate(() => {
    const isTarget = (element) =>
      element.matches('button, a[href], input, select, [role="tab"], [role="option"]') ||
      getComputedStyle(element).cursor === 'pointer';
    const exempt = (element) =>
      element.matches('textarea, .config-editor__gutter-content > div, .config-editor__diagnostic-marker') ||
      (element.matches('a') && element.parentElement.textContent.trim() !== element.textContent.trim());
    const out = [];
    for (const element of document.querySelectorAll('.config-editor *, .config-editor-documents *')) {
      if (!element.getClientRects().length || !isTarget(element) || exempt(element)) continue;
      if (element.parentElement.closest('button, a[href], [role="tab"], [role="option"], .config-editor__gutter-content > div')) continue;
      if (getComputedStyle(element).visibility === 'hidden') continue;
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
      out.push([`${element.tagName.toLowerCase()}.${String(element.className).split(' ')[0]} ${element.getAttribute('aria-label') ?? element.textContent.trim().slice(0, 20)}`, right - left, bottom - top]);
    }
    return out;
  });
}

async function expectSized(page, state) {
  const found = await targets(page);
  expect(found.length, `${state}: targets found`).toBeGreaterThan(0);
  const small = found.filter(([, width, height]) => width < MINIMUM || height < MINIMUM).map(([name, w, h]) => `${name} ${w}x${h}`);
  expect(small, `${state}: targets under 24×24 px`).toEqual([]);
}

test.describe('every pointer target of an editor (A31-3)', () => {
  setupEditor(test, '?sample=large');

  test('in the text, the footer and each popup', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.type('x: 1\n');
    await expect(page.locator('.config-editor__diff-marker-added').first()).toBeVisible();
    await page.locator('.config-editor__gutter').hover();
    await expectSized(page, 'text');
    await page.keyboard.press('Control+h');
    await expectSized(page, 'find and replace');
    await page.keyboard.press('Escape');
    await input(page).focus();
    await page.keyboard.press('Control+g');
    await expectSized(page, 'go to line');
    await page.keyboard.press('Escape');
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F3');
    await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
    await expectSized(page, 'change preview');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+m');
    await expect(page.locator('.config-editor__tab-focus-status')).toBeVisible();
    await expectSized(page, 'tab moves focus');
    await page.keyboard.press('Control+g');
    await page.keyboard.type('20\n');
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(page.locator('.config-editor__fold-chip').first()).toBeVisible();
    await expectSized(page, 'a fold');
  });
});

test.describe('every pointer target of the suggestions and the hover card (A31-3)', () => {
  setupEditor(test, '?language=yaml');

  test('in the list and the card', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+Space');
    await expect(page.locator('.config-editor__completion').first()).toBeVisible();
    await expectSized(page, 'suggestions');
    await page.keyboard.press('Escape');
    // On `listen`, a key the schema documents.
    await input(page).evaluate((element) => {
      const at = element.value.indexOf('listen') + 2;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(page.locator('.config-editor__hover')).toBeVisible();
    await expectSized(page, 'hover card');
  });
});

test.describe('every pointer target of a documents view (A31-3)', () => {
  setupEditor(test, '?documents');

  test('the tabs, their close buttons and the add button', async ({ page }) => {
    // A dirty tab shows its close button on hover; the active one always.
    await input(page).first().click();
    await page.keyboard.type('x');
    await page.locator('[role="tab"]').first().hover();
    await expect(page.locator('.config-editor-documents__close').first()).toBeVisible();
    await expectSized(page, 'tabs');
  });
});
