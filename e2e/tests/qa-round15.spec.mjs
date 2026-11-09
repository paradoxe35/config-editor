import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 15: the suggestion list as screen readers follow it, the change
// preview on phones, the go-to-line field, touch targets, and the demo page
// (its controls, its theme on every part of the page, phone widths).

/** WCAG contrast of two computed `rgb()` colors, the first over the second. */
function contrastIn(page, selector) {
  return page.locator(selector).first().evaluate((element) => {
    const parse = (color) => color.match(/[\d.]+/g).map(Number);
    const backgroundOf = (node) => {
      for (let at = node; at; at = at.parentElement) {
        const [r, g, b, a = 1] = parse(getComputedStyle(at).backgroundColor);
        if (a > 0.5) return [r, g, b];
      }
      return [255, 255, 255];
    };
    const luminance = (rgb) =>
      rgb
        .map((part) => part / 255)
        .map((part) => (part <= 0.03928 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4))
        .reduce((sum, part, i) => sum + part * [0.2126, 0.7152, 0.0722][i], 0);
    const [fg, bg] = [parse(getComputedStyle(element).color).slice(0, 3), backgroundOf(element)].map(luminance);
    return (Math.max(fg, bg) + 0.05) / (Math.min(fg, bg) + 0.05);
  });
}

const pickTheme = (page, theme) => page.locator('.demo__theme-select').selectOption(theme);

test.describe('the suggestion list, for screen readers', () => {
  setupEditor(test);

  test('the textarea names the list and its highlighted suggestion', async ({ page }) => {
    const e = editor(page);
    const list = page.locator('.config-editor__completions');
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('li');
    await expect(list.locator('[role="option"]').first()).toBeVisible();
    const listId = await list.getAttribute('id');
    expect(listId).toBeTruthy();
    await expect(e.input).toHaveAttribute('aria-controls', listId);
    const active = await e.input.getAttribute('aria-activedescendant');
    expect(active).toBeTruthy();
    await expect(page.locator(`#${active}`)).toHaveAttribute('aria-selected', 'true');
    await expect(list.locator('[role="option"][aria-selected="true"]')).toHaveCount(1);
    const options = await list.locator('[role="option"]').count();
    await expect(list.locator('[role="option"][aria-selected="false"]')).toHaveCount(options - 1);
    // The accessibility tree has the selected option.
    expect(await list.ariaSnapshot()).toMatch(/- option ".+" \[selected\]/);
    if (options > 1) {
      await page.keyboard.press('ArrowDown');
      const next = await e.input.getAttribute('aria-activedescendant');
      expect(next).not.toBe(active);
      await expect(page.locator(`#${next}`)).toHaveAttribute('aria-selected', 'true');
      await expect(page.locator(`#${active}`)).toHaveAttribute('aria-selected', 'false');
    }
    await page.keyboard.press('Escape');
    await expect(list.locator('[role="option"]')).toHaveCount(0);
    await expect(e.input).not.toHaveAttribute('aria-activedescendant', /./);
    // Closed: it controls no list (QA 23: the open state shows by it).
    await expect(e.input).not.toHaveAttribute('aria-controls', /./);
  });
});

test.describe('the change preview on a phone', () => {
  test.use({ viewport: { width: 390, height: 844 } });
  setupEditor(test);

  test('it is as tall as its content when there is room: the Current side shows', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835');
      element.setSelectionRange(at, at + 4);
    });
    await page.keyboard.type('9000');
    await page.locator('.config-editor__diff-marker-modified').first().click();
    const peek = page.locator('.config-editor__diff-peek');
    await expect(peek).toBeVisible();
    const sizes = await peek.evaluate((element) => ({ scroll: element.scrollHeight, client: element.clientHeight }));
    expect(sizes.scroll, 'nothing scrolled out of the preview').toBeLessThanOrEqual(sizes.client + 1);
    const box = await peek.boundingBox();
    const current = await peek.locator('h3').last().boundingBox();
    expect(current.y + current.height).toBeLessThanOrEqual(box.y + box.height);
    await expect(peek.locator('.config-editor__diff-peek-current')).toBeVisible();
  });
});

test.describe('go to line', () => {
  setupEditor(test, '?sample=large');

  test('the field has its box, and the hint lines up with the typed text', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+g');
    const field = page.locator('.config-editor__goto-input');
    await expect(field).toBeFocused();
    const look = await field.evaluate((element) => {
      const style = getComputedStyle(element);
      const hint = document.querySelector('.config-editor__goto-hint');
      const hintStyle = getComputedStyle(hint);
      return {
        border: style.borderTopWidth,
        borderColor: style.borderTopColor,
        textLeft: element.getBoundingClientRect().left + parseFloat(style.borderLeftWidth) + parseFloat(style.paddingLeft),
        hintLeft: hint.getBoundingClientRect().left + parseFloat(hintStyle.paddingLeft),
      };
    });
    expect(look.border).toBe('1px');
    expect(look.borderColor).not.toBe('rgba(0, 0, 0, 0)');
    expect(Math.abs(look.textLeft - look.hintLeft)).toBeLessThanOrEqual(1);
  });
});

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

test.describe('the demo page', () => {
  setupEditor(test);

  test('Apply changes is as tall as the selects beside it, and looks disabled when it is', async ({ page }) => {
    const apply = page.getByRole('button', { name: 'Apply changes' });
    await expect(apply).toBeDisabled();
    const button = await apply.boundingBox();
    const select = await page.locator('.demo__theme-select').boundingBox();
    expect(button.height).toBeLessThanOrEqual(select.height + 4);
    const disabled = await contrastIn(page, '.config-editor__toolbar .config-editor__action-button');
    // Enabled after an edit.
    await editor(page).input.click();
    await page.keyboard.type('x');
    await expect(apply).toBeEnabled();
    const enabled = await contrastIn(page, '.config-editor__toolbar .config-editor__action-button');
    expect(enabled / disabled).toBeGreaterThan(1.5);
  });

  test('in a dark theme the whole page is dark, and the toolbar labels readable', async ({ page }) => {
    for (const theme of ['nord', 'dracula', 'vscode-dark', 'github-dark']) {
      await pickTheme(page, theme);
      const below = await page.evaluate(() => {
        const at = document.elementFromPoint(20, window.innerHeight - 4);
        return getComputedStyle(at.closest('.demo') ?? at).backgroundColor;
      });
      const editorBackground = await page
        .locator('.config-editor__surface')
        .evaluate((element) => getComputedStyle(element).backgroundColor);
      expect(below, `${theme}: the page under the editor`).toBe(editorBackground);
      expect(await contrastIn(page, '.demo__picker-label'), `${theme}: toolbar labels`).toBeGreaterThanOrEqual(4.5);
    }
  });

  test('the theme select shows its longest name whole', async ({ page }) => {
    for (const width of [1200, 390]) {
      await page.setViewportSize({ width, height: 800 });
      const fit = await page.locator('.demo__theme-select').evaluate((select) => {
        const style = getComputedStyle(select);
        const context = document.createElement('canvas').getContext('2d');
        context.font = `${style.fontSize} ${style.fontFamily}`;
        const widest = Math.max(...[...select.options].map((option) => context.measureText(option.text).width));
        // The text area: the box less its padding, border and arrow.
        return { widest, room: select.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - 16 };
      });
      expect(fit.widest, `${width}px`).toBeLessThanOrEqual(fit.room);
    }
  });

  for (const width of [390, 320]) {
    test(`at ${width}px the Read-only box keeps its label, and nothing scrolls sideways`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });
      const label = page.locator('.demo__check', { hasText: 'Read-only' });
      await expect(label).toBeVisible();
      const text = await label.evaluate((element) => {
        const range = document.createRange();
        range.selectNodeContents(element);
        return { width: range.getBoundingClientRect().width, size: parseFloat(getComputedStyle(element).fontSize) };
      });
      expect(text.size).toBeGreaterThan(8);
      expect(text.width).toBeGreaterThan(40);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
    });
  }
});

test.describe('the demo page with schema help', () => {
  setupEditor(test, '?schema=help');

  test('the schema warnings are readable in every theme', async ({ page }) => {
    await expect(page.locator('.demo__schema-warnings li').first()).toBeVisible();
    for (const theme of ['vscode-dark', 'dracula', 'vscode-light', 'solarized-light']) {
      await pickTheme(page, theme);
      expect(await contrastIn(page, '.demo__schema-warnings li'), theme).toBeGreaterThanOrEqual(4.5);
    }
  });
});

test.describe('the documents demo', () => {
  setupEditor(test, '?documents');

  test('its controls take the theme, and it has a theme picker', async ({ page }) => {
    for (const theme of ['vscode-dark', 'nord', 'github-light']) {
      await pickTheme(page, theme);
      await expect(page.locator('.config-editor-documents')).toHaveAttribute('data-color-theme', theme);
      for (const control of ['.demo__settings-toggle', '.demo__save-all']) {
        expect(await contrastIn(page, control), `${theme}: ${control}`).toBeGreaterThanOrEqual(4.5);
        const look = await page.locator(control).evaluate((element) => {
          const style = getComputedStyle(element);
          return { radius: style.borderTopLeftRadius, height: element.getBoundingClientRect().height };
        });
        expect(look.radius).toBe('4px');
        expect(look.height).toBeGreaterThanOrEqual(24);
      }
    }
    // The settings open below their button, over the page.
    await page.locator('.demo__settings-toggle').click();
    await expect(page.locator('.demo__settings-toggle')).toHaveAttribute('aria-expanded', 'true');
    await expect(page.locator('.demo__settings-list')).toBeVisible();
  });
});
