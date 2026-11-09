import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 26 (accessibility and mobile): PageDown / PageUp in an editor
// taller than the window page by the rows in the window and keep the
// caret in view, at the on-screen column ArrowDown keeps; every editing
// command in a read-only editor says it cannot edit, as typing does; the
// problems tip closes when the focus leaves the editor; each docs link in
// a tip is named after its problem, and Enter / Alt+Enter on the focused
// tip open the focused or in-view problem's docs. Through the npm
// package's example pages (run `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name, options) =>
  `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 1000, height: 700 } });

const input = (page) => page.locator('.config-editor__input:visible').first();
/** What the editor with the visible text says through its live region. */
const said = (page) => input(page).evaluate((element) => element.closest('.config-editor').querySelector('.config-editor__announcement').textContent);
const tip = (page) => page.locator('.config-editor__problem-tip');

async function open(page, options, name = 'vanilla.html') {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(example(name, options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

/** Waits `count` frames. */
const frames = (page, count = 3) =>
  page.evaluate(
    (count) =>
      new Promise((done) => {
        const step = (left) => (left ? requestAnimationFrame(() => step(left - 1)) : done());
        step(count);
      }),
    count,
  );

const chord = async (page) => {
  await page.keyboard.press('Control+k');
  await page.keyboard.press('Control+i');
};

/** The caret's line (one-based) and its row's top and bottom in the window. */
const caretRow = (page) =>
  input(page).evaluate((element) => {
    const style = getComputedStyle(element);
    const lineHeight = parseFloat(style.lineHeight);
    const line = element.value.slice(0, element.selectionStart).split('\n').length;
    const rect = element.getBoundingClientRect();
    const top = rect.top + element.clientTop + parseFloat(style.paddingTop) + (line - 1) * lineHeight - element.scrollTop;
    return { line, top, bottom: top + lineHeight, lineHeight, window: document.documentElement.clientHeight };
  });

/** Where the caret is: one-based line and UTF-16 column. */
const caretAt = (page) =>
  input(page).evaluate((element) => {
    const lines = element.value.slice(0, element.selectionStart).split('\n');
    return `${lines.length}:${lines.at(-1).length + 1}`;
  });

const LINES = Array.from({ length: 200 }, (_, i) => `k${i + 1}: ${i + 1}\n`).join('');

// Editors taller than the window.
const TALL = {
  'growing with its text': () => {
    document.head.insertAdjacentHTML('beforeend', '<style>.config-editor{--config-editor-max-height:none}</style>');
  },
  'a tall height of its own': () => {
    document.head.insertAdjacentHTML('beforeend', '<style>.config-editor{--config-editor-height:2400px;--config-editor-max-height:none}</style>');
  },
  'filling a tall host box': () => {
    const editor = document.querySelector('.config-editor');
    const box = document.createElement('div');
    box.style.height = '3000px';
    editor.before(box);
    box.append(editor);
    document.head.insertAdjacentHTML('beforeend', '<style>.config-editor{height:100%;--config-editor-height:100%;--config-editor-max-height:none}</style>');
  },
};

test.describe('N26-A1: page keys in an editor taller than the window', () => {
  for (const [layout, setUp] of Object.entries(TALL)) {
    test(`${layout}: a page is the rows in the window, and the caret stays in view`, async ({ page }) => {
      const errors = await open(page, { value: LINES });
      await page.evaluate(setUp);
      await frames(page);
      const height = await input(page).evaluate((element) => element.clientHeight);
      expect(height).toBeGreaterThan(await page.evaluate(() => innerHeight * 2));
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await frames(page);
      let before = await caretRow(page);
      const inWindow = Math.floor(before.window / before.lineHeight);
      for (let press = 0; press < 4; press += 1) {
        const scrolled = await page.evaluate(() => scrollY);
        await page.keyboard.press('PageDown');
        await frames(page);
        const now = await caretRow(page);
        const moved = now.line - before.line;
        // A page of the window's rows, not the text area's.
        expect(moved, `press ${press}`).toBeGreaterThan(inWindow / 2);
        expect(moved, `press ${press}`).toBeLessThan(inWindow);
        // In view, the page scrolled no further than that takes.
        expect(now.top, `press ${press}`).toBeGreaterThanOrEqual(-1);
        expect(now.bottom, `press ${press}`).toBeLessThanOrEqual(now.window + 1);
        if ((await page.evaluate(() => scrollY)) !== scrolled) {
          expect(now.bottom, `press ${press}`).toBeGreaterThan(now.window - 2 * now.lineHeight);
        }
        before = now;
      }
      for (let press = 0; press < 12 && before.line > 1; press += 1) {
        await page.keyboard.press('PageUp');
        await frames(page);
        const now = await caretRow(page);
        expect(now.line).toBeLessThan(before.line);
        expect(now.top).toBeGreaterThanOrEqual(-1);
        expect(now.bottom).toBeLessThanOrEqual(now.window + 1);
        before = now;
      }
      expect(before.line).toBe(1);
      expect(errors).toEqual([]);
    });
  }
});

test.describe('N26-A2: page keys keep the on-screen column, as ArrowDown does', () => {
  const filler = (count) => Array.from({ length: count }, (_, i) => `k${i + 2}: ${'y'.repeat(40)}\n`).join('');
  // [first line, the line a page down lands on]: from the first line's end.
  const CASES = {
    emoji: ['a: 😀😀😀', null],
    tabs: ['a:\t\tx', null],
    'wide characters': ['a: 漢字漢字漢字', null],
    'combining marks': ['a: e\u0301e\u0301e\u0301', null],
    'mixed widths': ['a: iWmlW.i', null],
    'onto mixed widths': ['a: yyyyyyy', 'k: iWiWmlimW'],
    'onto tabs': ['a: yyyyyyy', 'k:\t\t\t\tz'],
    'onto emoji': ['a: yyyyyyy', 'k: 😀😀😀😀😀😀'],
    'onto combining marks': ['a: yyyyy', 'k: e\u0301e\u0301e\u0301e\u0301'],
  };
  // The column is where the text is drawn, in whatever font: the host's,
  // or the machine's (which draws emoji and wide characters at widths of
  // its own), so no count of columns finds it; in a proportional font no
  // two lines' columns line up at all.
  const FONTS = {
    'the default font': null,
    'a proportional font': 'sans-serif',
    'a serif font': 'serif',
  };
  /** The caret's x on its row, measured on the rendered line, in pixels. */
  const caretX = (page) =>
    input(page).evaluate((element) => {
      const before = element.value.slice(0, element.selectionStart).split('\n');
      const row = element.closest('.config-editor').querySelector(`.config-editor__code-line[data-line="${before.length}"]`);
      let left = before.at(-1).length;
      const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
      const range = document.createRange();
      range.setStart(row, 0);
      range.setEnd(row, 0);
      for (let node = walker.nextNode(); node && left; node = walker.nextNode()) {
        const take = Math.min(left, node.length);
        range.setEnd(node, take);
        left -= take;
      }
      return range.getBoundingClientRect().right - row.getBoundingClientRect().left;
    });
  for (const [font, family] of Object.entries(FONTS)) {
    for (const [name, [first, landing]] of Object.entries(CASES)) {
      test(`${font}, ${name}: PageDown and PageUp land where ArrowDown and ArrowUp do`, async ({ page }) => {
        const errors = await open(page, { value: `${first}\n${filler(80)}` });
        if (family) {
          await page.evaluate((family) => {
            document.head.insertAdjacentHTML('beforeend', `<style>.config-editor{--config-editor-mono:${family}}</style>`);
          }, family);
          await frames(page);
        }
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('PageDown');
        const rows = Number((await caretAt(page)).split(':')[0]) - 1;
        expect(rows).toBeGreaterThan(3);
        if (landing) {
          // The line a page lands on holds the characters to land among.
          await input(page).evaluate(
            (element, [rows, landing]) => {
              const lines = element.value.split('\n');
              lines[rows] = landing;
              window.editor.update({ value: lines.join('\n') });
            },
            [rows, landing],
          );
          await frames(page);
        }
        /** Where `presses` of `key` take the caret: line and column, and x. */
        const press = async (key, presses = 1) => {
          for (let i = 0; i < presses; i += 1) await page.keyboard.press(key);
          await frames(page);
          return [await caretAt(page), await caretX(page)];
        };
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('End');
        const [down, downX] = await press('ArrowDown', rows);
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('End');
        const [pagedDown, pagedDownX] = await press('PageDown');
        expect(pagedDown, 'PageDown').toBe(down);
        expect(Math.abs(pagedDownX - downX), 'PageDown, x').toBeLessThanOrEqual(1);
        // And back up from there, onto the first line.
        await page.keyboard.press('End');
        const from = await caretAt(page);
        const [up, upX] = await press('ArrowUp', rows);
        await page.keyboard.press('Control+Home');
        for (let i = 0; i < rows; i += 1) await page.keyboard.press('ArrowDown');
        await page.keyboard.press('End');
        expect(await caretAt(page)).toBe(from);
        const [pagedUp, pagedUpX] = await press('PageUp');
        expect(pagedUp, 'PageUp').toBe(up);
        expect(Math.abs(pagedUpX - upX), 'PageUp, x').toBeLessThanOrEqual(1);
        expect(errors).toEqual([]);
      });
    }
  }
});

test.describe('N26-A3: editing commands in a read-only editor say it cannot edit', () => {
  const COMMANDS = {
    'Ctrl+X without a selection (cut the line)': 'Control+x',
    'Ctrl+/ (toggle a comment)': 'Control+Slash',
    'Ctrl+Enter (a line below)': 'Control+Enter',
    'Ctrl+Shift+Enter (a line above)': 'Control+Shift+Enter',
  };
  for (const where of ['ConfigEditor', 'documents']) {
    for (const [command, key] of Object.entries(COMMANDS)) {
      // The read-only document is JSON, which has no comments to toggle.
      if (where === 'documents' && key === 'Control+Slash') continue;
      test(`${where}, ${command}: says so, at the caret, and changes nothing`, async ({ page, context }) => {
        await context.grantPermissions(['clipboard-read', 'clipboard-write']);
        const errors =
          where === 'documents'
            ? await open(page, undefined, 'documents.html')
            : await open(page, { value: 'a: 1\nb: 2\n', readOnly: true });
        if (where === 'documents') {
          await page.getByRole('tab', { name: /clients/ }).click();
        }
        const value = await input(page).inputValue();
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('ArrowDown');
        const at = await caretAt(page);
        await page.keyboard.press(key);
        await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
        const notice = page.locator('.config-editor__read-only-notice:visible');
        await expect(notice).toBeVisible();
        await expect(notice).toHaveText(/read-only/);
        expect(await input(page).inputValue()).toBe(value);
        expect(await caretAt(page)).toBe(at);
        expect(errors).toEqual([]);
      });
    }
  }

  test('undo after the host made the editor read-only: says so, and undoes nothing', async ({ page }) => {
    const errors = await open(page, { value: 'a: 1\n' });
    await input(page).focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('b: 2\n');
    await page.evaluate(() => window.editor.update({ readOnly: true }));
    const value = await input(page).inputValue();
    await page.keyboard.press('Control+z');
    await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
    await expect(page.locator('.config-editor__read-only-notice')).toBeVisible();
    expect(await input(page).inputValue()).toBe(value);
    expect(errors).toEqual([]);
  });

  test('F8 still walks the problems', async ({ page }) => {
    const schema = JSON.stringify({ type: 'object', properties: { b: { type: 'integer' } } });
    const errors = await open(page, { value: 'a: 1\nb: x\n', schema, readOnly: true });
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(tip(page)).toBeVisible();
    expect(await caretAt(page)).toMatch(/^2:/);
    expect(errors).toEqual([]);
  });
});

test.describe('N26-A4: the problems tip closes when the focus leaves the editor', () => {
  const schema = JSON.stringify({ type: 'object', properties: { a: { type: 'integer' } }, additionalProperties: false });
  const value = 'a: x\nzz: 1\n';
  const withHostButton = (page) =>
    page.evaluate(() => {
      const after = document.createElement('button');
      after.id = 'host-after';
      after.textContent = 'After';
      document.querySelector('.config-editor').after(after);
    });

  test('from the text, by the keyboard and the mouse', async ({ page }) => {
    const errors = await open(page, { value, schema });
    await withHostButton(page);
    for (const leave of [(page) => page.locator('#host-after').focus(), (page) => page.locator('#host-after').click()]) {
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('F8');
      await expect(tip(page)).toBeVisible();
      await leave(page);
      await expect(tip(page)).toBeHidden();
      await expect(page.locator('#host-after')).toBeFocused();
    }
    expect(errors).toEqual([]);
  });

  test('from the focused tip, by Tab', async ({ page }) => {
    const errors = await open(page, { value, schema, hostDiagnostics: [{ path: '/zz', severity: 'warning', message: 'host says', docsUrl: 'https://example.com/zz' }] });
    await withHostButton(page);
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await page.keyboard.press('F8');
    await expect(tip(page)).toBeVisible();
    await chord(page);
    await expect(tip(page)).toBeFocused();
    // Within the tip, it stays; out of the editor, it closes.
    for (let press = 0; press < 12; press += 1) {
      await page.keyboard.press('Tab');
      const outside = await page.evaluate(() => !document.activeElement.closest('.config-editor'));
      if (outside) break;
      const inTip = await page.evaluate(() => Boolean(document.activeElement.closest('.config-editor__problem-tip')));
      if (inTip) await expect(tip(page)).toBeVisible();
    }
    expect(await page.evaluate(() => !document.activeElement.closest('.config-editor'))).toBe(true);
    await expect(tip(page)).toBeHidden();
    // Moving into it from the text keeps it.
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('F8');
    await expect(tip(page)).toBeVisible();
    await chord(page);
    await expect(tip(page)).toBeFocused();
    await expect(tip(page)).toBeVisible();
    expect(errors).toEqual([]);
  });
});

test.describe('N26-A5: a tip listing many problems with docs', () => {
  const many = Array.from({ length: 30 }, (_, i) => ({
    path: '/zz',
    severity: 'warning',
    message: `Host message number ${i} that is fairly long to need wrapping in the tip`,
    docsUrl: `https://example.com/d${i}`,
  }));

  async function focusedTip(page) {
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    await page.keyboard.press('ArrowLeft');
    await chord(page);
    await expect(tip(page)).toBeVisible();
    await chord(page);
    await expect(tip(page)).toBeFocused();
  }

  test('each docs link is named after its problem', async ({ page }) => {
    const errors = await open(page, { value: 'a: 1\nzz: 1\n', hostDiagnostics: many });
    await focusedTip(page);
    const names = await tip(page)
      .locator('a')
      .evaluateAll((links) => links.map((link) => link.getAttribute('aria-label')));
    expect(names).toHaveLength(30);
    expect(new Set(names).size).toBe(30);
    for (const name of names) {
      // Its visible text first (as speech input says it), then its problem.
      expect(name).toMatch(/^Open docs ↗ \(Alt\+Enter\): Host message number \d+ /);
    }
    await expect(page.getByRole('link', { name: /Host message number 7 / })).toHaveCount(1);
    expect(errors).toEqual([]);
  });

  test('Enter and Alt+Enter open the problem in view, or the link focused', async ({ page, context }) => {
    const errors = await open(page, { value: 'a: 1\nzz: 1\n', hostDiagnostics: many });
    await context.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
    const opens = async (key) => {
      const popup = context.waitForEvent('page', { timeout: 3000 });
      await page.keyboard.press(key);
      const url = (await popup).url();
      await (await popup).close();
      await page.bringToFront();
      return url;
    };
    for (const key of ['Enter', 'Alt+Enter']) {
      await focusedTip(page);
      // At the top: the first problem's.
      const first = await tip(page).locator('a').first().getAttribute('href');
      expect(await opens(key)).toBe(first);
      // Scrolled down to the twelfth problem: its.
      await focusedTip(page);
      const twelfth = tip(page).locator('.config-editor__problem-tip-entry').nth(11);
      await twelfth.evaluate((entry) => {
        const box = entry.closest('.config-editor__problem-tip');
        box.scrollTop = entry.offsetTop - box.clientTop;
      });
      expect(await opens(key)).toBe(await twelfth.locator('a').getAttribute('href'));
      await page.keyboard.press('Escape');
    }
    // Alt+Enter on a link: that link's.
    await focusedTip(page);
    const third = tip(page).locator('a').nth(2);
    await third.focus();
    expect(await opens('Alt+Enter')).toBe(await third.getAttribute('href'));
    expect(errors).toEqual([]);
  });
});
