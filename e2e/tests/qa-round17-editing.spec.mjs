import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 17 Edit-as saves, typed in another language's view: a value
// starting with a YAML indicator is quoted, a block in a list item's map and
// a compact nested sequence are spliced in place with the comments kept, a
// carriage return is escaped, an XML comment in text survives.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Puts the caret just after `needle` (its `offset`-th character with one).
async function caretAfter(e, needle, offset = needle.length) {
  await e.input.evaluate(
    (element, [find, at]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + at, start + at);
    },
    [needle, offset],
  );
}

test.describe('YAML edited as JSON', () => {
  setupEditor(test, '?language=yaml');

  test('a value given a leading % or a YAML 1.1 word is written quoted', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\ns: x # sc\nt: z\n');
    await expect.poll(() => saved(page)).toContain('t: z');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"x"');
    await caretAfter(e, '"x', 1);
    await page.keyboard.type('%');
    await expect.poll(() => saved(page)).toBe('# c\ns: "%x" # sc\nt: z\n');
    await caretAfter(e, '"z', 1);
    await page.keyboard.press('Delete');
    await page.keyboard.type('yes');
    await expect.poll(() => saved(page)).toBe('# c\ns: "%x" # sc\nt: "yes"\n');
  });

  test('a block in a list item map and a compact nested list keep the comments', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nm:\n  - k: |\n      a\n    # kc\n    j: 2 # jc\n  - - b # bc\n    - c\nn: 1 # nc\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"c"');
    await caretAfter(e, '"a', 2);
    await page.keyboard.type('\\tq');
    await expect.poll(() => saved(page)).toBe(host.replace('  - k: |\n      a\n', '  - k: |\n      a\tq\n'));
    await caretAfter(e, '"c', 2);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toContain('    - cQ\nn: 1 # nc\n');
    expect(await saved(page)).toContain('  - - b # bc\n');
  });
});

test.describe('a carriage return typed in another view', () => {
  for (const [language, host, needle, expected] of [
    ['toml', '# c\nml = """\nl1\nl2\n"""\nn = 1\n', 'l1', '# c\nml = """\nl1\\r\nl2\n"""\nn = 1\n'],
    ['hcl', '# c\nml = <<EOT\nl1\nl2\nEOT\nn = 1\n', 'l1', '# c\nml = "l1\\r\\nl2\\n"\nn = 1\n'],
  ]) {
    test.describe(language, () => {
      setupEditor(test, `?language=${language}`);
      test('is written as an escape', async ({ page }) => {
        const e = editor(page);
        await e.paste(host);
        await expect.poll(() => saved(page)).toContain('n = 1');
        await pick(page, 'json');
        await expect.poll(() => e.value()).toContain(needle);
        await caretAfter(e, needle);
        await page.keyboard.type('\\r');
        await expect.poll(() => saved(page)).toBe(expected);
      });
    });
  }
});

test.describe('XML text around a comment edited as YAML', () => {
  setupEditor(test, '?language=xml');

  test('keeps the comment and the CDATA beside text with an attribute', async ({ page }) => {
    const e = editor(page);
    const host = '<root>\n  <p k="v">alpha<!-- c -->beta</p>\n  <q k="v">t<![CDATA[x < y]]>u</q>\n</root>\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('CDATA');
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('alphabeta');
    await caretAfter(e, 'alphabeta');
    await page.keyboard.type('X');
    await expect.poll(() => saved(page)).toContain('<p k="v">alpha<!-- c -->betaX</p>');
    await caretAfter(e, 'tx < yu', 7);
    await page.keyboard.type('Z');
    await expect.poll(() => saved(page)).toBe(host.replace('alpha<!-- c -->beta', 'alpha<!-- c -->betaX').replace('u</q>', 'uZ</q>'));
  });
});
