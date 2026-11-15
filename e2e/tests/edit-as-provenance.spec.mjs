import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// A value moved or copied in an Edit-as view with the editor's own
// commands keeps its host form (QA E38-1, E38-2, E38-5). The view items
// carry links to their host items: Ctrl+X / Ctrl+C record the copied text
// with its links, Ctrl+V of that text takes them, Alt+Up/Down move them.

test.use({ permissions: ['clipboard-read', 'clipboard-write'] });

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

/** Puts the caret at the start of the first line of the view holding `text`,
 * `lines` lines selected below it. */
async function selectLines(page, e, text, lines = 1) {
  await e.input.evaluate((element, wanted) => {
    const at = element.value.indexOf(wanted);
    if (at < 0) throw new Error(`no ${wanted}`);
    const start = element.value.lastIndexOf('\n', at) + 1;
    element.focus();
    element.setSelectionRange(start, start);
  }, text);
  for (let line = 0; line < lines; line++) await page.keyboard.press('Shift+ArrowDown');
}

async function open(page, host, view) {
  const e = editor(page);
  await e.paste(host);
  await expect.poll(() => saved(page)).toBe(host);
  await pick(page, view);
  await expect.poll(() => e.value()).not.toBe(host);
  return e;
}

test.describe('a TOML host edited in a JSON view', () => {
  setupEditor(test, '?language=toml');

  const host = '[a]\nd = 1979-05-27T07:32:00Z\nk = 1\n\n[b]\nz = 1\ny = 2\n';

  for (const pausing of [false, true]) {
    test(`a key cut and pasted over a selected line keeps its datetime${pausing ? ' after a pause' : ''}`, async ({
      page,
    }) => {
      const e = await open(page, host, 'json');
      await selectLines(page, e, '"d"');
      await page.keyboard.press('Control+X');
      if (pausing) await page.waitForTimeout(1300);
      await selectLines(page, e, '"z"');
      await page.keyboard.press('Control+V');
      await expect.poll(() => saved(page)).toBe('[a]\nk = 1\n\n[b]\nd = 1979-05-27T07:32:00Z\ny = 2\n');
    });
  }

  test('a list item copied keeps its hex number', async ({ page }) => {
    const e = await open(page, 'l = [\n  0x1F,\n  2,\n]\n', 'json');
    await selectLines(page, e, '31');
    await page.keyboard.press('Control+C');
    await selectLines(page, e, '    2', 0);
    await page.keyboard.press('Control+V');
    await expect.poll(() => saved(page)).toBe('l = [\n  0x1F,\n  0x1F,\n  2,\n]\n');
  });
});

test.describe('a TOML host edited in a YAML view', () => {
  setupEditor(test, '?language=toml');

  test('a key moved down a line into the next table keeps its hex number', async ({ page }) => {
    const e = await open(page, '[a]\nk = 1\nh = 0xff\n\n[b]\nz = 1\n', 'yaml');
    await selectLines(page, e, 'h: ', 0);
    await page.keyboard.press('Alt+ArrowDown');
    await expect.poll(() => saved(page)).toBe('[a]\nk = 1\n\n[b]\nh = 0xff\nz = 1\n');
  });
});

test.describe('an XML host edited in a YAML view', () => {
  setupEditor(test, '?language=xml');

  test('an element with attributes pasted over a selected line stays an element with them', async ({ page }) => {
    const host = '<r>\n  <a>\n    <t n="1"/>\n    <k>1</k>\n  </a>\n  <b>\n    <z>1</z>\n    <y>2</y>\n  </b>\n</r>\n';
    const e = await open(page, host, 'yaml');
    await selectLines(page, e, '    t:', 2);
    await page.keyboard.press('Control+X');
    await selectLines(page, e, '    z:');
    await page.keyboard.press('Control+V');
    await expect
      .poll(() => saved(page))
      .toBe('<r>\n  <a>\n    <k>1</k>\n  </a>\n  <b>\n    <t n="1"/>\n    <y>2</y>\n  </b>\n</r>\n');
  });
});
