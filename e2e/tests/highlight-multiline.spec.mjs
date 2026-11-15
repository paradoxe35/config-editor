import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Highlighting of constructs written over several lines, in every language.

async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

// The caret position is given separately: `|` is YAML syntax here.
async function atOffset(page, text, offset) {
  const e = editor(page);
  await e.paste(text);
  await e.input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, offset);
  return e;
}

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

test.describe('multi-line string bodies are text, not code', () => {
  test.describe('toml', () => {
    setupEditor(test, '?language=toml');

    test("Enter between brackets in a ''' string is a plain line break", async ({ page }) => {
      const text = "script = '''\necho []\n'''\n";
      const e = await atOffset(page, text, text.indexOf('[]') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe("script = '''\necho [\n]\n'''\n");
    });

    test('Enter in a """ string does not step out of braces', async ({ page }) => {
      const text = 'tpl = """\n{}\n"""\n';
      const e = await atOffset(page, text, text.indexOf('{}') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('tpl = """\n{\n}\n"""\n');
    });

    test('a key = value line in a string body is not colored as a key', async ({ page }) => {
      const e = editor(page);
      await e.paste("motd = '''\nkey = 1\n'''\n");
      const line = page.locator('.config-editor__code-line[data-line="2"]');
      await expect(line).toContainText('key = 1');
      await expect(line.locator('.config-editor__key')).toHaveCount(0);
    });
  });

  test.describe('hcl', () => {
    setupEditor(test, '?language=hcl');

    test('Enter between braces in a heredoc keeps the line indentation', async ({ page }) => {
      const text = 'a = <<-EOF\n    if [ -f x ]; then {}\n  EOF\n';
      const e = await atOffset(page, text, text.indexOf('{}') + 1);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('a = <<-EOF\n    if [ -f x ]; then {\n    }\n  EOF\n');
    });
  });

  test.describe('yaml', () => {
    setupEditor(test, '?language=yaml');

    test('Enter after a colon in a block scalar keeps the indentation', async ({ page }) => {
      const text = 'script: |\n  run:\n';
      const e = await atOffset(page, text, text.indexOf('run:') + 4);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('script: |\n  run:\n  \n');
    });

    test('Enter after a dash line in a block scalar does not continue a list', async ({ page }) => {
      const text = 'a: |\n  - x\n';
      const e = await atOffset(page, text, text.indexOf('- x') + 3);
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('a: |\n  - x\n  \n');
    });

    test('a ] typed in a block scalar is not moved', async ({ page }) => {
      const text = 'a: |\n  x [\n      \n';
      const e = await atOffset(page, text, text.indexOf('      ') + 6);
      await page.keyboard.type(']');
      expect(await e.value()).toBe('a: |\n  x [\n      ]\n');
    });
  });

  test.describe('xml', () => {
    setupEditor(test, '?language=xml');

    test('Enter between tags inside a comment is a plain line break', async ({ page }) => {
      const text = '<!-- <a></a> -->';
      const e = await atOffset(page, text, text.indexOf('</a>'));
      await page.keyboard.press('Enter');
      expect(await e.value()).toBe('<!-- <a>\n</a> -->');
    });
  });
});

// QA round 18: a comment written over several lines (XML `<!-- … -->`, HCL
// `/* … */`) was colored as a comment on its first line only; the lines
// after it showed as tags and values. Every line of it is a comment now, up
// to its end, and the rest of its last line is colored by the language.
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

// A group of its own (its own helpers).
test.describe(() => {
  // QA round 19 (UX): every line of a construct written over several lines is
  // colored as what it is (TOML arrays and `"""` strings, YAML quoted, plain
  // and flow nodes, XML start tags, attribute values and CDATA, HCL
  // heredocs); an HCL comment marker inside a string in a template sequence
  // (`"${"/*"}"`) is no comment; a key of several `oneOf` options names each
  // and a `const` key shows its values; and the suggestion list wraps around
  // at both ends in every language.
  // Each row of the code layer as `k(…)` per key token, `s` string, `v`
  // value, `l` literal, `c` comment, `p` punctuation, `_` uncolored text.
  const rowTokens = (page) =>
    page.evaluate(() => {
      const letters = { key: 'k', string: 's', value: 'v', literal: 'l', comment: 'c', punctuation: 'p', reference: 'r', interpolation: 'i' };
      return [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')].map((row) => {
        const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
        const runs = [];
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const kind = [...node.parentElement.classList]
            .map((name) => letters[name.replace('config-editor__', '')])
            .find(Boolean) ?? '_';
          const last = runs.at(-1);
          // One token may be cut in pieces (by marks); two tokens never merge.
          if (last && last.kind === kind && last.element === node.parentElement) last.text += node.textContent;
          else runs.push({ kind, text: node.textContent, element: node.parentElement });
        }
        return runs
          .flatMap(({ kind, text }) => (kind === '_' ? text.trim().split(/\s+/) : [text.trim()]).filter(Boolean).map((part) => `${kind}(${part})`))
          .join(' ');
      });
    });

  const CASES = {
    toml: {
      text: 'clients = [\n  "a", # one\n  """b\nc""", 2,\n]\ns = """\nline two"""\nlisten = 7835\n',
      rows: ['k(clients) p(=) p([)', 's("a") p(,) c(# one)', 's("""b)', 's(c""") p(,) l(2) p(,)', 'p(])', 'k(s) p(=) s(""")', 's(line two""")', 'k(listen) p(=) l(7835)'],
    },
    yaml: {
      text: 'q: "line one\n  line two" # c\np: plain one\n  plain two\nf: {max: 5,\n  other: "x"} # c\nlisten: 7835\n',
      rows: ['k(q) p(:) s("line one)', 's(line two") c(# c)', 'k(p) p(:) v(plain one)', 'v(plain two)', 'k(f) p(:) p({) k(max) p(:) l(5) p(,)', 'k(other) p(:) s("x") p(}) c(# c)', 'k(listen) p(:) l(7835)'],
    },
    xml: {
      text: '<a x="one\n  two" y="3">text</a>\n<c><![CDATA[c1\nc2\n]]></c>\n<listen>7835</listen>\n',
      rows: ['p(<) k(a) k(x) p(=) s("one)', 's(two") k(y) p(=) s("3") p(>) v(text) p(</) k(a) p(>)', 'p(<) k(c) p(>) p(<![CDATA[) s(c1)', 's(c2)', 'p(]]>) p(</) k(c) p(>)', 'p(<) k(listen) p(>) v(7835) p(</) k(listen) p(>)'],
    },
    hcl: {
      text: 'a = "${"/*"}"\nd = "a${"#"}b" # c\nh = <<EOT\nx ${"/*"} y\nEOT\nlisten = 7835\n',
      // QA 28: the template sequences in strings and heredocs are interpolations.
      rows: ['k(a) p(=) s(") i(${"/*"}) s(")', 'k(d) p(=) s("a) i(${"#"}) s(b") c(# c)', 'k(h) p(=) s(<<EOT)', 's(x) i(${"/*"}) s(y)', 's(EOT)', 'k(listen) p(=) l(7835)'],
    },
  };


  for (const [language, { text, rows }] of Object.entries(CASES)) {
    test.describe(`${language} constructs over several lines`, () => {
      setupEditor(test, `?language=${language}`);

      test('color every line as what it is', async ({ page }) => {
        await editor(page).paste(text);
        await expect.poll(async () => (await rowTokens(page)).slice(0, rows.length)).toEqual(rows);
        // The string color reaches a construct's inner lines on screen.
        const color = (selector, index) =>
          page
            .locator('.config-editor__highlight .config-editor__code-line')
            .nth(index)
            .locator(selector)
            .first()
            .evaluate((element) => getComputedStyle(element).color);
        const stringRow = rows.findIndex((row, index) => index > 0 && row.startsWith('s('));
        const keyColor = await color('.config-editor__key', 0);
        expect(await color('.config-editor__string', stringRow)).not.toBe(keyColor);
      });
    });
  }

  test.describe('an HCL comment marker in a template sequence', () => {
    setupEditor(test, '?language=hcl');

    test('leaves the lines after it code', async ({ page }) => {
      await editor(page).paste('listen = 7835\npublic_address = "${"/*"}"\ninsecure_plaintext = false\n');
      const rows = page.locator('.config-editor__highlight .config-editor__code-line');
      await expect(rows.nth(2).locator('.config-editor__key')).toHaveText('insecure_plaintext');
      await expect(page.locator('.config-editor__highlight .config-editor__comment')).toHaveCount(0);
    });
  });
});
