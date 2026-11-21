import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Lines and selections: cut and copy whole lines, Ctrl+Delete, backward selections, Escape, line numbers,
// line commands, the caret kept in view, text arriving whole in one command.

// Pastes `marked` and puts the caret at `|`.
async function at(page, marked) {
  const e = editor(page);
  const position = marked.indexOf('|');
  await e.paste(marked.replace('|', ''));
  await e.input.evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, position);
  return e;
}

const selection = (e) =>
  e.input.evaluate((element) => ({
    start: element.selectionStart,
    end: element.selectionEnd,
    direction: element.selectionDirection,
  }));

async function select(page, text, from, to) {
  const e = editor(page);
  await e.paste(text);
  await e.input.evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [from, to],
  );
  return e;
}

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const schema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port.' }, host: { type: 'string' } },
});

const value = 'port: "eighty"\n' + Array.from({ length: 30 }, (_, index) => `k${index}: ${index}`).join('\n') + '\n';

test.describe('yaml: editing conventions', () => {
  setupEditor(test, '?language=yaml');
  test('a line cut with nothing selected pastes above the caret line', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nb: |2\nc: 3');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1\nc: 3');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('Control+v');
    expect(await e.value()).toBe('a: 1\nb: 2\nc: 3');
  });
  test('cutting the last line leaves no empty line', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nb: |2');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1');
  });
});

test.describe('yaml: editing', () => {
  setupEditor(test, '?language=yaml');
  test('a whole-line cut keeps the caret column', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const e = await at(page, 'a: 1\nbb|b: 2\ncccc: 3\n');
    await page.keyboard.press('Control+x');
    expect(await e.value()).toBe('a: 1\ncccc: 3\n');
    expect(await e.caret()).toMatchObject({ line: 2, column: 3 });
  });
  test('Home on a long scrolled line brings the caret back into view', async ({ page }) => {
    const e = await at(page, `key: ${'x'.repeat(400)}|`);
    await page.keyboard.press('End');
    await page.keyboard.press('Home');
    const scrollLeft = await e.input.evaluate((element) => element.scrollLeft);
    expect(scrollLeft).toBeLessThan(50);
  });
});

test.describe('yaml: whole lines and selections', () => {
  setupEditor(test, '?language=yaml');
  test('Escape collapses a selection', async ({ page }) => {
    const e = await at(page, 'listen: 7835|');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Escape');
    expect((await e.caret()).collapsed).toBe(true);
  });
  test('a line number selects its line', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\n');
    await page.locator('.config-editor__gutter-content > div[data-line="2"]').click();
    const selected = await e.input.evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
    expect(selected).toBe('b: 2\n');
  });

  /** The middle of line `line`'s number. */
  const numberAt = async (page, line) => {
    const box = await page.locator(`.config-editor__gutter-content > div[data-line="${line}"] > span`).boundingBox();
    return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  };
  const selected = (e) =>
    e.input.evaluate((element) => ({
      text: element.value.slice(element.selectionStart, element.selectionEnd),
      direction: element.selectionDirection,
    }));

  test('a drag over the line numbers selects whole lines, down and up', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n');
    for (const [from, to, direction] of [
      [2, 4, 'forward'],
      [4, 2, 'backward'],
    ]) {
      const start = await numberAt(page, from);
      const end = await numberAt(page, to);
      await page.mouse.move(start.x, start.y);
      await page.mouse.down();
      await page.mouse.move(end.x, end.y, { steps: 6 });
      await page.mouse.up();
      expect(await selected(e)).toEqual({ text: 'b: 2\nc: 3\nd: 4\n', direction });
    }
    // Past the gutter's side, the drag goes on by the rows.
    const start = await numberAt(page, 1);
    const end = await numberAt(page, 3);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await page.mouse.move(end.x + 300, end.y, { steps: 6 });
    await page.mouse.up();
    expect((await selected(e)).text).toBe('a: 1\nb: 2\nc: 3\n');
  });

  test('a drag over the line numbers past the text scrolls it, down and up', async ({ page }) => {
    const lines = Array.from({ length: 200 }, (_, index) => `k${index + 1}: ${index + 1}\n`);
    const e = await at(page, `|${lines.join('')}`);
    const area = await e.input.boundingBox();
    const start = await numberAt(page, 2);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    // Held still below the text: the text scrolls on, the selection with it.
    await page.mouse.move(start.x, area.y + area.height + 40, { steps: 6 });
    await expect.poll(() => e.input.evaluate((element) => element.scrollTop)).toBeGreaterThan(200);
    await page.mouse.up();
    const down = await selected(e);
    expect(down.direction).toBe('forward');
    expect(down.text.startsWith('k2: 2\n')).toBe(true);
    // The last line selected is the one in view at the bottom.
    const shownLast = await page.evaluate(() => {
      const input = document.querySelector('.config-editor__input');
      const bottom = input.getBoundingClientRect().bottom;
      const numbers = [...document.querySelectorAll('.config-editor__gutter-content > div[data-line]')];
      return Math.max(...numbers.filter((row) => row.getBoundingClientRect().top < bottom - 4).map((row) => Number(row.dataset.line)));
    });
    expect(down.text.split('\n').length - 1).toBeGreaterThan(20);
    expect(Math.abs(down.text.split('\n').length - 1 - (shownLast - 1))).toBeLessThanOrEqual(1);
    // And up again, above the text, from a line far down.
    const scrolled = await e.input.evaluate((element) => element.scrollTop);
    const from = await page.evaluate(() => {
      const input = document.querySelector('.config-editor__input');
      const bottom = input.getBoundingClientRect().bottom;
      const numbers = [...document.querySelectorAll('.config-editor__gutter-content > div[data-line]')];
      return numbers.filter((row) => row.getBoundingClientRect().bottom < bottom - 4).pop().dataset.line;
    });
    const low = await numberAt(page, Number(from));
    await page.mouse.move(low.x, low.y);
    await page.mouse.down();
    await page.mouse.move(low.x, Math.max(1, area.y - 40), { steps: 6 });
    await expect.poll(() => e.input.evaluate((element) => element.scrollTop)).toBeLessThan(scrolled - 200);
    await page.mouse.up();
    const up = await selected(e);
    expect(up.direction).toBe('backward');
    expect(up.text.endsWith(lines[Number(from) - 1])).toBe(true);
  });

  test('Shift+click on a line number extends the selection by whole lines', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n');
    const click = async (line, shift) => {
      const point = await numberAt(page, line);
      if (shift) await page.keyboard.down('Shift');
      await page.mouse.click(point.x, point.y);
      if (shift) await page.keyboard.up('Shift');
    };
    await click(2);
    await click(4, true);
    expect(await selected(e)).toEqual({ text: 'b: 2\nc: 3\nd: 4\n', direction: 'forward' });
    // From the same line picked first, the other way.
    await click(1, true);
    expect(await selected(e)).toEqual({ text: 'a: 1\nb: 2\n', direction: 'backward' });
    // From a caret in the text: to the whole line clicked.
    await at(page, 'a: 1\nb: 2\nc: |3\nd: 4\ne: 5\n');
    await click(4, true);
    expect(await selected(e)).toEqual({ text: '3\nd: 4\n', direction: 'forward' });
  });
});

test.describe('yaml: commands keep a backward selection backward', () => {
  setupEditor(test, '?language=yaml');

  test('Tab indents and Shift+Up keeps extending from the start', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2\nc: 3|\n');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Tab');
    expect(await e.value()).toBe('  a: 1\n  b: 2\n  c: 3\n');
    expect((await selection(e)).direction).toBe('backward');
  });

  test('Ctrl+/ comments and keeps the direction', async ({ page }) => {
    const e = await at(page, 'a: 1\nb: 2|\n');
    await page.keyboard.press('Shift+ArrowUp');
    await page.keyboard.press('Control+/');
    expect(await e.value()).toBe('# a: 1\n# b: 2\n');
    expect((await selection(e)).direction).toBe('backward');
  });
});

test.describe('read-only: Escape collapses a selection', () => {
  setupEditor(test, '?language=yaml&readonly');

  test('after selecting a line by its number', async ({ page }) => {
    const e = editor(page);
    await page.locator('.config-editor__gutter-content > div[data-line="2"]').click();
    expect((await e.caret()).collapsed).toBe(false);
    await page.keyboard.press('Escape');
    expect((await e.caret()).collapsed).toBe(true);
  });
});

test.describe('keys the editor handles keep the caret in view', () => {
  setupEditor(test, '?language=yaml');

  test('a line number click does not move the view', async ({ page }) => {
    const e = editor(page);
    const lines = Array.from({ length: 400 }, (_, index) => `key${index}: ${index}`);
    await e.paste(`${lines.join('\n')}\n`);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, 0);
      element.blur();
      element.scrollTop = 4000;
    });
    await page.waitForTimeout(100);
    // A row in the middle of the view (the rendered rows reach past it both ways).
    const line = await page.evaluate(() => {
      const view = document.querySelector('.config-editor__input').getBoundingClientRect();
      const middle = (view.top + view.bottom) / 2;
      const rows = [...document.querySelectorAll('.config-editor__gutter-content > div[data-line]')];
      return Number(rows.find((row) => row.getBoundingClientRect().bottom > middle).dataset.line);
    });
    const row = page.locator(`.config-editor__gutter-content > div[data-line="${line}"]`);
    const before = await e.input.evaluate((element) => element.scrollTop);
    await row.click();
    const after = await e.input.evaluate((element) => element.scrollTop);
    expect(Math.abs(after - before)).toBeLessThan(2);
    const selected = await e.input.evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
    expect(selected).toBe(`key${line - 1}: ${line - 1}\n`);
  });
});

test.describe('every key the editor handles reveals the caret', () => {
  setupEditor(test, '?language=yaml');

  const lines = ['root:', ...Array.from({ length: 700 }, (_, index) => `  key${index}: ${index}`)];
  const caretInView = (e) =>
    e.input.evaluate((element) => {
      const line = element.value.slice(0, element.selectionStart).split('\n').length;
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
      const top = (line - 1) * lineHeight;
      return top >= element.scrollTop - lineHeight && top <= element.scrollTop + element.clientHeight;
    });

  for (const [name, offsetInLine, key] of [
    ['Ctrl+/', 4, 'Control+/'],
    ['Shift+Tab', 4, 'Shift+Tab'],
    ['smart Home', 12, 'Home'],
    ['Backspace in indentation', 2, 'Backspace'],
    ['an opening bracket', 13, '['],
  ]) {
    test(name, async ({ page }) => {
      const e = editor(page);
      await e.paste(`${lines.join('\n')}\n`);
      await e.input.evaluate(
        (element, offset) => {
          const at = element.value.indexOf('  key600: ') + offset;
          element.focus();
          element.setSelectionRange(at, at);
          element.scrollTop = 0;
        },
        offsetInLine,
      );
      await page.keyboard.press(key);
      expect(await caretInView(e)).toBe(true);
    });
  }
});

test.describe('a long backward selection keeps its caret in view', () => {
  setupEditor(test, '?language=yaml');

  const lines = ['root:', ...Array.from({ length: 200 }, (_, index) => `  key${index}: ${index}`)];
  const caretInView = (e) =>
    e.input.evaluate((element) => {
      const caret = element.selectionDirection === 'backward' ? element.selectionStart : element.selectionEnd;
      const line = element.value.slice(0, caret).split('\n').length;
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
      const top = (line - 1) * lineHeight;
      return top >= element.scrollTop - lineHeight && top <= element.scrollTop + element.clientHeight;
    });

  for (const [name, key] of [
    ['Tab', 'Tab'],
    ['Shift+Tab', 'Shift+Tab'],
    ['Ctrl+/', 'Control+/'],
  ]) {
    test(name, async ({ page }) => {
      const e = editor(page);
      await e.paste(`${lines.join('\n')}\n`);
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('  key80: ');
        element.focus();
        element.setSelectionRange(at, at);
      });
      for (let index = 0; index < 50; index += 1) await page.keyboard.press('Shift+ArrowUp');
      expect(await caretInView(e)).toBe(true);
      await page.keyboard.press(key);
      expect(await caretInView(e)).toBe(true);
    });
  }
});

test.describe('Ctrl+Delete at the end of a line joins it with the next', () => {
  setupEditor(test, '?language=yaml');

  test('without deleting the next line’s first word', async ({ page }) => {
    const text = 'a: 1\n  bb: 2';
    const e = await select(page, text, 4, 4);
    await page.keyboard.press('Control+Delete');
    expect(await e.value()).toBe('a: 1bb: 2');
  });
});

// Text of several lines arriving whole in one command
// (an IME commit, `Input.insertText`) is applied by the editor as a paste,
// not line by line by the textarea: over the selection, line breaks as LF,
// the caret after it, one undo step; read-only refuses it.
test.describe('lines arriving whole in one command', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('replace the selection as a paste does, in one undo step, and stay out of read-only', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(vanilla({ value: 'a: 1\nb: 2\n', schema: '{}' }));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const input = page.locator('.config-editor__input');
    await input.click();
    // `1` of line 1 selected.
    await input.evaluate((element) => element.setSelectionRange(3, 4));
    await page.keyboard.insertText('9\r\nc: 3\nd: 4');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4\nb: 2\n');
    await page.keyboard.type('x');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4x\nb: 2\n');
    await page.keyboard.press('Control+z');
    await expect(input).toHaveValue('a: 9\nc: 3\nd: 4\nb: 2\n');
    await page.keyboard.press('Control+z');
    await expect(input).toHaveValue('a: 1\nb: 2\n');
    await page.locator('#read-only').check();
    await input.focus();
    await page.keyboard.insertText('e: 5\nf: 6\n');
    await expect(input).toHaveValue('a: 1\nb: 2\n');
    expect(errors).toEqual([]);
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const pageUrl = (options, name = 'vanilla') =>
    `${pathToFileURL(join(js, 'examples', `${name}.html`)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  async function open(page, options, name) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(pageUrl(options, name));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  async function caretOnLine2(page) {
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
  }

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('line commands with a selection', () => {
    test('the line commands edit the lines, each one undo step', async ({ page }) => {
      const errors = await open(page, { value: 'a: 1\nb: 2\nc: 3\n' });
      for (const [key, expected] of [
        ['Alt+ArrowUp', 'b: 2\na: 1\nc: 3\n'],
        ['Alt+ArrowDown', 'a: 1\nc: 3\nb: 2\n'],
        ['Shift+Alt+ArrowDown', 'a: 1\nb: 2\nb: 2\nc: 3\n'],
        ['Shift+Alt+ArrowUp', 'a: 1\nb: 2\nb: 2\nc: 3\n'],
        ['Control+Shift+K', 'a: 1\nc: 3\n'],
        ['Control+BracketRight', 'a: 1\n  b: 2\nc: 3\n'],
      ]) {
        await caretOnLine2(page);
        await page.keyboard.press(key);
        await expect(input(page), key).toHaveValue(expected);
        await page.keyboard.press('Control+z');
        await expect(input(page), `${key} undone`).toHaveValue('a: 1\nb: 2\nc: 3\n');
      }
      expect(errors).toEqual([]);
    });
  });
});
