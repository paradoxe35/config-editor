import { expect } from '@playwright/test';

// The framework packages' example pages (js/examples): the same controls,
// ids, query flags and `window` hooks on each, so a test of one runs on all.
export const FRAMEWORK_PAGES = ['react.html', 'vue.html', 'svelte.html'];
export const DOCUMENTS_FRAMEWORK_PAGES = ['documents-react.html', 'documents-vue.html', 'documents-svelte.html'];

// Registers the per-test setup shared by every editor spec: a fresh page,
// collection of page errors, and a failure if the page threw anything.
// `query` selects the demo mode, for example '?language=json'.
export function setupEditor(test, query = '') {
  test.beforeEach(async ({ page, baseURL }) => {
    page.errors = [];
    page.on('pageerror', (error) => page.errors.push(String(error)));
    page.on('console', (message) => {
      if (message.type() === 'error') page.errors.push(message.text());
    });
    await page.goto(`${baseURL}${query}`);
    await page.locator('.config-editor__input').waitFor();
  });

  test.afterEach(async ({ page }) => {
    expect(page.errors, 'no page errors').toEqual([]);
  });
}

// Drives the editor the way a user does: keyboard, mouse, and paste events.
export function editor(page) {
  const input = page.locator('.config-editor__input');
  const popup = page.locator('.config-editor__completions');
  const options = popup.locator('.config-editor__completion');

  const keys = () => popup.locator('.config-editor__completion-key').allTextContents();
  const isOpen = async () => !(await popup.getAttribute('class')).includes('hidden');
  const diagnosticCount = () => page.locator('.config-editor__diagnostic-marker').count();

  // Replaces the whole document by typing it, so auto-indent and pairing run.
  const typeDoc = async (text) => {
    await input.click();
    await page.keyboard.press('Control+A');
    await page.keyboard.press('Delete');
    if (text) await page.keyboard.type(text);
    await page.waitForTimeout(120);
  };

  // Replaces the whole document as a paste does: no suggestions, no auto-indent.
  const paste = async (text) => {
    await input.evaluate((element, value) => {
      element.focus();
      element.setSelectionRange(0, element.value.length);
      element.setRangeText(value, 0, element.value.length, 'end');
      element.dispatchEvent(
        new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: value }),
      );
    }, text);
    await page.waitForTimeout(120);
  };

  const clickKey = async (label) => {
    const index = await options.evaluateAll(
      (buttons, wanted) =>
        buttons.findIndex((b) => b.querySelector('.config-editor__completion-key').textContent === wanted),
      label,
    );
    expect(index, `option "${label}" is listed; have ${JSON.stringify(await keys())}`).toBeGreaterThanOrEqual(0);
    await options.nth(index).click();
    await page.waitForTimeout(120);
  };

  const highlighted = () =>
    options.evaluateAll((buttons) =>
      buttons
        .filter((b) => {
          const bg = getComputedStyle(b).backgroundColor;
          return bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent';
        })
        .map((b) => b.querySelector('.config-editor__completion-key').textContent),
    );

  const value = () => input.inputValue();

  // Caret line/column (1-based) plus whether the selection is collapsed.
  const caret = () =>
    input.evaluate((element) => {
      const position = element.selectionStart;
      const before = element.value.slice(0, position);
      return {
        line: before.split('\n').length,
        column: position - (before.lastIndexOf('\n') + 1) + 1,
        collapsed: element.selectionStart === element.selectionEnd,
      };
    });

  const screenshot = (name) =>
    page.screenshot({ path: `test-results/screens/${name}.png` });

  return {
    input,
    popup,
    options,
    keys,
    isOpen,
    diagnosticCount,
    typeDoc,
    paste,
    clickKey,
    highlighted,
    value,
    caret,
    screenshot,
  };
}

// Page coordinates of the character cell at (line, column) in the editor textarea.
export async function pointAt(page, line, column) {
  const input = page.locator('.config-editor__input');
  const box = await input.boundingBox();
  const metrics = await input.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      left: parseFloat(style.paddingLeft),
      top: parseFloat(style.paddingTop),
      fontSize: parseFloat(style.fontSize),
      lineHeight: parseFloat(style.lineHeight),
      scrollTop: element.scrollTop,
    };
  });
  return {
    x: box.x + metrics.left + (column + 0.5) * metrics.fontSize * 0.6,
    y: box.y + metrics.top + (line + 0.5) * metrics.lineHeight - metrics.scrollTop,
  };
}

// Scrolls the editor so that `line` is inside the visible textarea.
export async function scrollToLine(page, line) {
  await page.locator('.config-editor__input').evaluate((element, target) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    element.scrollTop = Math.max(0, target * lineHeight - element.clientHeight / 2);
    element.dispatchEvent(new Event('scroll'));
  }, line);
  await page.waitForTimeout(80);
}
