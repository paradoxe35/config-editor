import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 20 (UX): a click or a tap on the underline of a problem with a
// docs link moves the caret there and shows its tip (the tip, not yet
// measured, covered the underline and took the click), and the link in the
// tip still opens; the underline cap draws the most serious problems first
// and the footer says when it left some out; a tip in an editor too short
// for it takes the window's room instead of scrolling; `Ctrl+K` and an
// unknown key types nothing; `Ctrl+K Ctrl+I` says each part once with one
// full stop, and finds the XML element with the caret on its `<`; a closed
// fold's chip names the most serious problem it hides. Through the npm
// package's vanilla page (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const input = (page) => page.locator('.config-editor__input');
const markers = (page) => page.locator('.config-editor__diagnostic-marker');
const tip = (page) => page.locator('.config-editor__problem-tip');
const footer = (page) => page.locator('.config-editor__diagnostic-status');
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

// "line:column" of the caret, and whether the editor has the focus.
const caret = (page) =>
  input(page).evaluate((element) => {
    const before = element.value.slice(0, element.selectionStart).split('\n');
    return `${before.length}:${before.at(-1).length + 1}${document.activeElement === element ? '' : ' (unfocused)'}`;
  });

const keyLines = (count) => Array.from({ length: count }, (_, index) => `key_number_${index}: value ${index}`).join('\n') + '\n';
const WITH_DOCS = [
  { range: { startLine: 6, startColumn: 3, endColumn: 9 }, severity: 'error', message: 'one with docs', docsUrl: 'https://example.com/d', source: 'srv' },
];

test.describe('U20-1: an underline with a docs link', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const mode of ['mouse', 'touch']) {
    test.describe(`by ${mode}`, () => {
      if (mode === 'touch') test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });
      else test.use({ viewport: { width: 1000, height: 600 } });

      test('a click or tap moves the caret and shows the tip; its link opens the docs', async ({ page, context }) => {
        const errors = await open(page, { value: keyLines(30), schema: '{}' });
        const opened = [];
        context.on('page', (popup) => opened.push(popup.url()));
        await page.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
        await setHost(page, WITH_DOCS);
        await expect(markers(page)).toHaveCount(1);
        const box = await markers(page).first().boundingBox();
        // On the left of the first character: the caret lands before it.
        const x = box.x + 2;
        const y = box.y + box.height / 2;
        if (mode === 'touch') await page.touchscreen.tap(x, y);
        else await page.mouse.click(x, y);
        await expect.poll(() => caret(page)).toBe('6:3');
        await expect(tip(page)).toBeVisible();
        await expect(tip(page)).toContainText('one with docs');
        expect(opened).toEqual([]);
        // The tip sits beside the line, never over it.
        const placed = await tip(page).boundingBox();
        expect(placed.y + placed.height <= box.y + 1 || placed.y >= box.y + box.height - 1).toBe(true);
        // The link inside the tip still opens the docs.
        const link = page.locator('.config-editor__problem-tip-link');
        if (mode === 'touch') {
          const at = await link.boundingBox();
          await page.touchscreen.tap(at.x + at.width / 2, at.y + at.height / 2);
        } else {
          await page.mouse.move(x, y);
          await link.click();
        }
        await expect.poll(() => opened).toEqual(['https://example.com/d']);
        expect(errors).toEqual([]);
      });
    });
  }
});

test.describe('U20-2: the underline cap', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 600 } });

  const LINES = 400;
  const cases = {
    'an info over the whole file': [
      { range: { startLine: 1, startColumn: 1, endLine: LINES, endColumn: 1 }, severity: 'info', message: 'file is generated' },
      { range: { startLine: 10, startColumn: 1, endColumn: 4 }, severity: 'error', message: 'REAL ERROR' },
    ],
    '250 hints listed first': [
      ...Array.from({ length: 250 }, (_, index) => ({ range: { startLine: 30 + index, startColumn: 1, endColumn: 3 }, severity: 'hint', message: `hint ${index}` })),
      { range: { startLine: 10, startColumn: 1, endColumn: 4 }, severity: 'error', message: 'REAL ERROR' },
    ],
  };
  for (const [name, problems] of Object.entries(cases)) {
    test(`${name} leaves the error underlined and the footer says underlines were left out`, async ({ page }) => {
      const value = Array.from({ length: LINES }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';
      const errors = await open(page, { value, schema: '{}' });
      await setHost(page, problems);
      const error = page.locator('.config-editor__diagnostic-marker:not(.config-editor__diagnostic-marker-info):not(.config-editor__diagnostic-marker-hint)');
      await expect(error).toHaveCount(1);
      await expect(error).toHaveAttribute('data-message', 'REAL ERROR');
      await expect(markers(page)).toHaveCount(200);
      await expect(footer(page)).toContainText('only 200 underlines are drawn, the most serious first');
      // Fewer problems: every one drawn, and the note goes.
      await setHost(page, problems.slice(-1));
      await expect(footer(page)).not.toContainText('underlines are drawn');
      expect(errors).toEqual([]);
    });
  }
});

test.describe('the problem tip in a short editor', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 700, height: 260 } });

  test('shows one message whole, beside its line, without scrolling', async ({ page }) => {
    const errors = await open(page, { value: keyLines(60), schema: '{}' });
    // The middle line in view.
    const line = await page.evaluate(() => {
      const box = document.querySelector('.config-editor__input').getBoundingClientRect();
      const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
      const shown = rows.map((row, index) => [row.getBoundingClientRect(), index + 1]).filter(([rect]) => rect.top >= box.top - 1 && rect.bottom <= Math.min(box.bottom, innerHeight) + 1);
      return shown[Math.floor(shown.length / 2)][1];
    });
    await setHost(page, [
      { range: { startLine: line, startColumn: 3, endColumn: 9 }, severity: 'error', message: 'The value here is not allowed by the server because it overlaps another listener', code: 'E0', source: 'burrow' },
    ]);
    const box = await markers(page).first().boundingBox();
    await page.mouse.move(box.x + 6, box.y + box.height / 2);
    await expect(tip(page)).toBeVisible();
    await expect
      .poll(() => tip(page).evaluate((element) => element.scrollHeight <= element.clientHeight + 1))
      .toBe(true);
    const placed = await tip(page).boundingBox();
    expect(placed.y).toBeGreaterThanOrEqual(0);
    expect(placed.y + placed.height).toBeLessThanOrEqual(260);
    expect(placed.y + placed.height <= box.y + 1 || placed.y >= box.y + box.height - 1).toBe(true);
    expect(errors).toEqual([]);
  });
});

test.describe('Ctrl+K chords', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('Ctrl+K then an unknown key types nothing, as in VS Code', async ({ page }) => {
    const errors = await open(page, { value: 'a: 1\n', schema: '{}' });
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('x');
    await page.keyboard.type('b');
    await expect.poll(() => input(page).inputValue()).toBe('a: 1\nb');
    expect(errors).toEqual([]);
  });

  test('Ctrl+K Ctrl+I says each part once, with one full stop after it', async ({ page }) => {
    const schema = JSON.stringify({
      type: 'object',
      properties: { listen: { type: 'integer', title: 'Port', description: 'Port to listen on.', default: 7835 } },
    });
    const errors = await open(page, { value: 'listen: 80\n', schema });
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(page.locator('.config-editor__hover')).toHaveCount(1);
    await expect(announcement(page)).toContainText('Port to listen on. Default: 7835');
    expect(await announcement(page).textContent()).not.toContain('..');
    expect(errors).toEqual([]);
  });

  test('Ctrl+K Ctrl+I with the caret on the `<` of an XML tag shows its docs', async ({ page }) => {
    const schema = JSON.stringify({
      type: 'object',
      properties: { config: { type: 'object', properties: { listen: { type: 'integer', description: 'Port to listen on' } } } },
    });
    const errors = await open(page, { value: '<config>\n  <listen>80</listen>\n</config>\n', language: 'xml', schema });
    await input(page).click();
    // Line 2, on the `<` of `<listen>`.
    await input(page).evaluate((element) => {
      const at = element.value.indexOf('<listen>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    expect(errors).toEqual([]);
  });
});

test.describe('a closed fold hiding a problem', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('its chip names the most serious problem it hides', async ({ page }) => {
    const errors = await open(page, { value: 'web:\n  listen: 1\n  hosts: []\nother: 2\n', schema: '{}' });
    await setHost(page, [
      { range: { startLine: 2, startColumn: 3, endColumn: 9 }, severity: 'warning', message: 'w' },
      { range: { startLine: 3, startColumn: 3, endColumn: 8 }, severity: 'error', message: 'e' },
    ]);
    await page.evaluate(() => window.editor.foldAll());
    const chip = page.locator('.config-editor__fold-chip');
    await expect(chip).toHaveCount(1);
    // QA 21: two problems, counted (they were worded as one).
    await expect(chip).toHaveAttribute('aria-label', 'Unfold lines 1–3, hides 2 problems, the most serious: Error');
    await expect(chip).toHaveAttribute('title', 'Unfold lines 1–3, hides 2 problems, the most serious: Error');
    expect(errors).toEqual([]);
  });
});
