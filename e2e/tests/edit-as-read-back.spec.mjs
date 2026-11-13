import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Edit-as saves of an XML host are read back as the view holds them
// (hard-bar QA D23/D24/D25): a structural edit keeps the comments and
// processing instructions an element shares one line with, and an edit the
// host would read back otherwise (an attribute moved below the text, a
// prefix left unbound) waits with the reason on the value.

const saved = (page) => page.locator('.demo__output-text').textContent();
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);
// The Edit-as problem saying `text` (the demo's schema marks the file too).
const marker = (page, text) => page.locator(`.config-editor__diagnostic-marker[data-message*="${text}"]`);

// Replaces the first `find` in the view with `text`, as typed.
async function replace(page, e, find, text) {
  await e.input.evaluate((element, wanted) => {
    const start = element.value.indexOf(wanted);
    if (start < 0) throw new Error(`no ${wanted}`);
    element.focus();
    element.setSelectionRange(start, start + wanted.length);
  }, find);
  if (text) await page.keyboard.insertText(text);
  else await page.keyboard.press('Backspace');
}

test.describe('an XML host edited in a YAML view', () => {
  setupEditor(test, '?language=xml');

  const host = '<r xmlns:x="urn:a" a="1"><!-- c --><?pi x?><s x:attr="3">t</s><u>v</u></r>\n';

  async function open(page) {
    const e = editor(page);
    await e.paste(host);
    await expect.poll(() => saved(page)).toBe(host);
    await pick(page, 'yaml');
    await expect.poll(() => e.value()).toContain('x:attr: 3');
    return e;
  }

  test('keeps the comment and processing instruction when an element is removed', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, '  u: v\n', '');
    await expect.poll(() => saved(page)).toBe('<r xmlns:x="urn:a" a="1"><!-- c --><?pi x?><s x:attr="3">t</s></r>\n');
  });

  test('waits when a declaration a prefix needs is removed', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, '  xmlns:x: urn:a\n', '');
    await expect(marker(page, 'bound to no namespace')).toHaveCount(1);
    expect(await saved(page)).toBe(host);
  });

  test('waits when an attribute is moved below the text', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, '    x:attr: 3\n    "#text": t\n', '    "#text": t\n    x:attr: 3\n');
    await expect(marker(page, 'another place than the view moved it to')).toHaveCount(1);
    expect(await saved(page)).toBe(host);
  });

  test('waits when an attribute is renamed to an undeclared prefix, and saves it declared', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, 'x:attr: 3', 'y:attr: 3');
    await expect(marker(page, 'prefix y of y:attr is bound to no namespace')).toHaveCount(1);
    expect(await saved(page)).toBe(host);
    await replace(page, e, '  xmlns:x: urn:a\n', '  xmlns:x: urn:a\n  xmlns:y: urn:b\n');
    await expect
      .poll(() => saved(page))
      .toBe('<r xmlns:x="urn:a" xmlns:y="urn:b" a="1"><!-- c --><?pi x?><s y:attr="3">t</s><u>v</u></r>\n');
  });

  test('writes attributes swapped in the view in that order', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, '  xmlns:x: urn:a\n  a: 1\n', '  a: 1\n  xmlns:x: urn:a\n');
    await expect.poll(() => saved(page)).toBe('<r a="1" xmlns:x="urn:a"><!-- c --><?pi x?><s x:attr="3">t</s><u>v</u></r>\n');
  });

  test('waits when an element is renamed with a prefix', async ({ page }) => {
    const e = await open(page);
    await replace(page, e, '  u: v', '  x:u: v');
    await expect(marker(page, 'only as an attribute')).toHaveCount(1);
    expect(await saved(page)).toBe(host);
  });
});
