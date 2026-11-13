import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Typing on a focused document tab types into its editor, at its caret,
// every character kept. Before, letters were dropped until a Space moved
// the focus into the editor, and the rest landed at the editor's old caret
// (round 1's `D1.ux-1`).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const tab = (page, id) => page.locator(`#qa .config-editor-documents__tab[data-document-id="${id}"]`);

async function open(page) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await page.evaluate(async () => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.view = new module.ConfigDocuments(host, {
      documents: [
        { id: 'main', fileName: 'main.yaml', value: 'a: 1\n' },
        { id: 'limits', fileName: 'limits.toml', value: '[limits]\n' },
      ],
    });
  });
  await expect(tab(page, 'main')).toBeVisible();
}

const focusedInput = (page) => page.evaluate(() => document.activeElement?.classList.contains('config-editor__input') ?? false);

test.describe('typing on a document tab', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, phrase] of Object.entries({
    'a phrase with spaces': 'max_pending = 1024',
    'a word': 'max_pending',
  })) {
    test(`${name} is typed whole into the clicked tab's editor`, async ({ page }) => {
      await open(page);
      await tab(page, 'limits').click();
      await expect(tab(page, 'limits')).toHaveAttribute('aria-selected', 'true');
      await page.keyboard.type(phrase, { delay: 30 });
      expect(await focusedInput(page)).toBe(true);
      // The editor's caret was at the start of the document.
      await expect.poll(() => page.evaluate(() => window.view.editor('limits').text)).toBe(`${phrase}[limits]\n`);
    });
  }

  test('a tab reached by the keyboard types into its editor too', async ({ page }) => {
    await open(page);
    await tab(page, 'main').click();
    await page.keyboard.press('ArrowRight');
    await expect(tab(page, 'limits')).toBeFocused();
    await page.keyboard.type('x y', { delay: 30 });
    await expect.poll(() => page.evaluate(() => window.view.editor('limits').text)).toBe('x y[limits]\n');
  });

  test('Space and Enter still only move into the editor', async ({ page }) => {
    await open(page);
    await tab(page, 'limits').click();
    await page.keyboard.press('Space');
    await expect.poll(() => focusedInput(page)).toBe(true);
    expect(await page.evaluate(() => window.view.editor('limits').text)).toBe('[limits]\n');
    await tab(page, 'main').click();
    await page.keyboard.press('Enter');
    await expect.poll(() => focusedInput(page)).toBe(true);
    expect(await page.evaluate(() => window.view.editor('main').text)).toBe('a: 1\n');
  });
});
