import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// The demo and example pages: themes, the toolbar, the Settings dropdown and its checkboxes, the
// schema warnings, the documents demo.

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

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (page) => page.locator('.config-editor__input');

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

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

// The suggestion list as screen readers follow it, the change
// preview on phones, the go-to-line field, touch targets, and the demo page
// (its controls, its theme on every part of the page, phone widths).
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

// The demo's Settings dropdown inside the window at every
// width and closing like a menu, the toolbar on one row while editing as
// another language, Alt+Enter with no docs link, and touch targets (the
// fold chip, the demo's checkboxes).
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

// The demo's Settings list closing when focus moves on and
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

test.describe('the vanilla example page', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('U19 low: its theme select shows the theme given in ?options', async ({ page }) => {
    const errors = await open(page, { theme: 'nord' });
    await expect(page.locator('#theme')).toHaveValue('nord');
    expect(errors).toEqual([]);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the demo tab is as wide as its name', () => {
    test('at 2560 px', async ({ browser, baseURL }) => {
      const context = await browser.newContext({ viewport: { width: 2560, height: 800 } });
      const page = await context.newPage();
      await page.goto(`${baseURL}?language=yaml`);
      await page.locator('.config-editor__input').waitFor();
      const spare = await page.locator('.demo__toolbar .config-editor__file').evaluate((file) => {
        const used = [...file.children].reduce((sum, child) => sum + child.getBoundingClientRect().width, 0);
        const gaps = parseFloat(getComputedStyle(file).columnGap) || 0;
        return file.getBoundingClientRect().width - used - gaps * (file.children.length - 1);
      });
      // A few pixels of font rounding (CI's fonts differ); the bug left 1480.
      expect(spare).toBeLessThan(24);
      await context.close();
    });
  });
});

test.describe("the demo's own toolbar under the host shield", () => {
  setupEditor(test, '');

  test("keeps the page's rules for its contents", async ({ page }) => {
    await page.addStyleTag({ content: 'span { letter-spacing: 3px; }' });
    const spacing = (selector) => page.locator(selector).first().evaluate((element) => getComputedStyle(element).letterSpacing);
    // The host's toolbar (`ConfigEditorToolbar`): the page's.
    expect(await spacing('.demo__toolbar .demo__picker-label')).toBe('3px');
  });
});

// A press on a settings label's words toggles it
// and keeps the list open; the checkboxes keep their size in wrapping
// rows; the example pages have a heading, a main landmark and 24px
// controls; the problem summary's live region is written only when
// its text changes.
for (const [how, touch] of [['click', false], ['tap', true]]) {
  test.describe(`a ${how} on a setting's words`, () => {
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

test.describe('the settings checkboxes', () => {
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
