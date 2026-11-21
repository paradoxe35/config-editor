import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// What the editor draws over the text sits on the text it marks when the
// line holds characters wider than a column (CJK, emoji, joined emoji,
// accents): a problem's underline, the problem tip, a fold chip, the hover
// card and what the pointer is over, and the suggestion list at the
// caret. The browser draws such characters about 1.7 columns wide; every
// overlay once took each character for one column and landed to the left.

// Scripts in the mixed text meet at spaces: where a Latin letter touches
// a Han character the browser picks the fallback font by the run the
// letter is in, so the highlighted copy (a span per token) and the text
// area (one run) already draw it a few pixels apart.
const FILLS = {
  cjk: '日本語テキスト日本語',
  emoji: '👨‍👩‍👧👍🏽😀🎉',
  mixed: 'aé 日😀 be\u0301x 本',
};
const THEMES = ['default', 'nord'];

/** The window box of the painted text of line `line` from UTF-16 `start` to `end`. */
function painted(page, line, start, end) {
  return page.evaluate(
    ([line, start, end]) => {
      const row = document.querySelectorAll('.config-editor__highlight .config-editor__code-line')[line];
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
      const range = document.createRange();
      let offset = 0;
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const after = offset + node.length;
        if (start >= offset && start <= after) range.setStart(node, start - offset);
        if (end >= offset && end <= after) range.setEnd(node, end - offset);
        offset = after;
      }
      const box = range.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, text: row.textContent };
    },
    [line, start, end],
  );
}

/** The painted box of the last `needle` on line `line`. */
async function paintedNeedle(page, line, needle) {
  const { text } = await painted(page, line, 0, 0);
  const at = text.lastIndexOf(needle);
  expect(at, `${needle} on line ${line}`).toBeGreaterThanOrEqual(0);
  return painted(page, line, at, at + needle.length);
}

const box = (locator) => locator.evaluate((element) => element.getBoundingClientRect().toJSON());

async function useTheme(page, theme) {
  if (theme !== 'default') await page.locator('.demo__theme-select').selectOption(theme);
}

for (const theme of THEMES) {
  test.describe(`overlays on wide text, ${theme} theme`, () => {
    test.describe('in JSON', () => {
      setupEditor(test, '?language=json');

      test('F8 scrolls a problem at the end of a long wide line into view', async ({ page }) => {
        await useTheme(page, theme);
        const e = editor(page);
        await e.paste(`{"public_address": "${'日本語テキスト'.repeat(20)}", "listen": true}\n`);
        await e.input.evaluate((element) => {
          element.focus();
          element.setSelectionRange(0, 0);
        });
        await page.keyboard.press('F8');
        await expect(page.locator('.config-editor__problem-tip')).toBeVisible();
        const text = await paintedNeedle(page, 0, 'true');
        const area = await box(e.input);
        // The caret, at its start, is in view.
        expect(text.left, 'not left of the view').toBeGreaterThanOrEqual(area.left);
        expect(text.left, 'not right of the view').toBeLessThanOrEqual(area.right - 10);
      });

      for (const [name, fill] of Object.entries(FILLS)) {
        test(`a problem's underline and tip sit on their text after ${name} text`, async ({ page }) => {
          await useTheme(page, theme);
          const e = editor(page);
          await e.paste(`{"${fill}": "${fill}", "listen": true}\n`);
          const marker = page.locator('.config-editor__diagnostic-marker', { hasText: 'true' });
          await expect(marker).toHaveCount(1);
          const text = await paintedNeedle(page, 0, 'true');
          const underline = await box(marker);
          expect(underline.left, 'underline left').toBeCloseTo(text.left, 0);
          expect(underline.right, 'underline right').toBeCloseTo(text.right, 0);
          // The pointer over the painted `true` opens its tip there.
          await page.mouse.move((text.left + text.right) / 2, (text.top + text.bottom) / 2, { steps: 3 });
          const tip = page.locator('.config-editor__problem-tip');
          await expect(tip).toBeVisible();
          expect(Math.abs((await box(tip)).left - text.left), 'tip left').toBeLessThan(2);
        });

        test(`the suggestion list opens at the caret after ${name} text`, async ({ page }) => {
          await useTheme(page, theme);
          const e = editor(page);
          await e.paste(`{"public_address": "${fill}", }\n`);
          await e.input.evaluate((element) => {
            const at = element.value.indexOf(', }') + 2;
            element.focus();
            element.setSelectionRange(at, at);
          });
          await page.keyboard.press('Control+Space');
          const list = page.locator('.config-editor__completions');
          await expect(list).not.toHaveClass(/hidden/);
          const caret = await page.evaluate(() => document.querySelector('.config-editor__input').selectionStart);
          const at = await painted(page, 0, caret, caret);
          expect(Math.abs((await box(list)).left - at.left), 'list left').toBeLessThan(2);
        });
      }
    });

    test.describe('in YAML', () => {
      setupEditor(test, '?language=yaml');

      for (const [name, fill] of Object.entries(FILLS)) {
        test(`a fold chip follows its ${name} header`, async ({ page }) => {
          await useTheme(page, theme);
          const e = editor(page);
          await e.paste(`"${fill}":\n  a: 1\n  b: 2\nlisten: 1\n`);
          await e.input.evaluate((element) => {
            element.focus();
            element.setSelectionRange(1, 1);
          });
          await page.keyboard.press('Control+Shift+BracketLeft');
          const chip = page.locator('.config-editor__fold-chip');
          await expect(chip).toHaveCount(1);
          const line = await painted(page, 0, 0, `"${fill}":`.length);
          const colon = await paintedNeedle(page, 0, ':');
          const column = colon.right - colon.left;
          expect(Math.abs((await box(chip)).left - (line.right + column)), 'chip left').toBeLessThan(2);
        });

        test(`the hover card opens for a key after ${name} text`, async ({ page }) => {
          await useTheme(page, theme);
          const e = editor(page);
          await e.paste(`websocket: {host: "${fill}", challenge: http}\n`);
          await page.mouse.move(5, 500);
          const key = await paintedNeedle(page, 0, 'challenge');
          // On its last letter: a pointer taken a column per character
          // was past the key there.
          await page.mouse.move(key.right - 2, (key.top + key.bottom) / 2, { steps: 3 });
          const card = page.locator('.config-editor__hover');
          await expect(card).toBeVisible();
          await expect(card).toContainText('challenge');
          expect(Math.abs((await box(card)).left - key.left), 'card left').toBeLessThan(2);
        });
      }
    });
  });
}

// The same overlays over wide text the editor starts with, before any
// edit: drawn at mount, before the text area was on the page to measure,
// the underlines once took each character for a column and stayed so
// until the first key.
const WIDE = FILLS.cjk;
const STARTS_WITH = {
  json: `{"${WIDE}": "${WIDE}", "listen": true}\n`,
  yaml: `# ${WIDE}\n"${WIDE}": "${WIDE}"\nlisten: true\n${WIDE}: x\n`,
  toml: `"${WIDE}" = "${WIDE}"\nlisten = true\n`,
};

/**
 * The problem underlines of the editors shown in `root` with wide text
 * before or under them, each with whether it sits on its own text as
 * painted (on any copy of that text on its line).
 */
function underlinesOnText(page, root) {
  return page.evaluate((root) => {
    const range = document.createRange();
    const paintedAt = (row, start, end) => {
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
      let offset = 0;
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const after = offset + node.length;
        if (start >= offset && start <= after) range.setStart(node, start - offset);
        if (end >= offset && end <= after) range.setEnd(node, end - offset);
        offset = after;
      }
      return range.getBoundingClientRect();
    };
    const underlinesOf = (scope) => {
      const rows = [...scope.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
      return [...scope.querySelectorAll('.config-editor__diagnostic-marker')]
        .filter((marker) => marker.textContent)
        .map((marker) => {
          const box = marker.getBoundingClientRect();
          const middle = (box.top + box.bottom) / 2;
          const row = rows.find((row) => {
            const line = row.getBoundingClientRect();
            return line.top <= middle && middle <= line.bottom;
          });
          const text = row?.textContent ?? '';
          const marked = marker.textContent;
          const places = [];
          for (let at = text.indexOf(marked); at >= 0; at = text.indexOf(marked, at + 1)) {
            places.push(paintedAt(row, at, at + marked.length));
          }
          const wide = /[^\x00-\x7f]/.test(text.slice(0, text.lastIndexOf(marked) + marked.length));
          const on = places.some(
            (place) => Math.abs(place.left - box.left) < 1.5 && Math.abs(place.right - box.right) < 1.5,
          );
          return { marked, wide, on, drawn: [box.left, box.right], painted: places.map((place) => [place.left, place.right]) };
        })
        .filter((underline) => underline.wide);
    };
    // A documents view keeps its other documents' editors, hidden.
    const editors = [...document.querySelector(root).querySelectorAll('.config-editor')];
    return editors.filter((editor) => editor.checkVisibility({ visibilityProperty: true })).flatMap(underlinesOf);
  }, root);
}

/** Expects underlines over or after wide text in `root`, every one on its text. */
async function expectUnderlinesOnText(page, root = 'body') {
  const underlines = await underlinesOnText(page, root);
  expect(underlines.length, 'underlines over or after wide text').toBeGreaterThan(0);
  expect(underlines.filter((underline) => !underline.on), 'underlines off their text').toEqual([]);
}

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const VANILLA = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

test.describe('overlays on wide text the editor starts with', () => {
  for (const [language, value] of Object.entries(STARTS_WITH)) {
    test(`${language}: the underlines (and the tip) sit on their text before any edit`, async ({ page, baseURL }) => {
      await page.addInitScript((text) => {
        window.configEditorSample = text;
      }, value);
      await page.goto(`${baseURL}?language=${language}&sample=page`);
      await page.locator('.config-editor__diagnostic-marker').first().waitFor();
      await expectUnderlinesOnText(page);
      if (language !== 'json') return;
      // The tip opens at the painted text under the pointer.
      const text = await paintedNeedle(page, 0, 'true');
      await page.mouse.move((text.left + text.right) / 2, (text.top + text.bottom) / 2, { steps: 3 });
      const tip = page.locator('.config-editor__problem-tip');
      await expect(tip).toBeVisible();
      expect(Math.abs((await box(tip)).left - text.left), 'tip left').toBeLessThan(2);
    });
  }

  test.describe('in the JS package', () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('a ConfigEditor made with wide text draws its underlines on it', async ({ page }) => {
      const options = { language: 'json', value: STARTS_WITH.json };
      await page.goto(`${VANILLA}?options=${encodeURIComponent(JSON.stringify(options))}`);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.locator('#editor .config-editor__diagnostic-marker').first().waitFor();
      await expectUnderlinesOnText(page, '#editor');
    });

    test('each Documents tab with wide text draws its underlines on it', async ({ page }) => {
      await page.goto(VANILLA);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(async (documents) => {
        const { ConfigDocuments } = await import('../config-editor/index.js');
        const schema = await (await fetch('../../schema.json')).text();
        window.editor.destroy();
        const host = document.createElement('div');
        host.id = 'documents';
        host.style.height = '400px';
        document.body.prepend(host);
        window.documentsView = new ConfigDocuments(host, {
          documents: documents.map(([language, value]) => ({ id: language, fileName: `wide.${language}`, value, schema })),
          activeId: 'json',
        });
      }, Object.entries(STARTS_WITH));
      for (const language of Object.keys(STARTS_WITH)) {
        await page.evaluate((id) => window.documentsView.update({ activeId: id }), language);
        await page.locator('#documents .config-editor__diagnostic-marker:visible').first().waitFor();
        await page.waitForTimeout(200);
        await expectUnderlinesOnText(page, '#documents');
      }
    });
  });
});
