import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// A phone: touch input (`hasTouch`, `isMobile`) on a phone viewport.
// Playwright taps with `page.touchscreen.tap`; drags go through the
// DevTools protocol's touch events. Headless Chromium has no long-press
// selection (a long press is a tap there), so the selection a phone's long
// press and its handles make, without a click or a key, is made with
// `setSelectionRange`: the editor follows the selection itself.

test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 740 } });

const activeLine = (page) =>
  page.locator('.config-editor__gutter-content .config-editor__line-number-active').getAttribute('data-line');

/** A finger dragged from `from` to `to` (DevTools touch events). */
async function touchDrag(page, from, to, steps = 12) {
  const client = await page.context().newCDPSession(page);
  const point = (t) => ({ x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t });
  await client.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [point(0)] });
  for (let step = 1; step <= steps; step += 1) {
    await client.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [point(step / steps)] });
  }
  await client.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await client.detach();
}

async function tap(page, line, column) {
  const point = await pointAt(page, line, column);
  await page.touchscreen.tap(point.x, point.y);
}

test.describe('touch on a phone', () => {
  setupEditor(test, '?language=yaml');

  test('a tap places the caret where it lands', async ({ page }) => {
    const e = editor(page);
    // `listen: 7835` is line 2: tap between `li` and `sten`, on the
    // boundary itself (`pointAt` aims at a character's middle, where a tap
    // is as close to the boundary before it as to the one after it, so the
    // caret could land on either side).
    await tap(page, 1, 1.5);
    await expect(e.input).toBeFocused();
    expect(await e.caret()).toEqual({ line: 2, column: 3, collapsed: true });
    await expect.poll(() => activeLine(page)).toBe('2');
    await tap(page, 3, 0);
    expect((await e.caret()).line).toBe(4);
    await expect.poll(() => activeLine(page)).toBe('4');
  });

  test('a selection made by the phone (long press, handles) is followed without a click or key', async ({
    page,
  }) => {
    const e = editor(page);
    await tap(page, 0, 0);
    // What a long press on `tunnel` and dragging its end handle leave.
    await e.input.evaluate((element) => {
      const start = element.value.indexOf('tunnel');
      element.setSelectionRange(start, start + 'tunnel.example'.length);
    });
    await expect.poll(() => activeLine(page)).toBe('3');
    // The selection reads back the way the phone left it.
    const selected = await e.input.evaluate((element) =>
      element.value.slice(element.selectionStart, element.selectionEnd),
    );
    expect(selected).toBe('tunnel.example');
    // Typing replaces it, as on a desktop.
    await page.keyboard.type('edge.example');
    expect(await e.value()).toContain('public_address: edge.example.com');
  });

  test('a touch drag scrolls the text and the gutter together', async ({ page, baseURL }) => {
    await page.goto(`${baseURL}?language=yaml&sample=large`);
    const e = editor(page);
    await e.input.waitFor();
    const box = await e.input.boundingBox();
    const x = box.x + box.width / 2;
    await touchDrag(page, { x, y: box.y + box.height * 0.8 }, { x, y: box.y + box.height * 0.2 });
    await expect.poll(() => e.input.evaluate((element) => element.scrollTop)).toBeGreaterThan(50);
    const offsets = await page.evaluate(() => {
      const input = document.querySelector('.config-editor__input');
      const gutter = document.querySelector('.config-editor__gutter-content');
      const first = gutter.querySelector('div[data-line]');
      const firstShown = [...gutter.querySelectorAll('div[data-line]')].find(
        (row) => row.getBoundingClientRect().bottom > input.getBoundingClientRect().top + 2,
      );
      return { scrollTop: input.scrollTop, line: Number(firstShown?.dataset.line ?? first.dataset.line) };
    });
    const lineHeight = await e.input.evaluate((element) => parseFloat(getComputedStyle(element).lineHeight));
    // The first line number in view is the first text line in view.
    const expected = Math.floor(offsets.scrollTop / lineHeight) + 1;
    expect(Math.abs(offsets.line - expected)).toBeLessThanOrEqual(1);
  });

  test('a suggestion is accepted with a tap', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('');
    await page.keyboard.type('pub');
    await expect.poll(() => e.isOpen()).toBe(true);
    const option = e.options.first();
    const box = await option.boundingBox();
    expect(box.height).toBeGreaterThanOrEqual(20);
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await expect.poll(() => e.value()).toContain('public_address');
    await expect(e.input).toBeFocused();
  });

  test('a tap on the footer goes to the problem; a tap on the underline shows its message', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: true\npublic_address: tunnel.example.com\n');
    await expect.poll(() => e.diagnosticCount()).toBe(1);
    const status = page.locator('.config-editor__diagnostic-status');
    await expect(status).toContainText('1 error');
    const box = await status.boundingBox();
    expect(box.x + box.width).toBeLessThanOrEqual(390);
    await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await expect(e.input).toBeFocused();
    expect((await e.caret()).line).toBe(1);

    // A tap on the underlined value: the message shows (no hover on a phone),
    // in the docs card of its documented key, the problem tip yielding.
    await tap(page, 3, 0);
    await tap(page, 0, 9);
    const marker = page.locator('.config-editor__diagnostic-marker');
    await expect(marker).toHaveClass(/config-editor__diagnostic-marker-shown/);
    const card = page.locator('.config-editor__hover');
    await expect(card).toBeVisible();
    await expect(card.locator('.config-editor__problem-tip-entry')).toContainText('Error');
    await expect(page.locator('.config-editor__problem-tip')).toBeHidden();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  });
});

test.describe('touch on protected text', () => {
  setupEditor(test, '?language=yaml&schema=protected');

  test('a tap on a read-only value says why it is read-only', async ({ page }) => {
    const e = editor(page);
    const lines = (await e.value()).split('\n');
    const line = lines.findIndex((candidate) => candidate.includes('srv-42'));
    await tap(page, line, lines[line].indexOf('srv-42') + 2);
    await expect(page.locator('.config-editor__protect-tip')).toContainText('Assigned by the server');
  });
});
