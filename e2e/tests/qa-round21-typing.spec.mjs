import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

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
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

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

  test('a string with a blank line edited as YAML', async ({ page }) => {
    await page.goto(page.url().replace('language=yaml', 'language=json'));
    await page.locator('.config-editor__input').waitFor();
    const e = editor(page);
    await e.paste('{\n  "s": "a\\n\\nb",\n  "n": 1\n}\n');
    await expect.poll(() => saved(page)).toContain('"n": 1');
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('s: |');
    const shown = await e.value();
    // Enter at the end of the block's first line, then text.
    await caretAt(e, shown.indexOf('\n\n'));
    await page.keyboard.press('Enter');
    await page.keyboard.type('x');
    await expect.poll(() => saved(page)).toContain('"s": "a\\nx\\n\\nb"');
  });
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

// The folded header is the caret's line (1-based `header`).
const FOLDED = {
  yaml: ['a:\n  b:\n    c: 1\n  e: 3\nf: 4\n', 1, 'a:\n  b:\n    c: 1\n  e: 3\n', 'f: 4\n'],
  json: ['{\n  "a": {\n    "c": 1\n  },\n  "f": 4\n}\n', 2, '  "a": {\n    "c": 1\n  },\n', '{\n  "f": 4\n}\n'],
  toml: ['l = [\n  1,\n  2,\n]\nn = 1\n', 1, 'l = [\n  1,\n  2,\n]\n', 'n = 1\n'],
  hcl: ['a {\n  c = 1\n}\nf = 4\n', 1, 'a {\n  c = 1\n}\n', 'f = 4\n'],
  xml: ['<r>\n  <a>\n    <c>1</c>\n  </a>\n  <f>4</f>\n</r>\n', 2, '  <a>\n    <c>1</c>\n  </a>\n', '<r>\n  <f>4</f>\n</r>\n'],
};

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

test.describe('YAML Enter', () => {
  setupEditor(test, '?language=yaml');

  test('on an empty item ends the list at its key', async ({ page }) => {
    const e = editor(page);
    await e.paste('l:\n  - a\n');
    await caretAt(e, 'l:\n  - a'.length);
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.keyboard.type('k: 1');
    await expect.poll(() => saved(page)).toBe('l:\n  - a\nk: 1\n');
  });

  test('inside a flow sequence goes one level deeper', async ({ page }) => {
    const e = editor(page);
    await e.paste('l: [a, b]\n');
    await caretAt(e, 'l: [a,'.length);
    await page.keyboard.press('Enter');
    await page.keyboard.type('z, ');
    await expect.poll(() => saved(page)).toBe('l: [a,\n  z, b]\n');
  });
});

test.describe('XML typing', () => {
  setupEditor(test, '?language=xml');

  test('</ writes the end tag of the open element', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <a>x\n</r>\n');
    await caretAt(e, '<r>\n  <a>x'.length);
    await page.keyboard.type('</');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n</r>\n');
    // Enter steps past it.
    await page.keyboard.press('Enter');
    await page.keyboard.type('<b>1');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n  <b>1</b>\n</r>\n');
  });

  test('an end tag typed whole after </ is written once', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <a>x\n</r>\n');
    await caretAt(e, '<r>\n  <a>x'.length);
    await page.keyboard.type('</a>');
    await expect.poll(() => saved(page)).toBe('<r>\n  <a>x</a>\n</r>\n');
  });

  test('an attribute quote closes before />', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r/>\n');
    await caretAt(e, 2);
    await page.keyboard.type(' a="1');
    await expect.poll(() => saved(page)).toBe('<r a="1"/>\n');
    await page.keyboard.type('"');
    await expect.poll(() => e.value()).toBe('<r a="1"/>\n');
  });
});

test.describe('HCL typing', () => {
  setupEditor(test, '?language=hcl');

  test('"${ writes its }', async ({ page }) => {
    const e = editor(page);
    await e.paste('n = 1\n');
    await caretAt(e, 'n = 1\n'.length);
    await page.keyboard.type('a = "${var.x}"');
    await expect.poll(() => saved(page)).toBe('n = 1\na = "${var.x}"');
  });
});

test.describe('find', () => {
  setupEditor(test, '?language=yaml');

  test('a pattern matching empty text finds and replaces at each place', async ({ page }) => {
    const e = editor(page);
    await e.paste('a: 1\nb: 2\n');
    await caretAt(e, 0);
    await page.keyboard.press('Control+h');
    const widget = page.locator('.config-editor__find');
    await widget.getByRole('button', { name: 'Use regular expression' }).click();
    await widget.getByRole('textbox', { name: 'Find' }).fill('^');
    await expect(widget.locator('.config-editor__find-count')).toHaveText('1 of 3');
    await widget.getByRole('textbox', { name: 'Replace' }).fill('# ');
    await widget.getByRole('button', { name: 'Replace all' }).click();
    await expect.poll(() => saved(page)).toBe('# a: 1\n# b: 2\n# ');
  });
});
