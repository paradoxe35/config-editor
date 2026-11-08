import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 21 Edit-as saves: list items moved while others are added or
// removed keep their comments (scalars, XML elements with children, TOML
// inline tables), numbers and booleans typed in their own spelling in the
// XML view keep the host's type and the spelling, a quoted string emptied
// and typed again after the save keeps its quotes, and an HCL heredoc whose
// text gains its delimiter takes another one.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Replaces the first `needle` in the view with `text`, as one input.
async function replace(page, e, needle, text) {
  await e.input.evaluate((element, find) => {
    const start = element.value.indexOf(find);
    if (start < 0) throw new Error(`no ${find} in ${element.value}`);
    element.focus();
    element.setSelectionRange(start, start + find.length);
  }, needle);
  if (text) await page.keyboard.insertText(text);
  else await page.keyboard.press('Delete');
}

// Loads `host` and switches the view to `view`, waiting for `shown` there.
async function open(page, host, view, shown) {
  const e = editor(page);
  await e.paste(host);
  await expect.poll(() => saved(page)).toBe(host);
  await pick(page, view);
  await expect.poll(() => e.value()).toContain(shown);
  return e;
}

test.describe('YAML host', () => {
  setupEditor(test, '?language=yaml');

  test('items moved with one added keep their comment lines, as JSON', async ({ page }) => {
    const host = "tags:\n  # first tag\n  - web\n  - 'api'   # quoted\nn: 1\n";
    const e = await open(page, host, 'json', '"tags"');
    const list = (await e.value()).match(/"tags": (\[[^\]]*\])/)[1];
    await replace(page, e, list, '["new", "api", "web"]');
    await expect
      .poll(() => saved(page))
      .toBe("tags:\n  - new\n  - 'api'   # quoted\n  # first tag\n  - web\nn: 1\n");
  });

  test('numbers and booleans typed in their own spelling as XML keep their types', async ({ page }) => {
    const host = 'f: 1.50  # f\ni: 5\nb: true\n';
    const e = await open(page, host, 'xml', '<f>1.5</f>');
    await replace(page, e, '<f>1.5</f>', '<f>2.50</f>');
    await expect.poll(() => saved(page)).toBe('f: 2.50  # f\ni: 5\nb: true\n');
    await replace(page, e, '<i>5</i>', '<i>+6</i>');
    await replace(page, e, '<b>true</b>', '<b>False</b>');
    await expect.poll(() => saved(page)).toBe('f: 2.50  # f\ni: +6\nb: false\n');
  });

  test('a quoted string emptied, saved and typed again as JSON keeps its quotes', async ({ page }) => {
    const host = 'a:\n  s: "abc"  # c\n  t: \'def\'\nn: 1\n';
    const e = await open(page, host, 'json', '"abc"');
    await replace(page, e, 'abc', '');
    await replace(page, e, 'def', '');
    // The typing pause ends: the emptied strings are saved.
    await expect.poll(() => saved(page)).toBe('a:\n  s: ""  # c\n  t: \'\'\nn: 1\n');
    await replace(page, e, '""', '"xyz"');
    await replace(page, e, '""', '"uvw"');
    await expect.poll(() => saved(page)).toBe('a:\n  s: "xyz"  # c\n  t: \'uvw\'\nn: 1\n');
  });
});

test.describe('XML host', () => {
  setupEditor(test, '?language=xml');

  test('repeated elements with children reordered as JSON take their comments', async ({ page }) => {
    const host =
      '<r>\n  <!-- about a -->\n  <s name="a">\n    <p>1</p>\n  </s>\n  <!-- about b -->\n  <s name="b">\n    <p>2</p>\n  </s>\n</r>\n';
    const e = await open(page, host, 'yaml', 'name: a');
    const shown = await e.value();
    const items = shown.slice(shown.indexOf('    - name: a'));
    await replace(page, e, items, '    - name: b\n      p: 2\n    - name: a\n      p: 1\n');
    await expect
      .poll(() => saved(page))
      .toBe(
        '<r>\n  <!-- about b -->\n  <s name="b">\n    <p>2</p>\n  </s>\n  <!-- about a -->\n  <s name="a">\n    <p>1</p>\n  </s>\n</r>\n',
      );
  });
});

test.describe('TOML host', () => {
  setupEditor(test, '?language=toml');

  test('inline tables reordered as JSON take their comment lines', async ({ page }) => {
    const host = 's = [\n  # about a\n  { n = "a" },\n  # about b\n  { n = "b" },\n]\n';
    const e = await open(page, host, 'json', '"s"');
    const list = (await e.value()).match(/"s": (\[[\s\S]*\])/)[1];
    await replace(page, e, list, '[{"n": "b"}, {"n": "a"}, {"n": "c"}]');
    await expect
      .poll(() => saved(page))
      .toBe('s = [\n  # about b\n  { n = "b" },\n  # about a\n  { n = "a" },\n  { n = "c" },\n]\n');
  });
});

test.describe('HCL host', () => {
  setupEditor(test, '?language=hcl');

  test('a heredoc whose text gains its delimiter takes another one', async ({ page }) => {
    const host = 'script = <<EOT\nline one\nEOT\nz = 1\n';
    const e = await open(page, host, 'yaml', 'line one');
    await replace(page, e, 'line one', 'line one\n  EOT');
    await expect.poll(() => saved(page)).toBe('script = <<EOT_1\nline one\nEOT\nEOT_1\nz = 1\n');
  });
});
