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

  test('keyboard: Enter accepts the key, then an arrow and Enter accept a value', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen: ');
    expect(await e.isOpen()).toBe(true);
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('listen: 7835');
    expect(await e.isOpen()).toBe(false);
  });

  test('a bare colon gets a separating space before the example value', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('web:');
    await page.keyboard.press('Enter');
    await page.keyboard.type('http:');
    await expect.poll(() => e.isOpen()).toBe(true);
    await e.clickKey('0.0.0.0:7835');
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
    await page.keyboard.type('paradoxe:');
    await page.keyboard.press('Enter');
    await expect(e.input).toHaveValue('clients:\n  paradoxe:\n    ');
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

  test('typing again after Escape reopens suggestions', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('lis');
    await page.keyboard.press('Escape');
    expect(await e.isOpen()).toBe(false);
    await page.keyboard.type('t');
    await expect.poll(() => e.isOpen()).toBe(true);
  });
});

test.describe('highlight', () => {
  test('hover and arrow keys share exactly one highlighted row', async ({ page }) => {
    const e = editor(page);
    await e.paste('clients:\n  paradoxe:\n    ');
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
