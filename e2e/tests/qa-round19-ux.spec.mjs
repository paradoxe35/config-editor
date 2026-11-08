import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

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
    const letters = { key: 'k', string: 's', value: 'v', literal: 'l', comment: 'c', punctuation: 'p' };
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
    rows: ['k(a) p(=) s("${"/*"}")', 'k(d) p(=) s("a${"#"}b") c(# c)', 'k(h) p(=) s(<<EOT)', 's(x ${"/*"} y)', 's(EOT)', 'k(listen) p(=) l(7835)'],
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

test.describe('oneOf options in suggestions and hover', () => {
  setupEditor(test, '?schema=help');

  test('a key of several options names each, a const key its values', async ({ page }) => {
    const e = editor(page);
    await e.paste('dns:\n  \n');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Control+Space');
    const provider = e.options.filter({ has: page.locator('.config-editor__completion-key', { hasText: /^provider$/ }) });
    await expect(provider.locator('.config-editor__completion-type')).toHaveText('"manual" | "static"');
    await expect(provider.locator('.config-editor__completion-detail')).toHaveText('Manual, Static file');
    await page.keyboard.press('Escape');

    await e.paste('dns:\n  provider: static\n  path: /x\n');
    const at = await pointAt(page, 1, 4);
    await page.mouse.move(at.x, at.y, { steps: 4 });
    const card = page.locator('.config-editor__hover');
    await expect(card.locator('.config-editor__hover-key')).toHaveText('provider');
    await expect(card.locator('.config-editor__hover-type')).toHaveText('"static"');
    await expect(card.locator('.config-editor__hover-choice-title')).toHaveText('Static file');
  });
});

const LIST_STARTS = { yaml: '', json: '{\n  \n}\n', toml: '', hcl: '', xml: '' };

for (const [language, text] of Object.entries(LIST_STARTS)) {
  test.describe(`the ${language} suggestion list`, () => {
    setupEditor(test, `?language=${language}`);

    test('wraps around at both ends', async ({ page }) => {
      const e = editor(page);
      await e.paste(text);
      if (text) {
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('ArrowDown');
        await page.keyboard.press('End');
      }
      await page.keyboard.press('Control+Space');
      await expect(e.options.first()).toBeVisible();
      const count = await e.options.count();
      expect(count).toBeGreaterThan(2);
      const active = () =>
        e.options.evaluateAll((options) => options.findIndex((option) => option.classList.contains('config-editor__completion-active')));
      await expect.poll(active).toBe(0);
      await page.keyboard.press('ArrowUp');
      await expect.poll(active).toBe(count - 1);
      await page.keyboard.press('ArrowDown');
      await expect.poll(active).toBe(0);
    });
  });
}
