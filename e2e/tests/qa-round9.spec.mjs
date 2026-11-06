import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Ninth-round findings, replayed with real keys. The classes behind them are
// covered by the unit matrices (`completion_matrix`, `editing_matrix`).

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

const selection = (e) => e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);

test.describe('yaml', () => {
  setupEditor(test, '?language=yaml');

  test('a rename leaves the caret after the new key, the next key typed after it', async ({ page }) => {
    const e = await at(page, 'limits:\n  max_h¦pending: 1\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('max_handshakes');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('max_handshakes'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('limits:\n  max_handshakes: 1\n');
    const [start, end] = await selection(e);
    expect(start).toBe(end);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('xml', () => {
  setupEditor(test, '?language=xml');

  test('an attribute accepted right before > keeps the tag whole', async ({ page }) => {
    const e = await at(page, '<tcp ¦>\n</tcp>\n');
    await page.keyboard.type('no');
    await expect.poll(() => e.keys()).toContain('nodelay');
    await page.keyboard.press('Enter');
    expect(await e.value()).toMatch(/^<tcp nodelay="[^"]*">\n<\/tcp>\n$/);
  });

  test('Ctrl+/ twice on a selection starting mid-line gives the text back', async ({ page }) => {
    const text = '<web>\n  <path>a</path>\n  <host>b</host>\n</web>\n';
    const e = await at(page, text.replace('a</path>', '¦a</path>').replace('b</host>', 'b¦</host>'));
    await page.keyboard.press('Control+/');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe(text);
  });
});

test.describe('toml', () => {
  setupEditor(test, '?language=toml');

  test('accepting the key already written before a dot changes nothing', async ({ page }) => {
    const e = await at(page, 'tcp¦.nodelay = true\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('tcp');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('tcp'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('tcp.nodelay = true\n');
  });
});

test.describe('hcl', () => {
  setupEditor(test, '?language=hcl');

  test('Ctrl+/ on a block comment keeps the caret on its text', async ({ page }) => {
    const e = await at(page, 'tcp {\n  /* nodelay¦ = true */\n}\n');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('tcp {\n  nodelay = true\n}\n');
    const [start, end] = await selection(e);
    expect([start, end]).toEqual(['tcp {\n  nodelay'.length, 'tcp {\n  nodelay'.length]);
  });
});

test.describe('json', () => {
  setupEditor(test, '?language=json');

  test('a key typed at another key’s opening quote goes before it on its own line', async ({ page }) => {
    const e = await at(page, '{\n  "¦tcp": {}\n}\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toContain('clients');
    expect(await e.keys()).not.toContain('tcp');
    const keys = await e.keys();
    for (let index = 0; index < keys.indexOf('clients'); index += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    const value = await e.value();
    expect(value).toContain('"clients": {');
    expect(value).toContain(',\n  "tcp": {}');
  });
});
