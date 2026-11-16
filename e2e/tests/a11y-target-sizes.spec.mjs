import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Pointer and touch targets are at least 24px: chevrons, change bars, footer and notice buttons, tabs,
// fold chips, popups and their links.

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
    // Quarter-pixel steps: the last point hit on each side, plus a step.
    const reach = (dx, dy) => {
      let distance = 0;
      while (distance < 60 && hits(cx + dx * (distance + 0.25), cy + dy * (distance + 0.25))) distance += 0.25;
      return distance;
    };
    return { width: reach(-1, 0) + reach(1, 0) + 0.5, height: reach(0, -1) + reach(0, 1) + 0.5 };
  });
}

async function expectFingerSized(page, selector) {
  const targets = page.locator(selector);
  expect(await targets.count(), `${selector} is on screen`).toBeGreaterThan(0);
  const area = await hitArea(targets.first());
  expect(area.width, `${selector} is 24px wide to a finger`).toBeGreaterThanOrEqual(24);
  expect(area.height, `${selector} is 24px tall to a finger`).toBeGreaterThanOrEqual(24);
}

const input = (page) => page.locator('.config-editor__input');

// --- Every pointer target is at least 24×24 px. ----------------------

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
      if (element.parentElement.closest('button, a[href], [role="tab"], [role="option"]')) continue;
      // A line number's parts are the number's (exempt), but for its
      // change markers, buttons of their own.
      if (element.parentElement.closest('.config-editor__gutter-content > div') && !element.matches('button')) continue;
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

test.describe('touch targets on a phone', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
  setupEditor(test, '?sample=large');

  test('chevrons, change bars, footer buttons and notice buttons are 24px to a finger', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('\n') + 1;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('x');
    await expect(page.locator('.config-editor__diff-marker-modified')).toHaveCount(1);
    await expectFingerSized(page, '.config-editor__fold-toggle');
    await expectFingerSized(page, '.config-editor__diff-marker-modified');
    await expectFingerSized(page, '.config-editor__change-status');
    await expectFingerSized(page, '.config-editor__diagnostic-status');
    await page.locator('.demo__external-change').click();
    await expect(page.locator('.config-editor__notice')).toBeVisible();
    await expectFingerSized(page, '.config-editor__notice-keep');
    await expectFingerSized(page, '.config-editor__notice-reload');
  });
});

test.describe('touch targets in the documents view', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
  setupEditor(test, '?documents');

  test('a tab’s close button and the "+" are 24px to a finger', async ({ page }) => {
    await expectFingerSized(page, '.config-editor-documents__close');
    await expectFingerSized(page, '.config-editor-documents__add');
  });
});

// A group of its own (its own helpers).
test.describe(() => {
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


  test.describe('touch targets of the fold chip and the demo checkboxes', () => {
    test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });
  setupEditor(test, '?sample=large');

  test('a folded block’s chip and the demo’s checkboxes are 24px to a finger', async ({ page }) => {
    await page.locator('.config-editor__fold-toggle').first().dispatchEvent('click');
      const chip = page.locator('.config-editor__fold-chip').first();
      await expect(chip).toBeVisible();
      await expectFingerSized(chip, 'the fold chip');
    await page.locator('.demo__settings-toggle').click();
      // The demo places the open list in the next frame; a finger lands on it once it is in the window.
      const list = page.locator('.demo__settings-list');
      await expect
        .poll(async () => {
          const box = await list.boundingBox();
          return box !== null && box.x >= 0 && box.x + box.width <= 390;
      }, { message: 'the open list is within the window' })
        .toBe(true);
      await expectFingerSized(page.locator('.demo__setting input').first(), 'a setting’s checkbox');
    });
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example('vanilla.html', options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('touch targets', () => {
    test.use({ viewport: { width: 390, height: 800 }, hasTouch: true, isMobile: true });

    const size = (locator) => locator.evaluate((element) => {
      const rect = element.getBoundingClientRect();
      return [rect.width, rect.height];
    });

    test('the Edit-as notice’s Dismiss is at least 24 by 24', async ({ page }) => {
      const errors = await open(page, { value: '# a comment\nport: 1\n', editAs: 'json' });
      const dismiss = page.locator('.config-editor__edit-as-notice-dismiss');
      await expect(dismiss).toBeVisible();
      const [width, height] = await size(dismiss);
      expect(width).toBeGreaterThanOrEqual(24);
      expect(height).toBeGreaterThanOrEqual(24);
      expect(errors).toEqual([]);
    });

    test('the hover card’s Documentation link is at least 24 tall, and the card no taller for it', async ({ page }) => {
      const docs = { type: 'string', description: 'The server name.', 'x-docs-url': 'https://example.com/name' };
      const plain = { type: 'string', description: 'The server name.' };
      const card = page.locator('.config-editor__hover');
      const heights = [];
      for (const name of [docs, plain]) {
        await page.goto('about:blank');
        const errors = await open(page, { value: 'name: edge\n', schema: JSON.stringify({ type: 'object', properties: { name } }) });
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('Control+k');
        await page.keyboard.press('Control+i');
        await expect(card).toBeVisible();
        heights.push((await card.boundingBox()).height);
        if (name === docs) {
          const [width, height] = await size(page.locator('.config-editor__hover-link'));
          expect(width).toBeGreaterThanOrEqual(24);
          expect(height).toBeGreaterThanOrEqual(24);
        }
        expect(errors).toEqual([]);
      }
      // The link's line, no more: about one 11px line and the grid's gap.
      expect(heights[0] - heights[1]).toBeLessThan(26);
    });
  });
});

// `Ctrl+Space` where nothing is offered says
// "No suggestions" in every language; `aria-keyshortcuts` names the
// documented single chords (the table itself is checked in
// `editor/a11y/keyboard_tests.rs`); every pointer target the editor draws
// is at least 24×24 px.
test.describe('every pointer target of an editor', () => {
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

  // Change markers on adjacent lines (added, modified and
  // removed) and fold chips on adjacent lines (every range folded).
  test('change markers and fold chips on adjacent lines', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+Home');
    // Lines written above the first (modified and added markers, one per
    // line), and a line removed further down.
    await page.keyboard.insertText('x: 1\ny: 2\n');
    await page.keyboard.press('Control+g');
    await page.keyboard.type('10\n');
    await page.keyboard.press('Control+Shift+k');
    await expect(page.locator('.config-editor__diff-marker').nth(1)).toBeAttached();
    await expect(page.locator('.config-editor__diff-deleted').first()).toBeAttached();
    await page.locator('.config-editor__gutter').hover();
    const markers = (await targets(page)).filter(([name]) => name.includes('diff-'));
    expect(markers.length, 'change markers measured').toBeGreaterThanOrEqual(3);
    await expectSized(page, 'adjacent change markers');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+0');
    await expect(page.locator('.config-editor__fold-chip').nth(1)).toBeVisible();
    const chips = (await targets(page)).filter(([name]) => name.includes('fold-chip'));
    expect(chips.length, 'fold chips measured').toBeGreaterThan(1);
    await expectSized(page, 'adjacent fold chips');
  });
});

test.describe('every pointer target of the suggestions and the hover card', () => {
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

test.describe('every pointer target of a documents view', () => {
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
