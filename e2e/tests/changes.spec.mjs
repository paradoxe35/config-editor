import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Changes against the baseline: the gutter markers, the footer count,
// "Revert this change" from the preview (mouse, Enter, R), Alt+F5 / Alt+F3,
// and the JS packages' onChangeSummary and unifiedDiff.

const status = (page) => page.locator('.config-editor__change-status');
const preview = (page) => page.locator('.config-editor__diff-peek');
const modified = (page) => page.locator('.config-editor__diff-marker-modified');
const added = (page) => page.locator('.config-editor__diff-marker-added');
const removed = (page) => page.locator('.config-editor__diff-deleted');
const saved = (page) => page.locator('.demo__output-text');

// Puts the caret at the start (or end) of one-based `line` and focuses the text.
async function caretAt(page, line, end = false) {
  await page.locator('.config-editor__input').evaluate(
    (element, [target, atEnd]) => {
      const lines = element.value.split('\n');
      let offset = lines.slice(0, target - 1).reduce((sum, text) => sum + text.length + 1, 0);
      if (atEnd) offset += lines[target - 1].length;
      element.focus();
      element.setSelectionRange(offset, offset);
    },
    [line, end],
  );
}

const lineOf = (text, line) => text.split('\n')[line - 1];
const textareaFocused = (page) =>
  page.evaluate(() => document.activeElement?.classList.contains('config-editor__input'));

for (const language of ['yaml', 'json', 'toml', 'hcl', 'xml']) {
  test.describe(`${language}: changes against the baseline`, () => {
    setupEditor(test, `?language=${language}&sample=large`);

    test('added, modified and removed lines are marked, counted, and reverted one by one', async ({ page }) => {
      const e = editor(page);
      const original = await e.value();
      // Modified: a letter at the start of line 3.
      await caretAt(page, 3);
      await page.keyboard.type('x');
      await page.keyboard.press('Escape');
      // Removed: line 8 cut whole.
      await caretAt(page, 8);
      await page.keyboard.press('Control+X');
      // Added: a new line after line 14.
      await caretAt(page, 14, true);
      await page.keyboard.press('Enter');
      await page.keyboard.type('added_line');
      await page.keyboard.press('Escape');

      await expect(modified(page)).toHaveCount(1);
      await expect(removed(page)).toHaveCount(1);
      await expect(added(page)).toHaveCount(1);
      await expect(status(page)).toHaveText('3 changes');
      await expect(status(page)).toHaveAttribute('title', /1 added, 1 modified, 1 removed lines/);

      // The modified line, with the mouse: one edit, undone in one step.
      await modified(page).click();
      await expect(preview(page)).toHaveCount(1);
      await preview(page).getByRole('button', { name: /Revert this change/ }).click();
      await expect(preview(page)).toHaveCount(0);
      await expect(modified(page)).toHaveCount(0);
      expect(lineOf(await e.value(), 3)).toBe(lineOf(original, 3));
      expect(await textareaFocused(page)).toBe(true);
      await expect(status(page)).toHaveText('2 changes');
      await page.keyboard.press('Control+Z');
      await expect(modified(page)).toHaveCount(1);
      await expect(status(page)).toHaveText('3 changes');
      await page.keyboard.press('Control+Shift+Z');
      await expect(status(page)).toHaveText('2 changes');

      // The removed line, from the keyboard: Alt+F3 opens its preview with
      // the focus on Revert, and Enter reverts.
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Alt+F3');
      await expect(preview(page)).toContainText('Removed lines');
      await expect(preview(page).getByRole('button', { name: /Revert this change/ })).toBeFocused();
      await page.keyboard.press('Enter');
      await expect(removed(page)).toHaveCount(0);
      await expect(status(page)).toHaveText('1 change');

      // The added line: Alt+F3 again, and R.
      await page.keyboard.press('Alt+F3');
      await expect(preview(page)).toContainText('Added line');
      await page.keyboard.press('r');
      await expect(added(page)).toHaveCount(0);
      await expect(status(page)).toHaveCount(0);
      expect(await e.value()).toBe(original);
      await expect(saved(page)).toHaveText(original, { useInnerText: false });
    });
  });
}

test.describe('moving between changes', () => {
  setupEditor(test, '?sample=large');

  test('Alt+F5 and Shift+Alt+F5 walk the changes and wrap, far down a large document', async ({ page }) => {
    const e = editor(page);
    await caretAt(page, 5);
    await page.keyboard.type('x');
    await page.keyboard.press('Escape');
    await caretAt(page, 1500);
    await page.keyboard.type('y');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F5');
    expect((await e.caret()).line).toBe(5);
    await page.keyboard.press('Alt+F5');
    expect((await e.caret()).line).toBe(1500);
    // The far change is in view, its marker rendered.
    await expect(modified(page)).toHaveCount(1);
    await expect(modified(page)).toBeInViewport();
    await page.keyboard.press('Alt+F5');
    expect((await e.caret()).line).toBe(5);
    await page.keyboard.press('Shift+Alt+F5');
    expect((await e.caret()).line).toBe(1500);
    // The footer count moves to the next change too.
    await status(page).click();
    expect((await e.caret()).line).toBe(5);
    expect(await textareaFocused(page)).toBe(true);
  });

  test('Escape closes a preview opened from the keyboard and gives the text the focus back', async ({ page }) => {
    await caretAt(page, 5);
    await page.keyboard.type('x');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F3');
    await expect(preview(page)).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(preview(page)).toHaveCount(0);
    expect(await textareaFocused(page)).toBe(true);
    await expect(status(page)).toHaveText('1 change');
  });
});

test.describe('reverting while editing as another language', () => {
  setupEditor(test, '?edit=json');

  test('the reverted change reaches the host in its own language', async ({ page }) => {
    const e = editor(page);
    const host = await saved(page).textContent();
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('7835');
      element.focus();
      element.setSelectionRange(at, at + 4);
    });
    await page.keyboard.type('9999');
    await expect(saved(page)).toContainText('listen: 9999');
    await modified(page).click();
    await preview(page).getByRole('button', { name: /Revert this change/ }).click();
    await expect(e.input).toHaveValue(/"listen": 7835/);
    await expect(saved(page)).toContainText('listen: 7835');
    await expect.poll(() => saved(page).textContent()).toBe(host);
    await expect(status(page)).toHaveCount(0);
  });
});

test.describe('read-only', () => {
  setupEditor(test, '?readonly');

  test('the change keys do nothing without changes and never edit', async ({ page }) => {
    const e = editor(page);
    const before = await e.value();
    await e.input.click();
    await page.keyboard.press('Alt+F5');
    await page.keyboard.press('Alt+F3');
    await expect(preview(page)).toHaveCount(0);
    expect(await e.value()).toBe(before);
  });
});

// The npm packages, through their example pages (built with `npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('js packages', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  async function open(page, name) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await page.locator('.config-editor__input').waitFor();
    return errors;
  }

  test('vanilla: onChangeSummary, unifiedDiff on the instance and as a function', async ({ page }) => {
    const errors = await open(page, 'vanilla.html');
    await page.evaluate(async () => {
      const module = await import('../config-editor/pkg/config_editor.js');
      window.editor.destroy();
      window.summaries = [];
      window.module = module;
      window.target = new module.ConfigEditor(document.getElementById('editor'), {
        value: 'a: 1\nb: 2\nc: 3\n',
        onChangeSummary: (summary) => window.summaries.push(summary),
      });
    });
    await expect.poll(() => page.evaluate(() => window.summaries.at(-1))).toEqual({
      added: 0,
      removed: 0,
      modified: 0,
      hunks: 0,
    });
    await caretAt(page, 2, true);
    await page.keyboard.type('0');
    await expect.poll(() => page.evaluate(() => window.summaries.at(-1))).toEqual({
      added: 0,
      removed: 0,
      modified: 1,
      hunks: 1,
    });
    expect(await page.evaluate(() => window.target.unifiedDiff())).toBe(
      '--- baseline\n+++ current\n@@ -1,3 +1,3 @@\n a: 1\n-b: 2\n+b: 20\n c: 3\n',
    );
    expect(await page.evaluate(() => window.target.unifiedDiff(0))).toBe(
      '--- baseline\n+++ current\n@@ -2 +2 @@\n-b: 2\n+b: 20\n',
    );
    expect(await page.evaluate(() => window.module.unifiedDiff('x\n', 'x\n'))).toBe('');
    expect(await page.evaluate(() => window.module.unifiedDiff('', 'y'))).toBe(
      '--- baseline\n+++ current\n@@ -0,0 +1 @@\n+y\n\\ No newline at end of file\n',
    );
    const error = await page.evaluate(() => {
      try {
        window.target.unifiedDiff(-1);
        return null;
      } catch (thrown) {
        return String(thrown.message ?? thrown);
      }
    });
    expect(error).toContain('context must be a whole number');
    // setBaseline to the text clears the count; setCallbacks replaces it.
    await page.evaluate(() => window.target.setBaseline(window.target.value));
    await expect.poll(() => page.evaluate(() => window.summaries.at(-1).hunks)).toBe(0);
    await page.evaluate(() => {
      window.replaced = [];
      window.target.setCallbacks({ onChangeSummary: (summary) => window.replaced.push(summary.hunks) });
    });
    await page.keyboard.type('0');
    await expect.poll(() => page.evaluate(() => window.replaced.at(-1))).toBe(1);
    expect(errors).toEqual([]);
  });

  test('react: onChangeSummary and ref.unifiedDiff', async ({ page }) => {
    const errors = await open(page, 'react.html');
    await caretAt(page, 1, true);
    await page.keyboard.type('X');
    await expect(page.locator('#changes')).toHaveText('1 changes: +0 ~1 -0');
    const patch = await page.evaluate(() => window.editorHandle.current.unifiedDiff(0));
    expect(patch).toMatch(/^--- baseline\n\+\+\+ current\n@@ -1 \+1 @@\n-.*\n\+.*X\n$/);
    expect(errors).toEqual([]);
  });
});
