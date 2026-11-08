import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointerOn } from './helpers.mjs';

// How problems are shown (v0.1 plan, A; QA 18 UX): a deprecated key struck
// through, a host range over several lines, a range while editing as
// another language, a field not found named by its path, the footer's
// counts each in its color and readable on a phone, F8 through problems at
// one place and the unplaced ones, messages as wide as the text area after
// the host resizes it, and thousands of host problems drawn capped.
// Through the Rust demo and the npm package's vanilla page (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const markers = (page) => page.locator('.config-editor__diagnostic-marker');
const footer = (page) => page.locator('.config-editor__diagnostic-status');
const input = (page) => page.locator('.config-editor__input');
const announcement = (page) => page.locator('.config-editor__announcement');
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

// The vanilla page's document:
//   1 # Burrow server configuration
//   2 listen: 7835
//   3 public_address: tunnel.example.com
//   4 insecure_plaintext: false

test.describe('the Rust demo', () => {
  test('a deprecated key in the document is struck through, the key only', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}?schema=help`);
    await input(page).waitFor();
    await editor(page).paste('timeout: 3\ndeadline: 3\n');
    const marker = markers(page);
    await expect(marker).toHaveCount(1);
    await expect(marker).toHaveClass(/config-editor__diagnostic-marker-deprecated/);
    // The key, not its value.
    await expect(marker).toHaveText('timeout');
    const lines = await marker.evaluate((element) => ({
      underline: getComputedStyle(element).textDecorationLine,
      strike: getComputedStyle(element.querySelector('span')).textDecorationLine,
    }));
    expect(lines).toEqual({ underline: 'underline', strike: 'line-through' });
  });
});

test.describe('the npm package', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a host range over three lines is underlined on each, counted and walked once', async ({ page }) => {
    const errors = await open(page);
    await setHost(page, [{ range: { startLine: 2, startColumn: 9, endLine: 4, endColumn: 19 }, message: 'one block' }]);
    await expect(markers(page)).toHaveCount(3);
    expect(await markers(page).allTextContents()).toEqual(['7835', 'public_address: tunnel.example.com', 'insecure_plaintext']);
    // Only the first is named for screen readers.
    expect(await markers(page).evaluateAll((elements) => elements.map((element) => element.getAttribute('aria-hidden')))).toEqual([null, 'true', 'true']);
    await expect(footer(page)).toHaveText(/^1 error ·/);
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText('1 of 1');
    expect(errors).toEqual([]);
  });

  for (const language of ['json', 'toml', 'hcl', 'xml']) {
    test(`a host range marks the field it starts in while editing as ${language}`, async ({ page }) => {
      const errors = await open(page);
      await setHost(page, [
        { range: { startLine: 2, startColumn: 9, endColumn: 13 }, message: 'on the value' },
        { range: { startLine: 4, startColumn: 1, endColumn: 19 }, message: 'on the key' },
      ]);
      await page.locator('#edit-as').selectOption(language);
      await expect(page.locator('body')).toHaveAttribute('data-editing', language);
      await expect(markers(page)).toHaveCount(2);
      const texts = await markers(page).allTextContents();
      expect(texts[0]).toBe('7835');
      expect(texts[1].replaceAll('"', '')).toBe('insecure_plaintext');
      await expect(footer(page)).toHaveText(/^2 errors ·/);
      expect(errors).toEqual([]);
    });
  }

  test('a problem with a field that is not written names its path in the footer', async ({ page }) => {
    const errors = await open(page);
    await setHost(page, [{ path: '/limits/burst', severity: 'warning', message: 'must be set' }]);
    await expect(footer(page)).toHaveText('1 warning · /limits/burst: must be set');
    expect(errors).toEqual([]);
  });

  test('a host problem on an object in JSON underlines its key', async ({ page }) => {
    const value = '{\n  "web": {\n    "listen": 80\n  }\n}\n';
    const errors = await open(page, { language: 'json', value, schema: '{}', fileName: 'x.json' });
    await setHost(page, [{ path: '/web', message: 'web is off' }]);
    await expect(markers(page)).toHaveText('"web"');
    expect(errors).toEqual([]);
  });

  test('each count of the footer has its own color', async ({ page }) => {
    const errors = await open(page);
    await setHost(page, [
      { path: '/listen', severity: 'error', message: 'e' },
      { path: '/public_address', severity: 'warning', message: 'w' },
      { path: '/insecure_plaintext', severity: 'info', message: 'i' },
    ]);
    await expect(footer(page)).toHaveText(/^1 error, 1 warning, 1 info ·/);
    const colors = await footer(page).evaluate((button) =>
      Object.fromEntries(
        [...button.querySelectorAll('.config-editor__diagnostic-count')].map((count) => [count.textContent, getComputedStyle(count).color]),
      ),
    );
    expect(new Set(Object.values(colors)).size, JSON.stringify(colors)).toBe(3);
    // The error color is the error count's only.
    const hint = await footer(page).locator('.config-editor__diagnostic-detail').evaluate((detail) => getComputedStyle(detail).color);
    expect(Object.values(colors)).not.toContain(hint);
    expect(errors).toEqual([]);
  });

  for (const width of [320, 390]) {
    test(`the footer's message is whole on a ${width} px phone`, async ({ page }) => {
      await page.setViewportSize({ width, height: 640 });
      const errors = await open(page);
      await setHost(page, [{ path: '/listen', message: 'port in use' }]);
      const detail = footer(page).locator('.config-editor__diagnostic-detail');
      await expect(detail).toHaveText(/hover or focus underlined text/);
      const fits = await detail.evaluate((element) => {
        const box = element.getBoundingClientRect();
        const frame = element.closest('.config-editor').getBoundingClientRect();
        return { clipped: element.scrollWidth > element.clientWidth + 1, inside: box.right <= frame.right + 1 && box.left >= frame.left - 1 };
      });
      expect(fits).toEqual({ clipped: false, inside: true });
      expect(errors).toEqual([]);
    });
  }

  test('F8 visits every problem at one place in turn, then the unplaced one; hover lists them all', async ({ page }) => {
    const errors = await open(page);
    await setHost(page, [
      { path: '/listen', severity: 'warning', message: 'second on listen' },
      { path: '/listen', severity: 'error', message: 'port taken' },
      { path: '/public_address', severity: 'info', message: 'info on address' },
      { severity: 'warning', message: 'restart needed' },
    ]);
    await expect(markers(page)).toHaveCount(3);
    await input(page).click();
    await page.keyboard.press('Control+Home');
    const said = [];
    for (let press = 0; press < 5; press += 1) {
      await page.keyboard.press('F8');
      await expect(announcement(page)).toContainText(` of 4`);
      said.push((await announcement(page).textContent()).replace(/^.*?(\d of 4).*?: (.*)$/s, '$1 $2').trim());
    }
    expect(said.map((text) => text.slice(0, 6))).toEqual(['1 of 4', '2 of 4', '3 of 4', '4 of 4', '1 of 4']);
    expect(said.join('\n')).toContain('restart needed');
    // Hovering the place shows both messages, the error first.
    await page.keyboard.press('Escape');
    await pointerOn(page, page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)').first());
    await expect(page.locator('.config-editor__problem-tip-message')).toHaveText(['Error port taken', 'Warning second on listen']);
    expect(errors).toEqual([]);
  });

  test('a document problem stays in the footer and is reached by F8 when the list is cut', async ({ page }) => {
    const schema = JSON.stringify({ type: 'object', additionalProperties: { type: 'integer' } });
    const value = Array.from({ length: 30 }, (_, index) => `k${index}: bad`).join('\n') + '\n';
    const errors = await open(page, { value, schema });
    await setHost(page, [{ severity: 'warning', message: 'restart needed' }]);
    await expect(footer(page)).toHaveText("20+ errors, 1 warning · restart needed · only the editor's first 20 are shown");
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText('21 of 21');
    await expect(announcement(page)).toContainText('restart needed');
    expect(errors).toEqual([]);
  });

  test('a message is as wide as the text area after the host resizes it, without a window resize', async ({ page }) => {
    await page.setViewportSize({ width: 1200, height: 700 });
    const errors = await open(page);
    // The host's container starts narrow, then widens.
    await page.locator('#editor').evaluate((element) => (element.style.width = '200px'));
    await setHost(page, [{ path: '/listen', message: 'a long message '.repeat(12) }]);
    await expect(markers(page)).toHaveCount(1);
    await page.locator('#editor').evaluate((element) => (element.style.width = '900px'));
    await pointerOn(page, markers(page));
    await expect
      .poll(() => page.locator('.config-editor__problem-tip').evaluate((element) => element.getBoundingClientRect().width))
      .toBe(360);
    expect(errors).toEqual([]);
  });

  test('thousands of host problems are all counted, a few hundred drawn', async ({ page }) => {
    const errors = await open(page);
    const list = Array.from({ length: 3000 }, (_, index) => ({ path: '/listen', message: `problem ${index}` }));
    await setHost(page, list);
    await expect(footer(page)).toHaveText(/^3000 errors ·/);
    expect(await markers(page).count()).toBeLessThanOrEqual(200);
    expect(errors).toEqual([]);
  });
});
