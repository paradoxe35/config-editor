import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// While an input method composes, its keys and its text are its own: the
// editor takes no key (Enter does not accept a suggestion over the composed
// text, Tab does not indent), and the composition is applied once, when it
// ends (each update written back mid-composition cost seconds on a long
// text, and over part of a secret placeholder landed twice). Round 6's
// `D6.a11y-1`, round 5's `D5.ux-1` and round 6's `D6.perf-1`.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

const SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    mode: { type: 'string', enum: ['fast', 'safe'] },
    metrics: { type: 'boolean' },
    a: { type: 'integer' },
  },
});

async function mount(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await page.evaluate(async (options) => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.changes = [];
    window.qa = new module.ConfigEditor(host, {
      ...options,
      onChange: (text) => window.changes.push(text),
    });
  }, options);
  await input(page).waitFor();
  return page.context().newCDPSession(page);
}

async function caretAt(page, start, end = start) {
  await input(page).evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [start, end],
  );
}

const value = (page) => page.evaluate(() => window.qa.value);

test.describe('keys during an IME composition', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const key of ['Enter', 'Tab', 'ArrowDown', 'Escape']) {
    test(`${key} is the input method's, not the editor's`, async ({ page }) => {
      const cdp = await mount(page, {
        language: 'yaml',
        value: 'a: 1\n',
        schema: SCHEMA,
      });
      await caretAt(page, 5);
      await cdp.send('Input.imeSetComposition', {
        text: 'm',
        selectionStart: 1,
        selectionEnd: 1,
      });
      await page.keyboard.press(key);
      await cdp.send('Input.insertText', { text: 'm' });
      await page.waitForTimeout(400);
      const text = await value(page);
      // No suggestion accepted over the composed text, no indent added.
      expect(text).not.toContain('mode');
      expect(text).not.toContain('metrics');
      expect(text).not.toMatch(/\n\s+m/);
      expect(text).toContain('m');
    });
  }
});

test.describe('a composition is applied once, when it ends', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('the document waits for the commit, then takes it once', async ({ page }) => {
    const cdp = await mount(page, { language: 'yaml', value: 'name: x\n' });
    await caretAt(page, 7);
    for (const text of ['n', 'ni', 'に', 'にほ']) {
      await cdp.send('Input.imeSetComposition', {
        text,
        selectionStart: text.length,
        selectionEnd: text.length,
      });
    }
    await page.waitForTimeout(300);
    expect(await value(page)).toBe('name: x\n');
    await cdp.send('Input.insertText', { text: '日本' });
    await expect.poll(() => value(page)).toBe('name: x日本\n');
    // One undo step takes the whole composition back.
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe('name: x\n');
  });

  test('a composition over part of a secret placeholder replaces it once', async ({ page }) => {
    const text = 'name: edge\nkey: «secret 3f2a»\n';
    const cdp = await mount(page, {
      language: 'yaml',
      value: text,
      schema: JSON.stringify({
        type: 'object',
        properties: { key: { type: 'string', writeOnly: true } },
      }),
    });
    const at = text.indexOf('«secret');
    await caretAt(page, at + 2, at + 8);
    await cdp.send('Input.imeSetComposition', {
      text: 'ü',
      selectionStart: 1,
      selectionEnd: 1,
    });
    await cdp.send('Input.insertText', { text: 'ü' });
    await expect.poll(() => value(page)).toBe('name: edge\nkey: ü\n');
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe(text);
  });
});
