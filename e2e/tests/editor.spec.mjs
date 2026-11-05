import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

setupEditor(test);

test.describe('accepting keys and values', () => {
  test('listen: accepting the key opens its value list, and a value closes it', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    expect(await e.keys()).toContain('listen');
    await e.clickKey('listen');
    await expect(e.input).toHaveValue('listen: ');
    expect(await e.isOpen()).toBe(true);
    expect(await e.keys()).toEqual(expect.arrayContaining(['0.0.0.0:7835', '7835']));
    await e.clickKey('0.0.0.0:7835');
    await expect(e.input).toHaveValue('listen: "0.0.0.0:7835"');
    expect(await e.isOpen()).toBe(false);
  });

  test('completing a key before its existing colon does not panic or duplicate the key', async ({ page }) => {
    const e = editor(page);
    await e.paste('insecure_plaintext: false');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(3, 3);
    });
    await page.keyboard.type('e');
    await expect.poll(() => e.keys()).toContain('insecure_plaintext');
    await e.clickKey('insecure_plaintext');
    await expect(e.input).toHaveValue('insecure_plaintext: false');
  });

  test('keyboard: arrow then Enter accepts the key, then an arrow and Enter accept a value', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    // Narrow to a single match, then arrow to engage the list explicitly:
    // Enter on an untouched auto-triggered list inserts a newline instead.
    await page.keyboard.type('t');
    await expect.poll(() => e.keys()).toEqual(['listen']);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen: ');
    expect(await e.isOpen()).toBe(true);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen: 7835');
    expect(await e.isOpen()).toBe(false);
  });

  test('Enter with an auto-triggered list inserts a newline instead of accepting', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('lis\n');
    expect(await e.isOpen()).toBe(false);
  });

  test('Tab with an auto-triggered list indents instead of accepting', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.type('l');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Tab');
    await expect(e.input).toHaveValue('listen: 7835\nl  ');
    expect(await e.isOpen()).toBe(false);
  });

  test('a bare colon gets a separating space before the example value', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('http:');
    await expect.poll(() => e.isOpen()).toBe(true);
    // Explicit request: Enter accepts the top match without arrowing.
    await page.keyboard.press('Control+Space');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('web:\n  http: "0.0.0.0:7835"');
    expect(await e.isOpen()).toBe(false);
  });

  test('Escape closes a value list opened by typing "http: "', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('http: ');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Escape');
    expect(await e.isOpen()).toBe(false);
  });

  test('Enter after a bare object key starts an indented line without accepting "null"', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('web:\n  ');
    expect(await e.isOpen()).toBe(false);
  });

  test('a new indented line does not open suggestions, Ctrl+Space does', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('clients:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('alex:');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('clients:\n  alex:\n    ');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.keys()).toEqual(expect.arrayContaining(['max_connections', 'secret']));
  });
});

test.describe('focus', () => {
  test('clicking outside the editor closes an open suggestion list', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.mouse.click(1000, 600);
    await expect.poll(() => e.isOpen(), 'list closes on blur').toBe(false);
  });

  test('Escape dismisses an open diff preview', async ({ page }) => {
    const e = editor(page);
    await e.paste(
      '# Burrow server configuration\nlisten: 7836\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n',
    );
    const marker = page.locator('.yaml-editor__diff-marker-modified').first();
    await expect(marker).toHaveCount(1);
    await marker.click();
    const preview = page.locator('.yaml-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(preview).toHaveCount(0);
  });

  test('a change preview shows before and after and closes from its close button', async ({ page }) => {
    const e = editor(page);
    await e.paste(
      '# Burrow server configuration\nlisten: 7836\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n',
    );
    await page.locator('.yaml-editor__diff-marker-modified').first().click();
    const preview = page.locator('.yaml-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    await expect(preview).toContainText('7835');
    await expect(preview).toContainText('7836');
    await preview.getByRole('button', { name: 'Close change preview' }).click();
    await expect(preview).toHaveCount(0);
  });

  test('a change preview survives typing elsewhere but not on its own line', async ({ page }) => {
    const e = editor(page);
    await e.paste(
      '# Burrow server configuration\nlisten: 7836\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n',
    );
    await page.locator('.yaml-editor__diff-marker-modified').first().click();
    const preview = page.locator('.yaml-editor__diff-peek');
    await expect(preview).toHaveCount(1);
    // Typing on an unrelated line keeps the preview open...
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(element.value.length, element.value.length);
    });
    await page.keyboard.press('Enter');
    await page.keyboard.type('added_key: 1');
    await expect(preview).toHaveCount(1);
    // ...but editing the previewed line dismisses it.
    const at = await pointAt(page, 1, 13);
    await page.mouse.click(at.x, at.y);
    await page.keyboard.type('0');
    await expect(preview).toHaveCount(0);
  });

  test('typing again after Escape reopens suggestions', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await page.keyboard.press('Escape');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.type('t');
    await expect.poll(() => e.isOpen()).toBe(true);
  });
});

test.describe('read-only view mode', () => {
  test('typing and shortcuts cannot modify the document, but hover docs still work', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}?readonly=1`);
    await page.locator('.yaml-editor__input').waitFor();
    const e = editor(page);
    await expect(e.input).toHaveAttribute('readonly', '');
    await expect(e.input).toHaveAttribute('aria-readonly', 'true');
    const before = await e.value();

    await e.input.click();
    await page.keyboard.type('garbage-text');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Delete');
    expect(await e.value()).toBe(before);

    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);

    const onKey = await pointAt(page, 1, 1);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    const card = page.locator('.yaml-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.yaml-editor__hover-key')).toHaveText('listen');
  });
});

test.describe('highlight', () => {
  test('hover and arrow keys share exactly one highlighted row', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients:\n  alex:\n    ');
    await page.keyboard.press('Control+Space');
    await expect(e.options).toHaveCount(2);
    const second = (await e.keys())[1];
    await e.options.nth(1).hover();
    await expect.poll(() => e.highlighted()).toHaveLength(1);
    expect(await e.highlighted()).toEqual([second]);
    await page.keyboard.press('ArrowDown');
    expect(await e.highlighted()).toHaveLength(1);
  });
});

test.describe('hover documentation', () => {
  test('hovering a key shows its schema documentation, and leaving the key hides it', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('http: 80');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Escape');
    const card = page.locator('.yaml-editor__hover');

    const onKey = await pointAt(page, 1, 3);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    await expect(card).toHaveCount(1);
    await expect(card.locator('.yaml-editor__hover-key')).toHaveText('http');
    await expect(card).toContainText('0.0.0.0:7835');
    await expect(card).toContainText('Port or ip:port for plain HTTP');

    const offKey = await pointAt(page, 1, 9);
    await page.mouse.move(offKey.x, offKey.y, { steps: 3 });
    await expect(card).toHaveCount(0);
  });

  test('the hover card is suppressed while suggestions are open', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('http: ');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    const onKey = await pointAt(page, 1, 3);
    await page.mouse.move(onKey.x, onKey.y, { steps: 4 });
    await page.waitForTimeout(500);
    await expect(page.locator('.yaml-editor__hover')).toHaveCount(0);
  });
});
