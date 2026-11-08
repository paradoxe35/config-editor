import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// QA round 19, problem display, hover and keyboard access: multi-line host
// ranges counted line by line in the underline cap, several problems at one
// place each with its own source, code and docs link (the underline in the
// most serious one's colour), a tall problem tip kept inside the editor and
// scrollable, every problem at a place listed past the cap, and the hover
// docs reached from the keyboard (`Ctrl+K Ctrl+I`) and by a tap.
// Host problems through the npm package's vanilla page (run `npm run build`
// in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const markers = (page) => page.locator('.config-editor__diagnostic-marker');
const footer = (page) => page.locator('.config-editor__diagnostic-status');
const input = (page) => page.locator('.config-editor__input');
const tip = (page) => page.locator('.config-editor__problem-tip');
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

const lines = (count) => Array.from({ length: count }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

test.describe('problems at scale and at one place', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('U19-H1: ranges over hundreds of lines draw at most 200 underlines in all; every problem is counted and walked', async ({ page }) => {
    const errors = await open(page, { value: lines(3000), schema: '{}' });
    const ranges = Array.from({ length: 20 }, (_, index) => ({
      range: { startLine: 1 + index * 7, startColumn: 1, endLine: 201 + index * 7, endColumn: 2 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await setHost(page, ranges);
    await expect(footer(page)).toHaveText(/^20 warnings/);
    // 20 × 201 lines would be 4020 underlines.
    expect(await markers(page).count()).toBeLessThanOrEqual(200);
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText(' of 20');
    expect(errors).toEqual([]);
  });

  test('U19-M2: several problems at one place each show their own source, code and docs link, the error first and in its colour', async ({ page }) => {
    const errors = await open(page);
    await setHost(page, [
      { path: '/listen', severity: 'warning', message: 'deprecated port', source: 'lint', code: 'W7' },
      { path: '/listen', severity: 'error', message: 'port in use', source: 'burrow', code: 'E_PORT', docsUrl: 'https://example.com/ports' },
      { path: '/listen', severity: 'hint', message: 'consider 443' },
      { path: '/listen', severity: 'hint', message: 'consider 8443' },
    ]);
    await expect(markers(page)).toHaveCount(4);
    // Only the error's underline is drawn in colour over the same text.
    await expect(page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)')).toHaveCount(1);
    await expect(page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)')).toHaveAttribute(
      'data-message',
      'port in use',
    );
    await page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)').hover();
    const entries = tip(page).locator('.config-editor__problem-tip-entry');
    await expect(entries).toHaveCount(4);
    await expect(entries.nth(0)).toHaveClass(/--error/);
    await expect(entries.nth(0).locator('.config-editor__problem-tip-message')).toHaveText('Error port in use');
    await expect(entries.nth(0).locator('.config-editor__problem-tip-detail')).toHaveText('burrow · E_PORT · Open docs ↗ (Alt+Enter)');
    await expect(entries.nth(0).locator('a')).toHaveAttribute('href', 'https://example.com/ports');
    await expect(entries.nth(1).locator('.config-editor__problem-tip-message')).toHaveText('Warning deprecated port');
    await expect(entries.nth(1).locator('.config-editor__problem-tip-detail')).toHaveText('lint · W7');
    await expect(entries.nth(1).locator('a')).toHaveCount(0);
    // Each default-source problem says its own source, under its own message.
    for (const index of [2, 3]) {
      await expect(entries.nth(index)).toHaveClass(/--hint/);
      await expect(entries.nth(index).locator('.config-editor__problem-tip-detail')).toHaveText('host');
    }
    expect(errors).toEqual([]);
  });

  test('U19-M3: a tall tip near the bottom stays inside the editor, scrolls, and F8 scrolls to the problem it went to', async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 600 });
    const errors = await open(page, { value: lines(60), schema: '{}' });
    const box = await input(page).boundingBox();
    const lineHeight = await input(page).evaluate((element) => parseFloat(getComputedStyle(element).lineHeight));
    // The last fully visible line.
    const last = Math.floor((box.height - 16) / lineHeight) - 1;
    const long = 'a long message that wraps over a few lines of the tip '.repeat(3);
    await setHost(page, [
      ...Array.from({ length: 11 }, (_, index) => ({
        range: { startLine: last + 1, startColumn: 1, endColumn: 3 },
        severity: 'warning',
        message: `warning ${index} ${long}`,
      })),
      { range: { startLine: last + 1, startColumn: 1, endColumn: 3 }, severity: 'error', message: 'the error' },
    ]);
    await expect(markers(page)).toHaveCount(12);
    await page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-covered)').hover();
    await expect(tip(page)).toBeVisible();
    const placed = await tip(page).evaluate((element) => {
      const rect = element.getBoundingClientRect();
      const area = document.querySelector('.config-editor__input').getBoundingClientRect();
      return {
        top: rect.top,
        bottom: rect.bottom,
        areaTop: area.top,
        areaBottom: area.bottom,
        scrolls: element.scrollHeight > element.clientHeight,
        first: element.querySelector('.config-editor__problem-tip-message').textContent,
      };
    });
    expect(placed.top).toBeGreaterThanOrEqual(placed.areaTop);
    expect(placed.bottom).toBeLessThanOrEqual(placed.areaBottom);
    expect(placed.scrolls).toBe(true);
    // The most serious first, at the top, in view.
    expect(placed.first).toBe('Error the error');
    // F8 to the last one at that place: its entry is scrolled into view.
    await input(page).click();
    await page.keyboard.press('Control+Home');
    for (let press = 0; press < 12; press += 1) await page.keyboard.press('F8');
    await expect(announcement(page)).toContainText('12 of 12');
    const current = tip(page).locator('.config-editor__problem-tip-entry--current');
    await expect(current).toHaveCount(1);
    await expect
      .poll(() =>
        current.evaluate((entry) => {
          const own = entry.getBoundingClientRect();
          const box = entry.parentElement.getBoundingClientRect();
          return own.top >= box.top - 1 && own.top < box.bottom;
        }),
      )
      .toBe(true);
    expect(errors).toEqual([]);
  });

  test('U19 low: hovering a place lists every problem there, those past the underline cap included', async ({ page }) => {
    const errors = await open(page, { value: lines(300), schema: '{}' });
    const list = Array.from({ length: 210 }, (_, index) => ({
      range: { startLine: index === 0 || index > 200 ? 1 : 1 + index, startColumn: 1, endColumn: 3 },
      severity: 'warning',
      message: `w${index}`,
    }));
    await setHost(page, list);
    await expect(footer(page)).toHaveText(/^210 warnings/);
    await markers(page).first().hover();
    await expect(tip(page).locator('.config-editor__problem-tip-entry')).toHaveCount(10);
    expect(errors).toEqual([]);
  });
});

test.describe('host problems (QA 19 host lows)', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('H19-F4: host hints on fields hide none of their schema errors, past the cap included', async ({ page }) => {
    const value = Array.from({ length: 30 }, (_, index) => `k${index}: x${index}`).join('\n') + '\n';
    const schema = JSON.stringify({ type: 'object', additionalProperties: { type: 'integer' } });
    const errors = await open(page, { value, schema });
    await expect(footer(page)).toHaveText(/^20\+ errors/);
    await setHost(page, Array.from({ length: 20 }, (_, index) => ({ path: `/k${index}`, severity: 'hint', message: `h${index}` })));
    await expect(footer(page)).toHaveText(/^20\+ errors, 20 hints/);
    expect(await page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-hint)').count()).toBe(20);
    expect(errors).toEqual([]);
  });

  test('H19-F7: a range with its end left out is marked on an empty line and at a line end', async ({ page }) => {
    const errors = await open(page);
    // Line 5 is the empty line after the last line break; line 2 is 12 characters.
    const thrown = await page.evaluate(() => {
      try {
        window.editor.update({
          hostDiagnostics: [
            { range: { startLine: 5, startColumn: 1 }, message: 'on the empty line' },
            { range: { startLine: 2, startColumn: 13 }, message: 'at the line end' },
          ],
        });
        return null;
      } catch (error) {
        return String(error);
      }
    });
    expect(thrown).toBeNull();
    await expect(footer(page)).toHaveText(/^2 errors/);
    expect(errors).toEqual([]);
  });

  test('H19-F10: a closed fold hiding a warning shows it on its chip', async ({ page }) => {
    const errors = await open(page, { value: 'web:\n  http: 80\n  hosts: []\nlisten: 1\n', schema: '{}' });
    await setHost(page, [{ range: { startLine: 3, startColumn: 3, endColumn: 8 }, severity: 'warning', message: 'hidden' }]);
    await page.evaluate(() => window.editor.foldAll());
    await expect(page.locator('.config-editor__fold-chip')).toHaveClass(/config-editor__fold-chip--warning/);
    expect(errors).toEqual([]);
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

test.describe('U19-M6: hover docs from the keyboard', () => {
  setupEditor(test, '?language=yaml');

  test('Ctrl+K Ctrl+I shows the docs of the key at the caret and says them; Escape closes them', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    await input(page).evaluate((element) => {
      element.focus();
      element.setSelectionRange(2, 2);
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    const description = await card.locator('.config-editor__hover-description').first().textContent();
    await expect(announcement(page)).toContainText(description.trim());
    await expect(announcement(page)).toContainText('listen');
    await page.keyboard.press('Escape');
    await expect(card).toHaveCount(0);
    // The caret did not move, and the text did not change.
    expect(await e.value()).toBe('listen: 7835\n');
  });

  test('the chord still folds and unfolds everything', async ({ page }) => {
    const e = editor(page);
    await e.paste('web:\n  listen: 1\n  hosts: []\n');
    await input(page).click();
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+0');
    await expect(page.locator('.config-editor__fold-chip')).toHaveCount(1);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(page.locator('.config-editor__fold-chip')).toHaveCount(0);
  });
});

test.describe('U19-M6: hover docs by a tap', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 740 } });
  setupEditor(test, '?language=yaml');

  test('a tap on a key shows its docs', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    const at = await pointAt(page, 0, 2);
    await page.touchscreen.tap(at.x, at.y);
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
});
