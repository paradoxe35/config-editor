import { expect, test } from '@playwright/test';
import { editor, pointAt, setupEditor } from './helpers.mjs';

// Folding on the ~2000-line YAML sample, with the real mouse and keys. The
// textarea holds the text on screen (folded lines cut out); the demo's
// "Saved value" is the whole document `on_change` sent.
//
// Sample lines used (one-based): 6 `  remote: true`, 7 `websocket:` (folds
// 8–12), 9 `  path: /tunnel`, 11 `  keepalive_secs: 20`, 12
// `  challenge: http`, 13 `heartbeat:`.

const doc = (page) => page.locator('.demo__output-text').textContent();
const row = (page, line) => page.locator(`.config-editor__gutter-content > div[data-line="${line}"]`);
const toggle = (page, line) => page.locator(`.config-editor__fold-toggle[data-line="${line}"]`);
const chip = (page, line) => page.locator(`.config-editor__fold-chip[data-line="${line}"]`);
const shownLines = (page) =>
  page
    .locator('.config-editor__gutter-content > div[data-line]')
    .evaluateAll((rows) => rows.map((element) => Number(element.dataset.line)));
const isClosed = async (page, line) =>
  (await toggle(page, line).count()) > 0 &&
  (await toggle(page, line).getAttribute('class')).includes('config-editor__fold-toggle-closed');

/** Clicks the textarea at a row on screen (0-based) and column. */
async function clickRow(page, screenRow, column) {
  const point = await pointAt(page, screenRow, column);
  await page.mouse.click(point.x, point.y);
}

/** Folds `line` (one-based) with its gutter chevron. */
async function foldWithChevron(page, line) {
  await row(page, line).hover();
  await toggle(page, line).click();
  await expect.poll(() => isClosed(page, line)).toBe(true);
}

test.describe('folding', () => {
  setupEditor(test, '?language=yaml&sample=large');

  test('a chevron shown on gutter hover folds a block; the chip opens it', async ({ page }) => {
    const e = editor(page);
    const whole = await doc(page);
    await page.mouse.move(600, 10);
    await expect(toggle(page, 7)).toHaveCSS('opacity', '0');
    await row(page, 7).hover();
    await expect(toggle(page, 7)).toHaveCSS('opacity', '1');
    await toggle(page, 7).click();

    expect(await e.value()).toContain('websocket:\nheartbeat:');
    expect(await doc(page)).toBe(whole);
    const lines = await shownLines(page);
    expect(lines).toContain(7);
    expect(lines).toContain(13);
    expect(lines.filter((line) => line > 7 && line < 13)).toEqual([]);
    // Closed folds keep their chevron without hover.
    await page.mouse.move(600, 10);
    await expect(toggle(page, 7)).toHaveCSS('opacity', '1');
    await expect(toggle(page, 7)).toHaveAttribute('aria-expanded', 'false');

    await chip(page, 7).click();
    expect(await e.value()).toContain('websocket:\n  listen:');
    expect(await isClosed(page, 7)).toBe(false);
    expect(await shownLines(page)).toContain(9);
    expect(await doc(page)).toBe(whole);
  });

  test('keys fold and unfold at the caret, and everything at once', async ({ page }) => {
    const e = editor(page);
    const whole = await doc(page);
    await clickRow(page, 8, 4);
    await page.keyboard.press('Control+Shift+BracketLeft');
    expect(await isClosed(page, 7)).toBe(true);
    // The caret moved out of the fold, onto its line.
    expect((await e.caret()).line).toBe(7);
    await page.keyboard.press('Control+Shift+BracketRight');
    expect(await isClosed(page, 7)).toBe(false);

    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+0');
    const folded = (await e.value()).split('\n').length;
    expect(folded).toBeLessThan(200);
    expect(await doc(page)).toBe(whole);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+J');
    expect((await e.value()).split('\n').length).toBe(whole.split('\n').length);
  });

  test('a fold stays through edits elsewhere and moves with its lines; undo keeps it', async ({ page }) => {
    const e = editor(page);
    const whole = await doc(page);
    await foldWithChevron(page, 7);
    await clickRow(page, 2, 2);
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('x: 1');
    await expect.poll(() => doc(page)).toContain('public_address: tunnel.pngwasi.me\nx: 1\n');
    expect(await isClosed(page, 8)).toBe(true);
    expect(await e.value()).toContain('websocket:\nheartbeat:');
    expect(await doc(page)).toContain('  path: /tunnel');

    // Typing and the line break may be separate undo steps.
    for (let step = 0; step < 3 && (await doc(page)) !== whole; step += 1) {
      await page.keyboard.press('Control+Z');
      await page.waitForTimeout(50);
    }
    expect(await doc(page)).toBe(whole);
    expect(await isClosed(page, 7)).toBe(true);
    expect(await e.value()).toContain('websocket:\nheartbeat:');
  });

  test('typing at a folded line end, or Backspace after the fold, opens it first', async ({ page }) => {
    const e = editor(page);
    await foldWithChevron(page, 7);
    await clickRow(page, 6, 2);
    await page.keyboard.press('End');
    await page.keyboard.type('#');
    await expect.poll(() => doc(page)).toContain('websocket:#\n  listen:');
    expect(await isClosed(page, 7)).toBe(false);
    await page.keyboard.press('Backspace');

    await foldWithChevron(page, 7);
    // `heartbeat:` is the row after the fold.
    await clickRow(page, 7, 0);
    await page.keyboard.press('Home');
    await page.keyboard.press('Backspace');
    // As in VS Code, the fold opens and the line joins its last line.
    await expect.poll(() => doc(page)).toContain('  challenge: httpheartbeat:');
    expect(await isClosed(page, 7)).toBe(false);
    expect(await e.value()).toContain('  challenge: httpheartbeat:');
  });

  test('copy and cut across a fold take the hidden lines', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = editor(page);
    await foldWithChevron(page, 7);
    // From the start of line 6 to the start of `heartbeat:`, over the fold.
    await clickRow(page, 5, 0);
    await page.keyboard.press('Shift+ArrowDown');
    await page.keyboard.press('Shift+ArrowDown');
    await page.keyboard.press('Control+C');
    const copied = await page.evaluate(() => navigator.clipboard.readText());
    expect(copied).toBe(
      '  remote: true\nwebsocket:\n  listen: "127.0.0.1:7836"\n  path: /tunnel\n' +
        '  host: tunnel.pngwasi.me\n  keepalive_secs: 20\n  challenge: http\n',
    );
    await page.keyboard.press('Control+X');
    await expect.poll(() => doc(page)).toContain('update:\nheartbeat:');
    expect(await doc(page)).not.toContain('/tunnel\n');
    await page.keyboard.press('Control+Z');
    await expect.poll(() => doc(page)).toContain('  path: /tunnel');
    expect(await e.value()).toContain('  remote: true\nwebsocket:');
  });

  test('a find match inside a fold opens it', async ({ page }) => {
    const e = editor(page);
    await foldWithChevron(page, 7);
    await e.input.click();
    await page.keyboard.press('Control+F');
    await page.keyboard.type('challenge: http');
    await page.keyboard.press('Enter');
    await expect.poll(() => isClosed(page, 7)).toBe(false);
    const selected = await e.input.evaluate((element) =>
      element.value.slice(element.selectionStart, element.selectionEnd),
    );
    expect(selected).toBe('challenge: http');
  });

  test('go to a line inside a fold opens it', async ({ page }) => {
    const e = editor(page);
    await foldWithChevron(page, 7);
    await e.input.click();
    await page.keyboard.press('Control+G');
    await page.keyboard.type('11');
    await page.keyboard.press('Enter');
    await expect.poll(() => isClosed(page, 7)).toBe(false);
    expect((await e.caret()).line).toBe(11);
  });

  test('a folded block shows the problems and changes it hides', async ({ page }) => {
    await clickRow(page, 10, 4);
    await page.keyboard.press('End');
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('soon');
    await expect
      .poll(() => row(page, 11).getAttribute('class'), { timeout: 5000 })
      .toContain('config-editor__line-number-error');
    await page.keyboard.press('Control+Shift+BracketLeft');
    expect(await isClosed(page, 7)).toBe(true);
    await expect(row(page, 7)).toHaveClass(/config-editor__line-number-error/);
    await expect(page.locator('.config-editor__fold-changes[data-line="7"]')).toHaveCount(1);
    // No underline is drawn for the hidden line.
    const markers = await page
      .locator('.config-editor__diagnostic-marker')
      .evaluateAll((all) => all.map((marker) => marker.getAttribute('aria-label')));
    expect(markers.filter((label) => label.includes('line 11'))).toEqual([]);
  });

  test('rows stay right with folds: fold all, scroll to the end, line numbers', async ({ page }) => {
    const e = editor(page);
    const total = (await doc(page)).split('\n').length;
    await e.input.click();
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+0');
    await page.keyboard.press('Control+End');
    await page.waitForTimeout(150);
    const lines = await shownLines(page);
    expect(Math.max(...lines)).toBe(total);
    // Rows are in order, and a folded line's next row is past its fold.
    expect([...lines].sort((a, b) => a - b)).toEqual(lines);
    const height = await e.input.evaluate((element) => {
      const style = getComputedStyle(element);
      return {
        rows: element.value.split('\n').length,
        scroll: element.scrollHeight,
        line: parseFloat(style.lineHeight),
      };
    });
    expect(height.scroll).toBeLessThan((height.rows + 3) * height.line + 60);

    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+J');
    await page.keyboard.press('Control+End');
    await page.waitForTimeout(150);
    expect(Math.max(...(await shownLines(page)))).toBe(total);
  });

  test('a 10k-line document folds and types quickly', async ({ page }) => {
    const e = editor(page);
    const big = (await doc(page)).repeat(5);
    await e.paste(big);
    const total = big.split('\n').length;
    expect(total).toBeGreaterThan(9000);
    await e.input.click();
    await page.evaluate(() => {
      window.__latencies = [];
      new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) window.__latencies.push(entry.duration);
      }).observe({ type: 'event', durationThreshold: 16, buffered: false });
    });
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+0');
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# end', { delay: 20 });
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 50))));
    const worst = await page.evaluate(() => Math.max(0, ...window.__latencies));
    expect(worst).toBeLessThan(400);
    await expect.poll(() => doc(page)).toContain('# end');
    expect(Math.max(...(await shownLines(page)))).toBe(total);
  });
});

test.describe('folding while editing as another language', () => {
  setupEditor(test, '?language=yaml&sample=large&edit=json');

  test('folds the text on screen and saves the whole document', async ({ page }) => {
    const e = editor(page);
    const whole = await doc(page);
    await e.input.click();
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+0');
    expect((await e.value()).split('\n').length).toBeLessThan(10);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+J');
    expect((await e.value()).split('\n').length).toBeGreaterThan(1500);
    expect(await doc(page)).toBe(whole);
  });
});

test.describe('folding off', () => {
  setupEditor(test, '?language=yaml&sample=large&folding=0');

  test('no chevrons, and the folding keys do nothing', async ({ page }) => {
    const e = editor(page);
    const lines = (await e.value()).split('\n').length;
    await row(page, 7).hover();
    await expect(toggle(page, 7)).toHaveCount(0);
    await e.input.click();
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+0');
    expect((await e.value()).split('\n').length).toBe(lines);
  });
});

test.describe('one-line documents', () => {
  setupEditor(test, '?language=yaml');

  test('have nothing to fold', async ({ page }) => {
    const e = editor(page);
    await e.typeDoc('a: 1');
    await row(page, 1).hover();
    await expect(toggle(page, 1)).toHaveCount(0);
  });
});
