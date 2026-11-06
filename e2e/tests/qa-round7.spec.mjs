import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Seventh-round findings, replayed with real keys. The language-wide classes
// behind them are covered by the unit matrices (`completion_matrix`,
// `editing_matrix`, `save_properties`).

const saved = (page) => page.locator('.demo__output-text').textContent();

async function at(page, marked) {
  const e = editor(page);
  const [start, end] = [marked.indexOf('¦'), marked.lastIndexOf('¦') - (marked.indexOf('¦') === marked.lastIndexOf('¦') ? 0 : 1)];
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

  test('an element name accepted in front of a tag keeps that element', async ({ page }) => {
    const e = await at(page, '<burrow config-root="true">\n  <¦tcp/>\n</burrow>');
    await page.keyboard.type('pu');
    await expect.poll(() => e.keys()).toContain('public_address');
    await page.keyboard.press('Enter');
    const value = await e.value();
    expect(value).toContain('<tcp/>');
    expect(value).toContain('<public_address>');
  });

  test('Ctrl+/ twice on lines holding a comment gives them back, valid in between', async ({ page }) => {
    const text = '<a>x</a>\n<!-- c -->\n<b/>';
    const e = await at(page, `¦${text}¦`);
    await page.keyboard.press('Control+/');
    const once = await e.value();
    expect(once.match(/<!--/g)).toHaveLength(1);
    // One comment, and no `--` inside it (XML forbids it).
    expect(once.startsWith('<!-- ') && once.endsWith(' -->')).toBe(true);
    expect(once.slice(5, -4)).not.toContain('--');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe(text);
  });
});

test.describe('hcl', () => {
  setupEditor(test, '?language=hcl');

  test('a bare block label gets no suggestions', async ({ page }) => {
    const e = await at(page, 'services ¦');
    await page.keyboard.type('we');
    await page.waitForTimeout(200);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('undo in Edit-as', () => {
  setupEditor(test, '?language=toml');

  test('undoing a deleted block restores the host with its comment', async ({ page }) => {
    const e = editor(page);
    const host = 'listen = 1\n\n# beta client\n[clients.beta]\nsecret = "y"\nmax_connections = 3\n';
    await e.paste(host);
    await page.locator('.demo__edit-select').selectOption('yaml');
    await expect(e.input).toHaveValue(/beta:/);
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('clients:');
      element.focus();
      element.setSelectionRange(start, element.value.length);
    });
    await page.keyboard.press('Delete');
    await expect.poll(() => saved(page)).not.toContain('beta');
    await page.keyboard.press('Control+z');
    await expect.poll(() => saved(page)).toBe(host);
  });
});

test.describe('toml', () => {
  setupEditor(test, '?language=toml');

  test('Enter after Ctrl+Space on the value already written only closes the list', async ({ page }) => {
    const e = await at(page, '[services.s]\nmode = "ba¦lance"\n');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(['balance']);
    await page.keyboard.press('Enter');
    expect(await e.value()).toBe('[services.s]\nmode = "balance"\n');
    expect(await e.isOpen()).toBe(false);
  });
});
