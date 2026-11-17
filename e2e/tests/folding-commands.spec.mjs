import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Fold commands: nothing to fold, copy and cut on a folded header, a fold chip naming the problem it hides.

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

// A YAML block scalar holding an empty line froze the
// tab (loading it, Enter in it, Edit as YAML of a string with a blank line);
// copy and cut on a folded header take the whole block; Enter on an empty
// `- ` item and inside a flow sequence; XML's `</` and an attribute quote
// before `/>`; HCL's `"${`; a find pattern matching empty text (`^`).

const saved = (page) => page.locator('.demo__output-text').textContent();

// Puts the caret at `offset` of the text on screen.
const caretAt = (e, offset) =>
  e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);

// The folded header is the caret's line (1-based `header`).
const FOLDED = {
  yaml: ['a:\n  b:\n    c: 1\n  e: 3\nf: 4\n', 1, 'a:\n  b:\n    c: 1\n  e: 3\n', 'f: 4\n'],
  json: ['{\n  "a": {\n    "c": 1\n  },\n  "f": 4\n}\n', 2, '  "a": {\n    "c": 1\n  },\n', '{\n  "f": 4\n}\n'],
  toml: ['l = [\n  1,\n  2,\n]\nn = 1\n', 1, 'l = [\n  1,\n  2,\n]\n', 'n = 1\n'],
  hcl: ['a {\n  c = 1\n}\nf = 4\n', 1, 'a {\n  c = 1\n}\n', 'f = 4\n'],
  xml: ['<r>\n  <a>\n    <c>1</c>\n  </a>\n  <f>4</f>\n</r>\n', 2, '  <a>\n    <c>1</c>\n  </a>\n', '<r>\n  <f>4</f>\n</r>\n'],
};

const said = (page) => page.locator('.config-editor__announcement').first();

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
    // Two problems, counted (they were worded as one).
    await expect(chip).toHaveAttribute('aria-label', 'Unfold lines 1–3, hides 2 problems, the most serious: Error');
    await expect(chip).toHaveAttribute('title', 'Unfold lines 1–3, hides 2 problems, the most serious: Error');
    expect(errors).toEqual([]);
  });
});

for (const [language, [text, header, block, left]] of Object.entries(FOLDED)) {
  test.describe(`${language}: copy and cut on a folded header`, () => {
    setupEditor(test, `?language=${language}`);

    test('take the whole block', async ({ page, context }) => {
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
      const e = editor(page);
      await e.paste(text);
      await expect.poll(() => saved(page)).toBe(text);
      const lines = text.split('\n');
      await caretAt(e, lines.slice(0, header - 1).join('\n').length + (header > 1 ? 1 : 0));
      await page.keyboard.press('Control+Shift+BracketLeft');
      await expect.poll(() => e.value()).not.toBe(text);
      await page.keyboard.press('Control+C');
      expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(block);
      await page.keyboard.press('Control+X');
      await expect.poll(() => saved(page)).toBe(left);
    });
  });
}

test.describe('fold keys with nothing to do', () => {
  setupEditor(test, '?sample=large');

  /** Puts the caret on 1-based `line`. */
  async function goTo(page, line) {
    await input(page).evaluate((element, line) => {
      const lines = element.value.split('\n');
      const at = lines.slice(0, line - 1).reduce((sum, text) => sum + text.length + 1, 0);
      element.focus();
      element.setSelectionRange(at, at);
    }, line);
  }

  test('say "Nothing to fold" and "Nothing to unfold"', async ({ page }) => {
    await input(page).click();
    // A line that starts no range and is in none: the last.
    const last = await input(page).evaluate((element) => element.value.replace(/\n$/, '').split('\n').length);
    const ranged = await input(page).evaluate((element) => {
      const lines = element.value.split('\n');
      return lines.findIndex((line, index) => /:\s*$/.test(line) && /^\s/.test(lines[index + 1] ?? '')) + 1;
    });
    expect(ranged).toBeGreaterThan(0);
    await page.keyboard.press('Control+End');
    await page.keyboard.type('zz: 1');
    await goTo(page, last + 1);
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(said(page)).toHaveText(/^Nothing to fold\s*$/);
    await page.keyboard.press('Control+Shift+BracketRight');
    await expect(said(page)).toHaveText(/^Nothing to unfold\s*$/);
    // Folded once: the same key again finds nothing more to fold there.
    await goTo(page, ranged);
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(said(page)).toHaveText(/^Folded lines/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(said(page)).toHaveText(/^Unfolded lines/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(said(page)).toHaveText(/^Nothing to unfold\s*$/);
  });
});
