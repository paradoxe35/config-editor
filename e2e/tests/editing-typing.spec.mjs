import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Typing in YAML block scalars with empty lines, in the demo and the npm package.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

// QA round 21 typing: a YAML block scalar holding an empty line froze the
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

const BLOCKS = [
  's: |\n  a\n\n  b\nn: 1\n',
  's: >\n  a\n\n  b\n',
  's: >-\n  a\n  \n  b\n',
  'x:\n  s: |\n    a\n\n\n    b\n  t: 1\n',
  'l:\n  - |\n    a\n\n    b\n',
];

test.describe('YAML block scalars with empty lines', () => {
  setupEditor(test, '?language=yaml');
  for (const text of BLOCKS) {
    test(`load and type in ${JSON.stringify(text)}`, async ({ page }) => {
      const e = editor(page);
      await e.paste(text);
      await expect.poll(() => saved(page)).toBe(text);
      // Enter on the empty line, then text there.
      const blank = text.search(/\n[ ]*\n/) + 1;
      await caretAt(e, blank);
      await page.keyboard.press('Enter');
      await page.keyboard.type('c');
      await expect.poll(() => e.value()).toContain('c');
      await page.keyboard.press('Enter');
      await page.keyboard.press('Enter');
      await page.keyboard.type('d');
      await expect.poll(() => saved(page)).toContain('d');
      expect(await saved(page)).toContain('b\n');
    });
  }
});

test.describe('YAML block scalars with empty lines in the npm package', () => {
  test.skip(!built, 'needs the JS package built (cd js && npm run build)');

  for (const [language, value] of [
    ['yaml', 's: |\n  a\n\n  b\n'],
    ['yaml', 'x:\n  s: >-\n    a\n    \n    b\n'],
    ['toml', 's = """\na\n\nb\n"""\n'],
    ['hcl', 's = <<EOT\na\n\nb\nEOT\n'],
    ['xml', '<r>\n  <s>a\n\nb</s>\n</r>\n'],
  ]) {
    test(`${language} ${JSON.stringify(value)} loads`, async ({ page }) => {
      await page.goto(vanilla({ schema: null, language, value }));
      await page.locator('.config-editor__input').waitFor();
      await expect.poll(() => page.locator('.config-editor__input').inputValue()).toBe(value);
    });
  }
});
