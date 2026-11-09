import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 23 (UX): the hover card stays while the pointer travels to it;
// a host problem saying something else never hides the editor's own; hover
// docs on values; the card fits the window; a tap outside closes it; the
// footer keeps one row on a phone; tabs drop their folder when narrow; the
// "deprecated" badge reads in every theme. Through the npm package's pages
// (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (name, options = {}) =>
  `${pathToFileURL(join(js, 'examples', name)).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input');
const tip = (page) => page.locator('.config-editor__problem-tip');
const card = (page) => page.locator('.config-editor__hover');
const footer = (page) => page.locator('.config-editor__diagnostic-status');
const announcement = (page) => page.locator('.config-editor__announcement');
const setHost = (page, list) => page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

async function open(page, options, name = 'vanilla.html') {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(name, options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

// The middle of the character at one-based `line`:`column`, as drawn.
const charBox = (page, line, column) =>
  page.evaluate(
    ([line, column]) => {
      const row = document.querySelectorAll('.config-editor__highlight .config-editor__code-line')[line - 1];
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
      let left = column - 1;
      let node;
      while ((node = walker.nextNode())) {
        if (left < node.length) break;
        left -= node.length;
      }
      const range = document.createRange();
      range.setStart(node, left);
      range.setEnd(node, left + 1);
      const box = range.getBoundingClientRect();
      return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
    },
    [line, column],
  );

// Which popups show now: 'card', 'tip', both or none.
const shown = (page) =>
  page.evaluate(() =>
    ['hover', 'problem-tip']
      .filter((name) => document.querySelector(`.config-editor__${name}`)?.checkVisibility({ visibilityProperty: true }))
      .join('+'),
  );

// Each language's document: `old` (deprecated, with docs and a docs link)
// on `line`, from `column`; `port` (documented too) on the next line.
const schema = JSON.stringify({
  type: 'object',
  properties: {
    old: { type: 'integer', deprecated: true, description: 'Old thing', 'x-docs-url': 'https://example.com/old' },
    port: { type: 'integer', description: 'The port', 'x-docs-url': 'https://example.com/port' },
  },
});
const DOCS = {
  yaml: { value: 'old: 1\nport: 2\nz: 3\n', line: 1, column: 1 },
  json: { value: '{\n  "old": 1,\n  "port": 2\n}\n', line: 2, column: 4 },
  toml: { value: 'old = 1\nport = 2\n', line: 1, column: 1 },
  hcl: { value: 'old = 1\nport = 2\n', line: 1, column: 1 },
  xml: { value: '<old>1</old>\n<port>2</port>\n', line: 1, column: 2 },
};

test.describe('M23-U1: the card with docs and a problem stays on the way to it', () => {
  test.use({ viewport: { width: 800, height: 400 } });

  for (const [language, doc] of Object.entries(DOCS)) {
    test(`${language}: small steps across the next key reach the card and its link`, async ({ page }) => {
      const errors = await open(page, { value: doc.value, language, schema });
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
      const key = await charBox(page, doc.line, doc.column + 1);
      await page.mouse.move(key.x, key.y);
      await expect(card(page)).toBeVisible();
      await expect(card(page)).toContainText(/deprecated/i);
      const box = await card(page).boundingBox();
      // Towards the card's lower left, over the next line's key.
      const target = { x: box.x + 30, y: box.y + box.height - 8 };
      const trace = [];
      for (let step = 1; step <= 12; step += 1) {
        await page.mouse.move(key.x + ((target.x - key.x) * step) / 12, key.y + ((target.y - key.y) * step) / 12);
        trace.push(await shown(page));
      }
      // Slower than the hover pause, the way a hand moves.
      await page.waitForTimeout(400);
      trace.push(await shown(page));
      expect(trace).toEqual(Array(13).fill('hover'));
      await expect(card(page)).toContainText('Old thing');
      const link = card(page).locator('.config-editor__hover-link');
      const linkBox = await link.boundingBox();
      const popup = page.waitForEvent('popup');
      await page.mouse.move(linkBox.x + 5, linkBox.y + linkBox.height / 2, { steps: 4 });
      await page.mouse.click(linkBox.x + 5, linkBox.y + linkBox.height / 2);
      expect((await popup).url()).toBe('https://example.com/old');
      expect(errors).toEqual([]);
    });

    test(`${language}: leaving the card's key elsewhere closes it without the problem tip for that place`, async ({ page }) => {
      await open(page, { value: doc.value, language, schema });
      const key = await charBox(page, doc.line, doc.column + 1);
      await page.mouse.move(key.x, key.y);
      await expect(card(page)).toBeVisible();
      // Sample every frame while the pointer goes up and away.
      const sampled = page.evaluate(
        () =>
          new Promise((resolve) => {
            const seen = new Set();
            const end = performance.now() + 700;
            const look = () => {
              const tip = document.querySelector('.config-editor__problem-tip');
              if (tip?.checkVisibility({ visibilityProperty: true })) seen.add('tip');
              if (performance.now() < end) requestAnimationFrame(look);
              else resolve([...seen]);
            };
            look();
          }),
      );
      await page.mouse.move(key.x + 400, 2, { steps: 5 });
      expect(await sampled).toEqual([]);
      await expect(card(page)).toBeHidden();
    });
  }
});

// Each language's `c: 1`, below the schema's minimum, on `line`.
const MINIMUM = {
  yaml: { value: 'c: 1\n', line: 1 },
  json: { value: '{\n  "c": 1\n}\n', line: 2 },
  toml: { value: 'c = 1\n', line: 1 },
  hcl: { value: 'c = 1\n', line: 1 },
  xml: { value: '<c>1</c>\n', line: 1 },
};
const minimumSchema = JSON.stringify({ type: 'object', properties: { c: { type: 'integer', minimum: 5 } } });

test.describe('M23-U2: a host problem hides the editor’s own only when it says the same', () => {
  for (const [language, doc] of Object.entries(MINIMUM)) {
    test(`${language}: a whole-line host error with another message shows beside it`, async ({ page }) => {
      const errors = await open(page, { value: doc.value, language, schema: minimumSchema });
      const own = page.locator('.config-editor__diagnostic-marker');
      await expect(own).toHaveCount(1);
      const message = await own.getAttribute('data-message');
      const length = doc.value.split('\n')[doc.line - 1].length;
      const whole = { startLine: doc.line, startColumn: 1, endLine: doc.line, endColumn: length + 1 };
      await setHost(page, [{ range: whole, severity: 'error', message: 'H line' }]);
      await expect(footer(page)).toHaveText(/^2 errors/);
      // F8 reaches both, and so do screen readers.
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      const said = [];
      for (let press = 0; press < 2; press += 1) {
        await page.keyboard.press('F8');
        await expect(announcement(page)).toContainText(' of 2');
        said.push(await announcement(page).textContent());
      }
      expect(said.join('\n')).toContain('H line');
      expect(said.join('\n')).toContain('minimum');
      // The same message there: once, the host's.
      await setHost(page, [{ range: whole, severity: 'error', message: message.replace(/^\/c: /, '') }]);
      await expect(footer(page)).toHaveText(/^1 error/);
      expect(errors).toEqual([]);
    });
  }
});

const valueSchema = JSON.stringify({
  type: 'object',
  properties: { port: { type: 'integer', description: 'The port' }, n: { type: 'integer', description: 'A number' } },
});
// Each language's `port: x` (not an integer) and `n: 3`: (line, column of the value).
const VALUES = {
  yaml: { value: 'port: x\nn: 3\n', at: [[1, 7], [2, 4]] },
  json: { value: '{\n  "port": "x",\n  "n": 3\n}\n', at: [[2, 12], [3, 8]] },
  toml: { value: 'port = "x"\nn = 3\n', at: [[1, 9], [2, 5]] },
  hcl: { value: 'port = "x"\nn = 3\n', at: [[1, 9], [2, 5]] },
  xml: { value: '<port>x</port>\n<n>3</n>\n', at: [[1, 7], [2, 4]] },
};

test.describe('L23-U2: the pointer on a value shows its property’s docs', () => {
  for (const [language, doc] of Object.entries(VALUES)) {
    test(`${language}: with its problem, and without one`, async ({ page }) => {
      const errors = await open(page, { value: doc.value, language, schema: valueSchema });
      await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
      const [[line, column], [plainLine, plainColumn]] = doc.at;
      const value = await charBox(page, line, column);
      await page.mouse.move(value.x, value.y);
      await expect(card(page)).toBeVisible();
      await expect(card(page)).toContainText('The port');
      await expect(card(page).locator('.config-editor__problem-tip-entry')).toHaveCount(1);
      await expect(tip(page)).toBeHidden();
      await page.mouse.move(790, 390);
      await expect(card(page)).toBeHidden();
      const plain = await charBox(page, plainLine, plainColumn);
      await page.mouse.move(plain.x, plain.y);
      await expect(card(page)).toContainText('A number');
      expect(errors).toEqual([]);
    });
  }
});

test.describe('L23-U3: a card with many problems fits the window and scrolls', () => {
  test.use({ viewport: { width: 800, height: 400 } });

  for (const line of [1, 12]) {
    test(`on line ${line}`, async ({ page }) => {
      const value = Array.from({ length: 14 }, (_, i) => `k${i}: 1`).join('\n') + '\n';
      const keySchema = JSON.stringify({
        type: 'object',
        additionalProperties: { type: 'integer', description: 'A key', 'x-docs-url': 'https://example.com/k' },
      });
      await open(page, { value, schema: keySchema });
      await page.addStyleTag({ content: 'header { display: none }' });
      await setHost(
        page,
        Array.from({ length: 25 }, (_, i) => ({
          range: { startLine: line, startColumn: 1, endColumn: 3 },
          severity: 'warning',
          message: `problem number ${i} with a message long enough to take a line or two of its own`,
        })),
      );
      const key = await charBox(page, line, 1);
      await page.mouse.move(key.x, key.y);
      await expect(card(page)).toBeVisible();
      await expect(card(page)).toHaveClass(/config-editor__hover--scrolls/);
      const fits = await card(page).evaluate((element) => {
        const box = element.getBoundingClientRect();
        return { top: box.top >= 0, bottom: box.bottom <= innerHeight, scrolls: element.scrollHeight > element.clientHeight };
      });
      expect(fits).toEqual({ top: true, bottom: true, scrolls: true });
      // The pointer reaches it and the wheel scrolls it.
      const box = await card(page).boundingBox();
      await page.mouse.move(box.x + 20, box.y + box.height / 2, { steps: 6 });
      await page.mouse.wheel(0, 200);
      await expect.poll(() => card(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    });
  }
});

test.describe('L23-U5: a tap outside the editor closes the card', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 700 } });

  for (const [language, doc] of Object.entries(DOCS)) {
    test(`${language}`, async ({ page }) => {
      const errors = await open(page, { value: doc.value, language, schema });
      const key = await charBox(page, doc.line, doc.column + 1);
      await page.touchscreen.tap(key.x, key.y);
      await expect(card(page)).toBeVisible();
      const below = await page.locator('.config-editor').evaluate((editor) => editor.getBoundingClientRect().bottom + 30);
      await page.touchscreen.tap(200, Math.min(below, 690));
      await expect(card(page)).toBeHidden();
      await expect(tip(page)).toBeHidden();
      expect(errors).toEqual([]);
    });
  }
});

test.describe('L23-U1: the footer keeps one row on a phone', () => {
  for (const width of [390, 480]) {
    test(`at ${width} px`, async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width, height: 500 } });
      const page = await context.newPage();
      await open(page, { value: 'listen: 7835\nflag: nope\n', schema: JSON.stringify({ properties: { flag: { type: 'boolean' } } }) });
      await expect(footer(page)).toContainText('1 error');
      const layout = await page.locator('.config-editor__status').evaluate((status) => {
        const box = status.getBoundingClientRect();
        const summary = status.querySelector('.config-editor__diagnostic-status').getBoundingClientRect();
        return { height: Math.round(box.height), leftGap: Math.round(summary.left - box.left) };
      });
      expect(layout.height).toBeLessThanOrEqual(29);
      // No empty half: the summary starts at the footer's left.
      expect(layout.leftGap).toBeLessThanOrEqual(12);
      await context.close();
    });
  }
});

test.describe('L23-U7: narrow tabs leave their folder out', () => {
  test.use({ viewport: { width: 390, height: 600 } });

  test('the names that tell the tabs apart show; the title and the name read keep the folder', async ({ page }) => {
    await open(page, {}, 'documents.html');
    const add = page.locator('.config-editor-documents__add');
    await add.click();
    await add.click();
    const tabs = page.locator('[role="tab"]');
    const added = tabs.filter({ hasText: 'new' });
    await expect(added).toHaveCount(2);
    for (const tab of await added.all()) {
      const title = await tab.getAttribute('title');
      expect(title).toMatch(/\//);
      const seen = await tab.locator('.config-editor-documents__name').innerText();
      expect(seen).not.toContain('/');
      expect(title.endsWith(seen)).toBe(true);
      expect(await tab.getAttribute('aria-label')).toContain(title);
    }
    const seen = await added.locator('.config-editor-documents__name').allInnerTexts();
    expect(new Set(seen).size).toBe(2);
  });
});

test.describe('L23-U8: the deprecated badge reads in every theme', () => {
  const luminance = (rgb) =>
    rgb
      .map((part) => part / 255)
      .map((part) => (part <= 0.03928 ? part / 12.92 : ((part + 0.055) / 1.055) ** 2.4))
      .reduce((sum, part, i) => sum + part * [0.2126, 0.7152, 0.0722][i], 0);
  const ratio = (a, b) => {
    const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (light + 0.05) / (dark + 0.05);
  };

  test('at least 4.5:1 on the card', async ({ page }) => {
    await open(page, { value: 'old: 1\n', schema });
    const themes = await page.locator('#theme option').evaluateAll((options) => options.map((option) => option.value));
    expect(themes.length).toBeGreaterThan(10);
    const failures = [];
    for (const theme of themes) {
      await page.evaluate((theme) => window.editor.update({ theme }), theme);
      // Away until the last theme's card has gone, then onto the key again.
      await page.mouse.move(790, 590);
      await expect(card(page)).toHaveCount(0);
      const key = await charBox(page, 1, 2);
      await page.mouse.move(key.x - 2, key.y);
      await page.mouse.move(key.x, key.y);
      const badge = card(page).locator('.config-editor__hover-deprecated');
      await expect(badge).toBeVisible();
      const [fore, back] = await badge.evaluate((element) => {
        const rgb = (color) => color.match(/[\d.]+/g).slice(0, 3).map(Number);
        return [rgb(getComputedStyle(element).color), rgb(getComputedStyle(element.closest('.config-editor__hover')).backgroundColor)];
      });
      const measured = ratio(fore, back);
      if (measured < 4.5) failures.push(`${theme}: ${measured.toFixed(2)}`);
    }
    expect(failures).toEqual([]);
  });
});
