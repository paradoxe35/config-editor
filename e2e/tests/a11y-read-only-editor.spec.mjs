import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setupEditor } from './helpers.mjs';

// A read-only editor: typing, editing commands, line commands and Ctrl+M say it cannot edit and change nothing.

const input = (page) => page.locator('.config-editor__input');

const said = (page) => page.locator('.config-editor__announcement').first();

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const announcement = (page) => page.locator('.config-editor__announcement').first();

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('L25-A5: typing in a read-only editor', () => {
    for (const [what, act] of [
      ['a character', (page) => page.keyboard.type('x')],
      ['Backspace', (page) => page.keyboard.press('Backspace')],
      ['Enter', (page) => page.keyboard.press('Enter')],
    ]) {
      test(`${what}: says it cannot edit, at the caret, and changes nothing`, async ({ page }) => {
        const value = 'name: a\nport: 1\n';
        const errors = await open(page, { value, readOnly: true });
        await input(page).focus();
        await page.keyboard.press('Control+End');
        await act(page);
        await expect(announcement(page)).toHaveText(/read-only/);
        const notice = page.locator('.config-editor__read-only-notice');
        await expect(notice).toBeVisible();
        await expect(notice).toHaveText(/read-only/);
        expect(await input(page).inputValue()).toBe(value);
        // A move of the caret puts it away.
        await page.keyboard.press('ArrowUp');
        await expect(notice).toBeHidden();
        expect(errors).toEqual([]);
      });
    }
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input:visible').first();

  /** What the editor with the visible text says through its live region. */
  const said = (page) => input(page).evaluate((element) => element.closest('.config-editor').querySelector('.config-editor__announcement').textContent);

  const tip = (page) => page.locator('.config-editor__problem-tip');

  async function open(page, options, name = 'vanilla.html') {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  /** Where the caret is: one-based line and UTF-16 column. */
  const caretAt = (page) =>
    input(page).evaluate((element) => {
      const lines = element.value.slice(0, element.selectionStart).split('\n');
      return `${lines.length}:${lines.at(-1).length + 1}`;
    });

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('N26-A3: editing commands in a read-only editor say it cannot edit', () => {
    const COMMANDS = {
      'Ctrl+X without a selection (cut the line)': 'Control+x',
      'Ctrl+/ (toggle a comment)': 'Control+Slash',
      'Ctrl+Enter (a line below)': 'Control+Enter',
      'Ctrl+Shift+Enter (a line above)': 'Control+Shift+Enter',
    };
    for (const where of ['ConfigEditor', 'documents']) {
      for (const [command, key] of Object.entries(COMMANDS)) {
        // The read-only document is JSON, which has no comments to toggle.
        if (where === 'documents' && key === 'Control+Slash') continue;
        test(`${where}, ${command}: says so, at the caret, and changes nothing`, async ({ page, context }) => {
          await context.grantPermissions(['clipboard-read', 'clipboard-write']);
          const errors =
            where === 'documents'
              ? await open(page, undefined, 'documents.html')
              : await open(page, { value: 'a: 1\nb: 2\n', readOnly: true });
          if (where === 'documents') {
            await page.getByRole('tab', { name: /clients/ }).click();
          }
          const value = await input(page).inputValue();
          await input(page).focus();
          await page.keyboard.press('Control+Home');
          await page.keyboard.press('ArrowDown');
          const at = await caretAt(page);
          await page.keyboard.press(key);
          await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
          const notice = page.locator('.config-editor__read-only-notice:visible');
          await expect(notice).toBeVisible();
          await expect(notice).toHaveText(/read-only/);
          expect(await input(page).inputValue()).toBe(value);
          expect(await caretAt(page)).toBe(at);
          expect(errors).toEqual([]);
        });
      }
    }

    test('undo after the host made the editor read-only: says so, and undoes nothing', async ({ page }) => {
      const errors = await open(page, { value: 'a: 1\n' });
      await input(page).focus();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('b: 2\n');
      await page.evaluate(() => window.editor.update({ readOnly: true }));
      const value = await input(page).inputValue();
      await page.keyboard.press('Control+z');
      await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
      await expect(page.locator('.config-editor__read-only-notice')).toBeVisible();
      expect(await input(page).inputValue()).toBe(value);
      expect(errors).toEqual([]);
    });

    test('F8 still walks the problems', async ({ page }) => {
      const schema = JSON.stringify({ type: 'object', properties: { b: { type: 'integer' } } });
      const errors = await open(page, { value: 'a: 1\nb: x\n', schema, readOnly: true });
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      await expect(tip(page)).toBeVisible();
      expect(await caretAt(page)).toMatch(/^2:/);
      expect(errors).toEqual([]);
    });
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options, name = 'vanilla') =>
    `${pathToFileURL(join(js, 'examples', `${name}.html`)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const said = (page) =>
    input(page).evaluate((element) => element.closest('.config-editor').querySelector('.config-editor__announcement').textContent);

  async function open(page, options, name) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(options, name));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  // L28-A2: the line commands did nothing at all, so next to a lock or in a
  // read-only editor they said nothing either.
  const COMMANDS = {
    'Alt+↑': 'Alt+ArrowUp',
    'Alt+↓': 'Alt+ArrowDown',
    'Shift+Alt+↑': 'Shift+Alt+ArrowUp',
    'Shift+Alt+↓': 'Shift+Alt+ArrowDown',
    'Ctrl+Shift+K': 'Control+Shift+K',
    'Ctrl+]': 'Control+BracketRight',
    'Ctrl+[': 'Control+BracketLeft',
  };

  async function caretOnLine2(page) {
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('L28-A2', () => {
    for (const [command, key] of Object.entries(COMMANDS)) {
      test(`a read-only editor: ${command} says it cannot edit`, async ({ page }) => {
        const errors = await open(page, { value: 'a: 1\nb: 2\n', readOnly: true });
        await caretOnLine2(page);
        await page.keyboard.press(key);
        await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
        await expect(page.locator('.config-editor__read-only-notice:visible')).toBeVisible();
        await expect(input(page)).toHaveValue('a: 1\nb: 2\n');
        expect(errors).toEqual([]);
      });
    }
  });
});

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
