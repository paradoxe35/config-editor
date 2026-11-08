import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 18: a comment written over several lines (XML `<!-- … -->`, HCL
// `/* … */`) was colored as a comment on its first line only; the lines
// after it showed as tags and values. Every line of it is a comment now, up
// to its end, and the rest of its last line is colored by the language.

const CASES = {
  xml: {
    text: '<config>\n  <!-- <listen>7835</listen>\n  <public_address>a.example.com</public_address> --> <mode>x</mode>\n  <limits>\n    <!--\n    <max_pending>5</max_pending>\n    -->\n  </limits>\n</config>\n',
    // Line (1-based) → the text of its comment, and the keys after it.
    comments: { 2: '<!-- <listen>7835</listen>', 3: '  <public_address>a.example.com</public_address> -->', 5: '<!--', 6: '    <max_pending>5</max_pending>', 7: '    -->' },
    after: { 3: ['mode', 'mode'] },
  },
  hcl: {
    text: 'listen = 7835\n/* public_address = "x"\n   insecure_plaintext = true */ mode = "x"\nlimits {\n  /*\n  max_pending = 5\n  */\n}\n',
    comments: { 2: '/* public_address = "x"', 3: '   insecure_plaintext = true */', 5: '/*', 6: '  max_pending = 5', 7: '  */' },
    after: { 3: ['mode'] },
  },
};

for (const [language, { text, comments, after }] of Object.entries(CASES)) {
  test.describe(`a ${language} comment over several lines`, () => {
    setupEditor(test, `?language=${language}`);

    test('is colored as a comment on every line, in the comment color', async ({ page }) => {
      const e = editor(page);
      await e.paste(text);
      const rows = page.locator('.config-editor__highlight .config-editor__code-line');
      for (const [line, comment] of Object.entries(comments)) {
        const row = rows.nth(Number(line) - 1);
        await expect(row.locator('.config-editor__comment')).toHaveText([comment]);
        // No tag or key colors inside the comment.
        const keys = await row.locator('.config-editor__key').allTextContents();
        expect(keys, `line ${line}`).toEqual(after[line] ?? []);
      }
      // The comment color is the theme's, and differs from a key's.
      const color = (selector) =>
        rows.nth(5).locator(selector).first().evaluate((element) => getComputedStyle(element).color);
      const key = await rows.nth(0).locator('.config-editor__key').first().evaluate((element) => getComputedStyle(element).color);
      expect(await color('.config-editor__comment')).not.toBe(key);
      // Its opening deleted: the lines below it are code again.
      const open = language === 'xml' ? '<!--' : '/*';
      const lines = text.split('\n');
      lines[4] = lines[4].replace(open, ' '.repeat(open.length));
      await e.paste(lines.join('\n'));
      await expect(rows.nth(5).locator('.config-editor__key').first()).toHaveText('max_pending');
      await expect(rows.nth(5).locator('.config-editor__comment')).toHaveCount(0);
    });
  });
}
