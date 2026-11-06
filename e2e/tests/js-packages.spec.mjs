import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The npm packages in js/: the vanilla `config-editor` (wasm-bindgen) and the
// `config-editor-react` wrapper, through their example pages. Build them first
// with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');
const saved = (browserPage) => browserPage.locator('#saved');

for (const name of ['vanilla.html', 'react.html']) {
  test.describe(name, () => {
    test('typing reaches onChange, suggestions work, and nothing logs errors', async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('lim');
      await expect
        .poll(() => browserPage.locator('.config-editor__completion-key').allTextContents())
        .toContain('limits');
      await browserPage.keyboard.press('Escape');
      await browserPage.keyboard.type('its:');
      await expect(saved(browserPage)).toContainText('limits:');
      expect(errors).toEqual([]);
    });

    test('schema problems are marked', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('bogus_key: 1');
      await expect(browserPage.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    });

    test('edit as switches live and saves back in the host language', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.locator('#edit-as').selectOption('json');
      await expect(input(browserPage)).toHaveValue(/"listen": 7835/);
      await input(browserPage).evaluate((element) => {
        const at = element.value.indexOf('7835');
        element.focus();
        element.setSelectionRange(at, at + 4);
      });
      await browserPage.keyboard.type('9999');
      await expect(saved(browserPage)).toContainText('listen: 9999');
      await expect(saved(browserPage)).toContainText('# Burrow server configuration');
    });

    test('value, theme and read-only update live', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.locator('#replace').click();
      await expect(input(browserPage)).toHaveValue('listen: 9000\n');
      await browserPage.locator('#theme').selectOption('nord');
      await expect(browserPage.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
      await browserPage.locator('#read-only').check();
      await input(browserPage).click();
      await browserPage.keyboard.type('x');
      await expect(input(browserPage)).toHaveValue('listen: 9000\n');
    });
  });
}

test.describe('vanilla.html', () => {
  test('a language registered from JavaScript edits, validates and converts', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#edit-as').selectOption('env');
    await expect(input(browserPage)).toHaveValue(/listen=7835/);
    await expect(browserPage.locator('.config-editor__key').first()).toBeVisible();
    await input(browserPage).evaluate((element) => {
      const at = element.value.indexOf('false');
      element.focus();
      element.setSelectionRange(at, at + 5);
    });
    // `insecure_plaintext` is a boolean: schema validation, placed through
    // the JS language's key positions.
    await browserPage.keyboard.type('maybe');
    await expect(browserPage.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
  });

  test('destroy removes the editor', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await browserPage.locator('#destroy').click();
    await expect(input(browserPage)).toHaveCount(0);
  });

  test('headless validation is exposed', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    const problems = await browserPage.evaluate(() => window.configEditor.validate('json', '{"a": }'));
    expect(problems).toHaveLength(1);
    expect(problems[0].severity).toBe('error');
  });
});

test.describe('react.html', () => {
  test('a colors prop overrides the theme', async ({ page: browserPage }) => {
    await open(browserPage, 'react.html');
    const style = await browserPage.locator('.config-editor').first().getAttribute('style');
    expect(style).toContain('--config-editor-key: #ffcc66');
  });

  test('onEditAsChange reports the switch', async ({ page: browserPage }) => {
    await open(browserPage, 'react.html');
    await browserPage.locator('#edit-as').selectOption('toml');
    await expect(browserPage.locator('#editing')).toHaveText('toml');
  });
});
