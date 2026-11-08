import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 23 editing: HCL's `${` and `%{` close inside a heredoc body as
// in a quoted string (the key handler skipped what the language closes
// in every multi-line string body), typed over and deleted with their
// opener; brackets there still do not pair.

const saved = (page) => page.locator('.demo__output-text').textContent();

// Puts the caret at `offset` of the text on screen.
const caretAt = (e, offset) =>
  e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);

test.describe('HCL heredoc templates', () => {
  setupEditor(test, '?language=hcl');

  const host = 'msg = <<EOT\nhi \nEOT\n';
  const body = 'msg = <<EOT\nhi '.length;

  for (const sigil of ['$', '%']) {
    test(`${sigil}{ writes its } in a heredoc, typed over`, async ({ page }) => {
      const e = editor(page);
      await e.paste(host);
      await caretAt(e, body);
      await page.keyboard.type(`${sigil}{`);
      await expect.poll(() => e.value()).toBe(`msg = <<EOT\nhi ${sigil}{}\nEOT\n`);
      await page.keyboard.type('x}');
      await expect.poll(() => saved(page)).toBe(`msg = <<EOT\nhi ${sigil}{x}\nEOT\n`);
    });
  }

  test('Backspace deletes the pair, and brackets do not pair', async ({ page }) => {
    const e = editor(page);
    await e.paste(host);
    await caretAt(e, body);
    await page.keyboard.type('${');
    await page.keyboard.press('Backspace');
    await expect.poll(() => e.value()).toBe('msg = <<EOT\nhi $\nEOT\n');
    await page.keyboard.type(' ( [');
    await expect.poll(() => saved(page)).toBe('msg = <<EOT\nhi $ ( [\nEOT\n');
  });
});
