import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 24 editing, as a user makes the saves: a leading byte order
// mark stays through an edit of the first key from another view, a YAML
// root written as a flow collection keeps its form, an XML attribute stays
// one when the child of its name goes, and tagged YAML list items move
// with their tags.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Puts the caret just after `needle`.
async function caretAfter(e, needle) {
  await e.input.evaluate((element, find) => {
    const start = element.value.indexOf(find);
    if (start < 0) throw new Error(`no ${find}`);
    element.focus();
    element.setSelectionRange(start + find.length, start + find.length);
  }, needle);
}

// Selects `needle` and types `text` over it.
async function replace(page, e, needle, text) {
  await e.input.evaluate((element, find) => {
    const start = element.value.indexOf(find);
    if (start < 0) throw new Error(`no ${find}`);
    element.focus();
    element.setSelectionRange(start, start + find.length);
  }, needle);
  await page.keyboard.insertText(text);
}

const BOM = '\u{FEFF}';

for (const [language, host, edited] of [
  ['yaml', `${BOM}a: 1 # c\nb: 2\n`, `${BOM}a: 10 # c\nb: 2\n`],
  ['toml', `${BOM}a = 1 # c\nb = 2\n`, `${BOM}a = 10 # c\nb = 2\n`],
]) {
  test.describe(`a ${language} host with a byte order mark`, () => {
    setupEditor(test, `?language=${language}`);

    test('keeps the mark and the comment when its first key is edited as JSON', async ({ page }) => {
      const e = editor(page);
      await e.paste(host);
      await expect.poll(() => saved(page)).toBe(host);
      await pick(page, 'json');
      await expect.poll(() => e.value()).toContain('"a": 1');
      await caretAfter(e, '"a": 1');
      await page.keyboard.type('0');
      await expect.poll(() => saved(page)).toBe(edited);
    });
  });
}

test.describe('a YAML root written as a flow collection', () => {
  setupEditor(test, '?language=yaml');

  test('keeps its form and comments through an edit as JSON', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\n[a, b] # t\n');
    await expect.poll(() => saved(page)).toBe('# c\n[a, b] # t\n');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"b"');
    await caretAfter(e, '"b');
    await page.keyboard.type('x');
    await expect.poll(() => saved(page)).toBe('# c\n[a, bx] # t\n');
  });

  test('tagged list items move with their tags', async ({ page }) => {
    const e = editor(page);
    const host = 'l:\n  # A\n  - !A {x: 1}\n  # B\n  - !B {x: 2}\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toBe(host);
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"x": 2');
    const shown = await e.value();
    const list = shown.slice(shown.indexOf('['), shown.lastIndexOf(']') + 1);
    await replace(page, e, list, '[{"x": 2}, {"x": 1}]');
    await expect.poll(() => saved(page)).toBe('l:\n  # B\n  - !B {x: 2}\n  # A\n  - !A {x: 1}\n');
  });
});

test.describe('an XML attribute beside a child of its name', () => {
  setupEditor(test, '?language=xml');

  test('stays an attribute when the child is removed as JSON', async ({ page }) => {
    const e = editor(page);
    const host = '<r a="1">\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toBe(host);
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"@a"');
    const shown = await e.value();
    const child = shown.match(/\n\s*"a": 2,/)[0];
    await replace(page, e, child, '');
    await expect.poll(() => saved(page)).toBe('<r a="1">\n  <z>0</z> <!-- z -->\n</r>\n');
  });
});
