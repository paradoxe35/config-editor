import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Tenth-round findings, replayed with real keys. The classes behind them are
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

async function accept(page, e, label) {
  await expect.poll(() => e.keys()).toContain(label);
  const keys = await e.keys();
  for (let index = 0; index < keys.indexOf(label); index += 1) await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
}

test.describe('json', () => {
  setupEditor(test, '?language=json');

  test('a key typed after another key’s opening quote goes before it', async ({ page }) => {
    const e = await at(page, '{"listen": 7835, "¦tcp": {"nodelay": true}}');
    await page.keyboard.type('ud');
    await accept(page, e, 'udp');
    const value = JSON.parse(await e.value());
    expect(Object.keys(value)).toEqual(['listen', 'udp', 'tcp']);
    expect(value.tcp).toEqual({ nodelay: true });
  });
});

test.describe('toml', () => {
  setupEditor(test, '?language=toml');

  test('a table typed in front of a header goes before it', async ({ page }) => {
    const e = await at(page, 'listen = 7835\n[¦tcp]\nnodelay = true\n');
    await page.keyboard.type('ud');
    await accept(page, e, 'udp');
    const value = await e.value();
    expect(value).toContain('[udp]');
    expect(value).toContain('[tcp]\nnodelay = true');
  });

  test('a table renamed by a completion takes its sub-tables along', async ({ page }) => {
    const e = await at(page, '[t¦cp]\nnodelay = true\n[tcp.x]\na = 1\n');
    await page.keyboard.press('Control+Space');
    await accept(page, e, 'metrics');
    expect(await e.value()).toBe('[metrics]\nnodelay = true\n[metrics.x]\na = 1\n');
  });
});

test.describe('yaml', () => {
  setupEditor(test, '?language=yaml');

  test('a suggestion accepted in the indentation lines up with the line', async ({ page }) => {
    const e = await at(page, 'tcp:\n¦  nodelay: true\n');
    await page.keyboard.press('Control+Space');
    await accept(page, e, 'keepalive_secs');
    const value = await e.value();
    expect(value).toMatch(/^tcp:\n  keepalive_secs: \d+\n  nodelay: true\n$/);
  });

  test('Ctrl+/ in a block scalar comments the whole entry and back', async ({ page }) => {
    const text = 'listen: 1\nnote: |\n  line1\n  line2\nbind: x\n';
    const e = await at(page, text.replace('line1', 'line1¦'));
    await page.keyboard.press('Control+/');
    const once = await e.value();
    expect(once).not.toContain('  # line1');
    expect(once).toContain('# note: |');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe(text);
  });
});

test.describe('hcl', () => {
  setupEditor(test, '?language=hcl');

  test('Ctrl+/ inside a block comment over lines removes it', async ({ page }) => {
    const e = await at(page, 'tcp {\n  /* nodelay = true\n  keepalive_secs¦ = 30 */\n}\n');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('tcp {\n  nodelay = true\n  keepalive_secs = 30\n}\n');
  });

  test('Enter in a one-line block opens it over lines', async ({ page }) => {
    const e = await at(page, 'tcp { nodelay = true¦ }\n');
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('tcp {\n  nodelay = true\n  \n}\n');
  });

  test('a closing brace typed above the one written for the block is not doubled', async ({ page }) => {
    const e = await at(page, '¦');
    await page.keyboard.type('tcp {');
    await page.keyboard.press('Enter');
    await page.keyboard.type('nodelay = true');
    await page.keyboard.press('Enter');
    await page.keyboard.type('}');
    expect(await e.value()).toBe('tcp {\n  nodelay = true\n}');
  });
});

test.describe('xml', () => {
  setupEditor(test, '?language=xml');

  test('an end tag typed where the editor already wrote it is not doubled', async ({ page }) => {
    const e = await at(page, '¦');
    await page.keyboard.type('<tcp>');
    await page.keyboard.press('Enter');
    await page.keyboard.type('<nodelay>true');
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('</tcp>');
    expect((await e.value()).match(/<\/tcp>/g)).toHaveLength(1);
  });

  test('an end tag typed right before its written copy is not doubled', async ({ page }) => {
    const e = await at(page, '¦');
    await page.keyboard.type('<tcp></tcp>');
    expect(await e.value()).toBe('<tcp></tcp>');
  });
});
