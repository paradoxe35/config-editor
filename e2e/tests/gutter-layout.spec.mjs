import { test, expect } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// The gutter's markers (a read-only lock, a secret's key, the problem dot)
// sit in their own column: a gap from the gutter's edge, and clear of the
// line number however many digits it has.

/** The left edge of a line number's digits, from its text node. */
function digitsLeft(page, line) {
  return page.evaluate((line) => {
    const row = document.querySelector(`.config-editor__gutter div[data-line="${line}"] span`);
    const range = document.createRange();
    range.selectNodeContents(row);
    return range.getBoundingClientRect().left;
  }, line);
}

const gutterLeft = (page) => page.evaluate(() => document.querySelector('.config-editor__gutter').getBoundingClientRect().left);

/** Each marker's box beside its line's digits: a gap on both sides. */
async function expectClear(page, selector, minimumGap = 3) {
  const markers = page.locator(selector);
  expect(await markers.count()).toBeGreaterThan(0);
  const edge = await gutterLeft(page);
  for (const marker of await markers.all()) {
    const box = await marker.boundingBox();
    const line = await marker.evaluate((element) => element.closest('[data-line]').dataset.line);
    expect(box.x - edge, `${selector} on line ${line} keeps a gap from the edge`).toBeGreaterThanOrEqual(minimumGap);
    expect(await digitsLeft(page, line), `${selector} on line ${line} is clear of its number`).toBeGreaterThanOrEqual(box.x + box.width + 1);
  }
}

test.describe('the gutter markers, read-only and secrets', () => {
  setupEditor(test, '?schema=protected');

  test('locks and keys keep a gap from the edge and from the line number', async ({ page }) => {
    await expectClear(page, '.config-editor__lock-marker');
    await expectClear(page, '.config-editor__secret-marker');
  });
});

test.describe('the gutter on a document with four-digit line numbers', () => {
  setupEditor(test, '?sample=large');

  test('the problem dot stays clear of a four-digit line number', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('nonsense_key: 1');
    await page.keyboard.press('Escape');
    await expect(page.locator('.config-editor__line-number-error')).toHaveCount(1, { timeout: 5000 });
    const line = await page.locator('.config-editor__line-number-error').getAttribute('data-line');
    expect(Number(line)).toBeGreaterThanOrEqual(1000);
    const dot = await page.locator('.config-editor__line-number-error').evaluate((row) => {
      const style = getComputedStyle(row, '::before');
      const box = row.getBoundingClientRect();
      return { left: box.left + parseFloat(style.left), width: parseFloat(style.width) };
    });
    expect(dot.left - (await gutterLeft(page))).toBeGreaterThanOrEqual(3);
    expect(await digitsLeft(page, line)).toBeGreaterThanOrEqual(dot.left + dot.width + 3);
  });
});

/** The problem dot of the error row, in page coordinates. */
const errorDot = (page) =>
  page.locator('.config-editor__line-number-error').evaluate((row) => {
    const style = getComputedStyle(row, '::before');
    const box = row.getBoundingClientRect();
    return { left: box.left + parseFloat(style.left), width: parseFloat(style.width) };
  });

// A number that fills the digit column (five digits in a 10,000-line
// document) still has the dot a clear gap away, at every width.
for (const width of [1100, 390]) {
  test.describe(`the gutter on a document with five-digit line numbers, ${width}px wide`, () => {
    test.use({ viewport: { width, height: 700 } });

    test('the problem dot keeps its own column beside the number', async ({ page, baseURL }) => {
      const text = `${Array.from({ length: 10005 }, (_, i) => `# note ${i}`).join('\n')}\nnonsense_key: 1\n`;
      await page.addInitScript((sample) => {
        window.configEditorSample = sample;
      }, text);
      await page.goto(`${baseURL}?sample=page`);
      await page.locator('.config-editor__input').waitFor();
      await page.locator('.config-editor__input').click();
      await page.keyboard.press('Control+End');
      await expect(page.locator('.config-editor__line-number-error')).toHaveCount(1, { timeout: 10000 });
      const line = await page.locator('.config-editor__line-number-error').getAttribute('data-line');
      expect(Number(line)).toBe(10006);
      const dot = await errorDot(page);
      expect(dot.left - (await gutterLeft(page))).toBeGreaterThanOrEqual(3);
      expect(await digitsLeft(page, line)).toBeGreaterThanOrEqual(dot.left + dot.width + 3);
    });
  });
}

test.describe('the gutter markers at phone width', () => {
  test.use({ viewport: { width: 390, height: 700 } });
  setupEditor(test, '?schema=protected');

  test('locks and keys keep a gap on a phone too', async ({ page }) => {
    await expectClear(page, '.config-editor__lock-marker');
    await expectClear(page, '.config-editor__secret-marker');
  });
});

// The gutter is about as narrow as VS Code's: the marker column, the
// digits, a small gap, the fold chevrons' column, then the text. The change
// bars sit in the marker column before the number, so each bar and each
// chevron keeps a 24px hit area that reaches neither the number, the
// other's area, nor the text; and a marker appearing never moves the text.

/** The widest line number in view (its digits' box), the text's left edge and the gutter's box. */
const gutterGeometry = (page) =>
  page.evaluate(() => {
    const gutter = document.querySelector('.config-editor__gutter');
    let widest = null;
    for (const span of gutter.querySelectorAll('div[data-line] > span')) {
      const range = document.createRange();
      range.selectNodeContents(span);
      const box = range.getBoundingClientRect();
      if (box.width > 0 && (!widest || box.width > widest.width)) {
        widest = { left: box.left, right: box.right, width: box.width, digits: span.textContent.length };
      }
    }
    const input = document.querySelector('.config-editor__input');
    const textLeft = input.getBoundingClientRect().left + parseFloat(getComputedStyle(input).paddingLeft);
    const box = gutter.getBoundingClientRect();
    return { widest, textLeft, gutterLeft: box.left, gutterWidth: box.width };
  });

/** A hit area: the element's box joined with its positioned pseudo-elements'. */
const hitBox = (locator) =>
  locator.evaluate((element) => {
    const box = element.getBoundingClientRect();
    let [left, top, right, bottom] = [box.left, box.top, box.right, box.bottom];
    for (const pseudo of ['::before', '::after']) {
      const style = getComputedStyle(element, pseudo);
      if (style.content === 'none' || style.position !== 'absolute') continue;
      const x = box.left + parseFloat(style.left);
      const y = box.top + parseFloat(style.top);
      left = Math.min(left, x);
      top = Math.min(top, y);
      right = Math.max(right, x + parseFloat(style.width));
      bottom = Math.max(bottom, y + parseFloat(style.height));
    }
    return { left, top, right, bottom };
  });

for (const lines of [9, 99, 999, 9999]) {
  test.describe(`the gutter's width with ${lines} lines`, () => {
    test('the text starts close after the widest number, the number close to the edge', async ({ page, baseURL }) => {
      const text = Array.from({ length: lines }, (_, i) => (i % 2 ? `  value: ${i}` : `block_${i}:`)).join('\n');
      await page.addInitScript((sample) => {
        window.configEditorSample = sample;
      }, text);
      await page.goto(`${baseURL}?sample=page`);
      await page.locator('.config-editor__input').click();
      await page.keyboard.press('Control+End');
      // The last lines in view (the rows follow the scroll a frame later).
      await expect.poll(async () => (await gutterGeometry(page)).widest.digits).toBe(String(lines).length);
      const { widest, textLeft, gutterLeft } = await gutterGeometry(page);
      // VS Code leaves about 26px (its decorations and folding) after the number.
      const gap = textLeft - widest.right;
      expect(gap, 'from the number to the text').toBeGreaterThanOrEqual(24);
      expect(gap, 'from the number to the text').toBeLessThanOrEqual(28);
      // Before the number: the marker column, and the room of the digits a
      // short document lacks (at least three, so line 100 moves nothing).
      const blank = Math.max(0, 3 - widest.digits) * (widest.width / widest.digits);
      expect(widest.left - gutterLeft - blank, 'from the edge to the number').toBeLessThanOrEqual(29);
    });
  });
}

test.describe('the gutter without folding', () => {
  setupEditor(test, '?sample=large&folding=0');

  test('the text starts about a character after the number', async ({ page }) => {
    const { widest, textLeft } = await gutterGeometry(page);
    expect(textLeft - widest.right).toBeGreaterThanOrEqual(6);
    expect(textLeft - widest.right).toBeLessThanOrEqual(12);
  });
});

test.describe('the gutter markers stay clickable', () => {
  setupEditor(test, '?sample=large');

  test('a change bar and a chevron each own a 24px area clear of the number and the text, and open their control', async ({ page }) => {
    const e = editor(page);
    const before = await gutterGeometry(page);
    // A change on the first line that starts a fold.
    const line = await page.locator('.config-editor__fold-toggle').first().getAttribute('data-line');
    await e.input.click();
    await page.keyboard.press('Control+g');
    await page.keyboard.type(`${line}\n`);
    await page.keyboard.press('End');
    await page.keyboard.type(' x');
    const bar = page.locator(`.config-editor__gutter div[data-line="${line}"] .config-editor__diff-marker`);
    await expect(bar).toBeVisible();
    const toggle = page.locator(`.config-editor__fold-toggle[data-line="${line}"]`);
    await page.locator('.config-editor__gutter').hover();
    await expect(toggle).toBeVisible();
    // No layout shift: the markers take no room of their own.
    const after = await gutterGeometry(page);
    expect(after.gutterWidth).toBe(before.gutterWidth);
    expect(after.textLeft).toBe(before.textLeft);

    const digits = await page.evaluate((line) => {
      const range = document.createRange();
      range.selectNodeContents(document.querySelector(`.config-editor__gutter div[data-line="${line}"] > span`));
      const box = range.getBoundingClientRect();
      return { left: box.left, right: box.right };
    }, line);
    const barArea = await hitBox(bar);
    const toggleArea = await hitBox(toggle);
    for (const [name, area] of [['bar', barArea], ['chevron', toggleArea]]) {
      expect(area.right - area.left, `${name} wide`).toBeGreaterThanOrEqual(24);
      expect(area.bottom - area.top, `${name} tall`).toBeGreaterThanOrEqual(24);
    }
    expect(barArea.right, 'the bar ends before the number').toBeLessThanOrEqual(digits.left + 0.5);
    expect(toggleArea.left, 'the chevron starts after the number').toBeGreaterThanOrEqual(digits.right - 0.5);
    expect(toggleArea.right, 'the chevron ends before the text').toBeLessThanOrEqual(after.textLeft + 0.5);

    // Each area's far side still takes the click: the bar's at the gutter's
    // edge, the chevron's beside the text.
    await page.mouse.click(barArea.left + 2, (barArea.top + barArea.bottom) / 2);
    await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.locator('.config-editor__diff-peek')).toHaveCount(0);
    await page.locator('.config-editor__gutter').hover();
    await page.mouse.click(toggleArea.right - 2, (toggleArea.top + toggleArea.bottom) / 2);
    await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  });
});

/** The centre of `locator`'s own box, or of its `pseudo` box, in page coordinates. */
const centreOf = (locator, pseudo) =>
  locator.evaluate((element, pseudo) => {
    const box = element.getBoundingClientRect();
    if (!pseudo) return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
    const style = getComputedStyle(element, pseudo);
    return {
      x: box.left + parseFloat(style.left) + parseFloat(style.width) / 2,
      y: box.top + parseFloat(style.top) + parseFloat(style.height) / 2,
    };
  }, pseudo);

/** Whether the element at `point` is the one `selector` finds there, else what it is. */
const hitAt = (page, point, selector) =>
  page.evaluate(
    ({ x, y, selector }) => {
      // An icon's drawing is the icon.
      const hit = document.elementFromPoint(x, y);
      const element = hit instanceof SVGElement ? hit.closest('span') : hit;
      return element.matches(selector) ? 'itself' : `${element.tagName}.${element.className}`;
    },
    { ...point, selector },
  );

/**
 * Opens a document whose line 1 is read-only, has a problem (a key the
 * schema does not know) and is changed, and whose line 3 follows a removed
 * line and starts a fold.
 */
async function markedLines(page, baseURL) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.addInitScript(() => {
    window.configEditorSample = 'nonsense_key: 1\nname: x\ngone: 1\nblock:\n  a: 1\n';
  });
  await page.goto(`${baseURL}?sample=page&lock=/nonsense_key`);
  const e = editor(page);
  await e.input.waitFor();
  const before = await gutterGeometry(page);
  await e.input.click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('End');
  await page.keyboard.type(' # c');
  await page.keyboard.press('Control+g');
  await page.keyboard.type('3\n');
  await page.keyboard.press('Shift+End');
  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('Delete');
  const row = (line) => page.locator(`.config-editor__gutter div[data-line="${line}"]`);
  const marks = {
    row,
    before,
    lock: row(1).locator('.config-editor__lock-marker'),
    bar: row(1).locator('.config-editor__diff-marker'),
    removed: row(3).locator('.config-editor__diff-deleted'),
    chevron: page.locator('.config-editor__fold-toggle[data-line="3"]'),
  };
  await expect(row(1)).toHaveClass(/config-editor__line-number-error/, { timeout: 5000 });
  await expect(marks.lock).toBeVisible();
  await expect(marks.bar).toBeVisible();
  await expect(marks.removed).toHaveCount(1);
  return marks;
}

test.describe('the gutter markers sharing a line', () => {
  test('each marker is the element at its own centre', async ({ page, baseURL }) => {
    const { row, lock, bar, removed, chevron } = await markedLines(page, baseURL);
    await page.locator('.config-editor__gutter').hover();
    await expect(chevron).toBeVisible();

    const markers = [
      ['the lock', await centreOf(lock), '.config-editor__lock-marker'],
      ['the problem dot', await centreOf(row(1), '::before'), '.config-editor__line-number-error'],
      ['the change bar', await centreOf(bar), '.config-editor__diff-marker'],
      ['the removed lines', await centreOf(removed, '::before'), '.config-editor__diff-deleted'],
      ['the chevron', await centreOf(chevron), '.config-editor__fold-toggle'],
    ];
    for (const [name, centre, selector] of markers) {
      expect(await hitAt(page, centre, selector), `${name} is under its own centre`).toBe('itself');
    }
    // The lock's title shows on hover; a click on it or on the dot selects
    // the line, and opens no change.
    expect(await lock.getAttribute('title')).toBeTruthy();
    for (const [, centre] of markers.slice(0, 2)) {
      await page.mouse.click(centre.x, centre.y);
      await expect(page.locator('.config-editor__diff-peek')).toHaveCount(0);
    }
    // The bar keeps its area around them: at the gutter's edge it still opens.
    const gutter = await page.locator('.config-editor__gutter').boundingBox();
    await page.mouse.click(gutter.x + 1, markers[2][1].y);
    await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
    expect(page.errors).toEqual([]);
  });

  test("the removed lines' marker is a triangle as tall as VS Code's, and moves nothing", async ({ page, baseURL }) => {
    const { removed, before } = await markedLines(page, baseURL);
    const mark = await removed.evaluate((element) => {
      const style = getComputedStyle(element, '::before');
      return { width: parseFloat(style.width), height: parseFloat(style.height), clip: style.clipPath };
    });
    expect(mark.width, 'wide').toBeGreaterThanOrEqual(4);
    expect(mark.height, 'tall').toBeGreaterThanOrEqual(8);
    expect(mark.clip, 'a triangle').toContain('polygon');
    const after = await gutterGeometry(page);
    expect(after.gutterWidth).toBe(before.gutterWidth);
    expect(after.textLeft).toBe(before.textLeft);
    // Still its 24px area: the click beside it, at the gutter's edge, opens it.
    const box = await removed.boundingBox();
    const gutter = await page.locator('.config-editor__gutter').boundingBox();
    await page.mouse.click(gutter.x + 1, box.y + 3);
    await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
    expect(page.errors).toEqual([]);
  });
});
