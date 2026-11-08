import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 19 Edit-as saves, typed in another language's view: an edit
// elsewhere keeps XML's `1.0` as written, a value typed in the XML view
// keeps the host's type (an emptied element is no `{}`, a TOML datetime
// retyped there stays a datetime), and YAML list items of maps moved in
// another view take their comments along.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Selects `needle` (or puts the caret after it with `collapse`).
async function select(e, needle, collapse = false) {
  await e.input.evaluate(
    (element, [find, end]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      const stop = start + find.length;
      element.setSelectionRange(end ? stop : start, stop);
    },
    [needle, collapse],
  );
}

test.describe('XML edited as JSON', () => {
  setupEditor(test, '?language=xml');

  test('an edit elsewhere keeps 1.0 and -0.0 as written', async ({ page }) => {
    const e = editor(page);
    const host = '<!-- top -->\n<r a="2.0">\n  <name>edge</name>\n  <version>1.0</version>\n  <z>-0.0</z>\n</r>\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('<z>');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"edge"');
    await select(e, 'edge', true);
    await page.keyboard.type('Q');
    await expect.poll(() => saved(page)).toBe(host.replace('edge', 'edgeQ'));
  });
});

test.describe('YAML edited as XML', () => {
  setupEditor(test, '?language=yaml');

  test('an emptied element saves the empty string, and a typed number stays a string', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nname: edge  # nc\nver: "1"\nport: 80\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('port: 80');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<name>edge</name>');
    await select(e, 'edge');
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).toMatch(/^# top\nname: (""|'')  # nc\nver: "1"\nport: 80\n$/);
    await page.keyboard.type('9');
    await expect.poll(() => saved(page)).toMatch(/^# top\nname: ("9"|'9')  # nc\n/);
    await select(e, '<ver>1', true);
    await page.keyboard.type('2');
    await expect.poll(() => saved(page)).toContain('ver: "12"\n');
  });

  test('map items moved in the JSON view take their comments', async ({ page }) => {
    const e = editor(page);
    const host = '# top\nl:\n  - name: a  # ac\n    v: 1\n  - name: b  # bc\n    v: 2\nn: 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n: 1');
    await pick(page, 'json');
    await expect.poll(() => e.value()).toContain('"b"');
    const shown = await e.value();
    const first = '{\n      "name": "a",\n      "v": 1\n    }';
    const second = '{\n      "name": "b",\n      "v": 2\n    }';
    await select(e, `${first},\n    ${second}`);
    await page.keyboard.insertText(`${second},\n    ${first}`);
    expect(shown).toContain(first);
    await expect.poll(() => saved(page)).toBe(
      '# top\nl:\n  - name: b  # bc\n    v: 2\n  - name: a  # ac\n    v: 1\nn: 1\n',
    );
  });
});

test.describe('TOML edited as XML', () => {
  setupEditor(test, '?language=toml');

  test('a datetime emptied and retyped waits, then is saved as a datetime', async ({ page }) => {
    const e = editor(page);
    const host = '# c\ndt = 1979-05-27T07:32:00Z  # dtc\nn = 1\n';
    await e.paste(host);
    await expect.poll(() => saved(page)).toContain('n = 1');
    await pick(page, 'xml');
    await expect.poll(() => e.value()).toContain('<dt>');
    await select(e, '1979-05-27T07:32:00Z');
    await page.keyboard.press('Delete');
    await page.keyboard.type('2');
    await expect(page.locator('.config-editor').first()).toContainText('not a TOML date or time');
    expect(await saved(page)).toBe(host);
    await page.keyboard.type('024-01-02T03:04:05Z');
    await expect.poll(() => saved(page)).toBe(host.replace('1979-05-27T07:32:00Z', '2024-01-02T03:04:05Z'));
  });
});
