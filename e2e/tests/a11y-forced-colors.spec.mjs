import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Forced colors (Windows high contrast): every mark, severity, tab and suggestion stays visible.

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(vanilla(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  // A host's info over the whole file, an error on `listen`.
  const problems = ({ value, line, column }) => [
    { range: { startLine: 1, startColumn: 1, endLine: value.split('\n').length, endColumn: 1 }, severity: 'info', message: 'This file is generated' },
    { range: { startLine: line, startColumn: column, endColumn: column + 6 }, severity: 'error', message: 'listen is taken' },
  ];

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('U21-3: forced colors keep every mark visible', () => {
    test.use({ viewport: { width: 900, height: 500 } });

    const schema = JSON.stringify({
      type: 'object',
      properties: { port: { type: 'integer' }, host: { type: 'string' }, alpha: { type: 'string' }, beta: { type: 'boolean' } },
    });
    const value = 'port: 1\nhost: example\nk1: a\nk2: b\nk1x: c\nblock:\n  a: 1\n  b: 2\n';

    // Whether `locator` (its first match) changes what is drawn in its box
    // when `change` undoes it there: hidden, or its marking class removed.
    async function shows(page, locator, change) {
      const element = locator.first();
      await expect(element).toBeVisible();
      const box = await element.boundingBox();
      const clip = { x: Math.max(0, box.x - 1), y: Math.max(0, box.y - 1), width: box.width + 2, height: box.height + 2 };
      await page.waitForTimeout(150);
      const before = await page.screenshot({ clip, animations: 'disabled' });
      await element.evaluate((element, change) => {
        if (change.remove) element.classList.remove(change.remove);
        else element.style.visibility = 'hidden';
      }, change);
      const after = await page.screenshot({ clip, animations: 'disabled' });
      return !before.equals(after);
    }
    const hide = {};
    const unmark = (name) => ({ remove: name });

    for (const forcedColors of ['none', 'active']) {
      test(`forced colors ${forcedColors}: matches, the current match, the active suggestion, the lock, a fold's problem mark and an on toggle show`, async ({ browser }) => {
        const context = await browser.newContext({ forcedColors, colorScheme: 'dark', viewport: { width: 900, height: 500 } });
        const page = await context.newPage();
        const errors = await open(page, { value, schema, readOnlyPaths: ['/host'] });
        const invisible = [];
        const check = async (what, locator, change) => {
          if (!(await shows(page, locator, change))) invisible.push(what);
        };
        await check('the gutter lock', page.locator('.config-editor__lock-marker'), hide);
        // The active suggestion, told from the other rows.
        await input(page).click();
        await page.keyboard.press('Control+End');
        await page.keyboard.press('Control+Space');
        await expect(page.locator('.config-editor__completion-active')).toBeVisible();
        await check('the active suggestion', page.locator('.config-editor__completion-active'), unmark('config-editor__completion-active'));
        await page.keyboard.press('Escape');
        // Find: every match, and the current one told from the others.
        await page.keyboard.press('Control+f');
        await page.keyboard.type('k1');
        await expect(page.locator('.config-editor__search-match')).toHaveCount(2);
        await check('a find match', page.locator('.config-editor__search-match:not(.config-editor__search-match-current)'), hide);
        await check('the current find match', page.locator('.config-editor__search-match-current'), unmark('config-editor__search-match-current'));
        await page.locator('.config-editor__find-toggle').first().click();
        await check('a find toggle that is on', page.locator('.config-editor__find-toggle-on'), unmark('config-editor__find-toggle-on'));
        await page.keyboard.press('Escape');
        // A closed fold hiding a problem: its chip's mark.
        await setHost(page, [{ range: { startLine: 7, startColumn: 3, endColumn: 4 }, severity: 'error', message: 'hidden' }]);
        await input(page).focus();
        await page.keyboard.press('Control+End');
        await page.keyboard.press('ArrowUp');
        await page.keyboard.press('ArrowUp');
        await page.keyboard.press('ArrowUp');
        await page.keyboard.press('Control+Shift+BracketLeft');
        await check("a fold's problem mark", page.locator('.config-editor__fold-chip--error'), unmark('config-editor__fold-chip--error'));
        expect(invisible).toEqual([]);
        expect(errors).toEqual([]);
        await context.close();
      });
    }
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(vanilla(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  const schema = JSON.stringify({
    type: 'object',
    properties: {
      config: {
        type: 'object',
        properties: { old: { type: 'integer', deprecated: true, description: 'Old thing' }, port: { type: 'integer' } },
      },
    },
  });

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('L22-U7: forced colors keep the severities apart', () => {
    test('errors, warnings and infos have their own underline style', async ({ browser }) => {
      const context = await browser.newContext({ forcedColors: 'active', viewport: { width: 900, height: 400 } });
      const page = await context.newPage();
      const errors = await open(page, { value: 'a: 1\nb: 2\nc: 3\n', schema: '{}' });
      const at = (line, severity) => ({ range: { startLine: line, startColumn: 1, endColumn: 2 }, severity, message: severity });
      await setHost(page, [at(1, 'error'), at(2, 'warning'), at(3, 'info')]);
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(3);
      const styles = await page.evaluate(() =>
        ['error', 'warning', 'info'].map(
          (message) => getComputedStyle(document.querySelector(`.config-editor__diagnostic-marker[data-message="${message}"]`)).textDecorationStyle,
        ),
      );
      expect(new Set(styles).size).toBe(3);
      expect(errors).toEqual([]);
      await context.close();
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

  const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);

  async function open(page, name, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example(name, options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).first().waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('M23-A4: forced colors', () => {
    test.use({ forcedColors: 'active' });

    test('the selected tab has a system-color bar, closable or not', async ({ page }) => {
      const errors = await open(page, 'documents.html');
      const highlight = await page.evaluate(() => {
        const probe = document.createElement('div');
        probe.style.forcedColorAdjust = 'none';
        probe.style.backgroundColor = 'Highlight';
        document.body.append(probe);
        const color = getComputedStyle(probe).backgroundColor;
        probe.remove();
        return color;
      });
      const bar = (id) =>
        tab(page, id).evaluate((element) => {
          const style = getComputedStyle(element, '::before');
          return style.content === 'none' ? null : style.backgroundColor;
        });
      for (const [id, other] of [
        ['main', 'limits'],
        ['limits', 'main'],
      ]) {
        await tab(page, id).click();
        await expect(tab(page, id)).toHaveAttribute('aria-selected', 'true');
        expect(await bar(id), id).toBe(highlight);
        expect(await bar(other), other).toBe(null);
      }
      expect(errors).toEqual([]);
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

  test.describe('L24-A6: forced colors', () => {
    test.use({ forcedColors: 'active', colorScheme: 'dark' });

    test('every part of the selected suggestion is HighlightText', async ({ page }) => {
      const errors = await open(page, {
        value: '',
        schema: JSON.stringify({
          type: 'object',
          properties: {
            port: { type: 'integer', description: 'Port to listen on.' },
            mode: { enum: ['fast', 'safe'], description: 'Routing mode.' },
            old: { type: 'string', deprecated: true, description: 'Old key.' },
          },
        }),
      });
      const highlightText = await page.evaluate(() => {
        const probe = document.createElement('span');
        probe.style.forcedColorAdjust = 'none';
        probe.style.color = 'HighlightText';
        document.body.append(probe);
        const color = getComputedStyle(probe).color;
        probe.remove();
        return color;
      });
      await input(page).focus();
      await page.keyboard.press('Control+Space');
      const options = page.locator('.config-editor__completion');
      await expect(options).toHaveCount(3);
      for (let row = 0; row < 3; row += 1) {
        const active = page.locator('.config-editor__completion-active');
        await expect(active).toHaveCount(1);
        const colors = await active.evaluate((option) =>
          [...option.querySelectorAll('*')]
            .filter((element) => [...element.childNodes].some((node) => node.nodeType === 3 && node.textContent.trim()))
            .map((element) => [element.className, getComputedStyle(element).color]),
        );
        expect(colors.length).toBeGreaterThan(1);
        for (const [name, color] of colors) expect(color, `row ${row}: ${name}`).toBe(highlightText);
        await page.keyboard.press('ArrowDown');
      }
      expect(errors).toEqual([]);
    });
  });
});
