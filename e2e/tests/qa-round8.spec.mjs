import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Eighth-round findings, replayed with real keys. The classes behind them are
// covered by the unit matrices (`completion_matrix`, `editing_matrix`,
// `save_properties`).

const saved = (page) => page.locator('.demo__output-text').textContent();

async function at(page, marked) {
  const e = editor(page);
  const first = marked.indexOf('¦');
  const last = marked.lastIndexOf('¦');
  const [start, end] = first === last ? [first, first] : [first, last - 1];
  await e.paste(marked.replaceAll('¦', ''));
  await e.input.evaluate(
    (element, [from, to]) => {
      element.focus();
      element.setSelectionRange(from, to);
    },
    [start, end],
  );
  return e;
}

test.describe('xml', () => {
  setupEditor(test, '?language=xml');

  test('a name accepted inside an element with children renames both tags', async ({ page }) => {
    const e = await at(page, '<burrow config-root="true">\n  <t¦cp>\n    <nodelay>true</nodelay>\n  </tcp>\n</burrow>');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('metrics');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('metrics'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    const value = await e.value();
    expect(value).toContain('<metrics>');
    expect(value).toContain('</metrics>');
    expect(value).not.toContain('tcp');
  });

  test('> typed before a tag’s own > moves over it', async ({ page }) => {
    const e = await at(page, '<tcp¦></tcp>');
    await page.keyboard.type('>');
    expect(await e.value()).toBe('<tcp></tcp>');
  });
});

test.describe('hcl', () => {
  setupEditor(test, '?language=hcl');

  test('a key typed in front of a block goes before it, the block kept', async ({ page }) => {
    const e = await at(page, '¦tcp {\n  nodelay = true\n}\n');
    await page.keyboard.type('hea');
    await expect.poll(() => e.keys()).toContain('heartbeat');
    await page.keyboard.press('Enter');
    const value = await e.value();
    expect(value).toContain('heartbeat {');
    expect(value).toContain('tcp {\n  nodelay = true\n}');
  });
});

test.describe('yaml', () => {
  setupEditor(test, '?language=yaml');

  test('Tab on a selection starting inside the indentation keeps its column', async ({ page }) => {
    const e = await at(page, 'tcp:\n ¦ nodelay: true\n  keepalive_secs: 3¦0\n');
    await page.keyboard.press('Tab');
    const selection = await e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);
    expect(selection[0]).toBe(5 + 3);
    await page.keyboard.press('Shift+Tab');
    expect(await e.value()).toBe('tcp:\n  nodelay: true\n  keepalive_secs: 30\n');
    const back = await e.input.evaluate((element) => element.selectionStart);
    expect(back).toBe(5 + 1);
  });
});

test.describe('Edit-as next to an equal sibling', () => {
  setupEditor(test, '?language=yaml');

  test('a block-style sibling equal to a one-line container keeps its style and comment', async ({ page }) => {
    const e = editor(page);
    await e.paste('# c\nlisten: 1\ntcp: {nodelay: true}\nudp:\n  nodelay: true # u\n');
    await page.locator('.demo__edit-select').selectOption('json');
    await expect(e.input).toHaveValue(/"udp"/);
    await e.input.evaluate((element) => {
      const at = element.value.indexOf('"listen": 1,') + '"listen": 1,'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('"insecure_plaintext": true,', { delay: 10 });
    await page.keyboard.press('Escape');
    await expect.poll(() => saved(page)).toContain('insecure_plaintext: true');
    expect(await saved(page)).toContain('udp:\n  nodelay: true # u');
    expect(await saved(page)).toContain('tcp: {nodelay: true}');
  });
});
