import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Scrolling by keys, wheel and finger: PageUp/PageDown keep the column and the host page, the wheel and
// a swipe pass to the page at the text's end, the gutter scrolls the text.

// A group of its own (needs the built npm packages).
test.describe(() => {
  // QA round 24 (accessibility and mobile): PageDown / PageUp in an editor
  // that fits the window scroll the editor only, never the page; in a short
  // window the text stays within its surface (every row has its line
  // number) and the find widget within the window; under forced colors the
  // selected suggestion is all HighlightText; the Edit-as notice's Dismiss
  // and the hover card's Documentation link are 24px targets. Through the
  // npm package's example pages (run `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(example('vanilla.html', options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  /** Waits two frames: what was put back before the next frame is drawn is back. */
  const frames = (page) => page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

  // Sixty keys in each language, so a page key has somewhere to go.
  const DOCUMENTS = {
    yaml: Array.from({ length: 60 }, (_, i) => `k${i}: ${i}\n`).join(''),
    json: `${JSON.stringify(Object.fromEntries(Array.from({ length: 60 }, (_, i) => [`k${i}`, i])), null, 2)}\n`,
    toml: Array.from({ length: 60 }, (_, i) => `k${i} = ${i}\n`).join(''),
  };

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('L24-A2: PageDown / PageUp in an editor that fits the window', () => {
    test.use({ viewport: { width: 1100, height: 700 } });

    for (const [language, value] of Object.entries(DOCUMENTS)) {
      for (const tallPage of [false, true]) {
        test(`${language}${tallPage ? ', in a tall page' : ''}: scroll the editor, never the page`, async ({ page }) => {
          const errors = await open(page, { value, language });
          if (tallPage) {
            await page.evaluate(() => {
              const filler = document.createElement('div');
              filler.style.height = '2000px';
              document.body.append(filler);
            });
          }
          const inView = await input(page).evaluate((element) => {
            const rect = element.getBoundingClientRect();
            return rect.top >= 0 && rect.bottom <= innerHeight;
          });
          expect(inView).toBe(true);
          await input(page).focus();
          const caretLine = () => input(page).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
          const scrolls = [];
          for (const key of ['PageDown', 'PageDown', 'PageUp', 'Shift+PageDown']) {
            await page.keyboard.press(key);
            await frames(page);
            scrolls.push(await page.evaluate(() => scrollY));
          }
          expect(scrolls).toEqual([0, 0, 0, 0]);
          // The keys still did their work: the caret moved a page down, and
          // the text scrolled.
          expect(await caretLine()).toBeGreaterThan(10);
          expect(await input(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
          expect(errors).toEqual([]);
        });
      }
    }
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  // QA round 25 (accessibility and mobile): PageDown / PageUp never move the
  // host page or a host's scroll box, not even for a frame, from the text
  // (in view, partly below the fold, in a host box) or from any of the
  // editor's controls, while the text still pages; Enter and Alt+Enter in
  // the focused hover card open its docs, and the focused card is a named
  // dialog; Alt+F5, Shift+Alt+F5 and folding say what they did; typing in a
  // read-only editor says it cannot edit. Through the npm package's example
  // pages (run `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

  const card = (page) => page.locator('.config-editor__hover');

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

  const SIXTY = {
    yaml: Array.from({ length: 60 }, (_, i) => `k${i}: ${i}\n`).join(''),
    json: `${JSON.stringify(Object.fromEntries(Array.from({ length: 60 }, (_, i) => [`k${i}`, i])), null, 2)}\n`,
    toml: Array.from({ length: 60 }, (_, i) => `k${i} = ${i}\n`).join(''),
  };

  // Where the editor is on the host's page.
  const LAYOUTS = {
    // In view, the page taller than the window.
    'in view': () => {
      const filler = document.createElement('div');
      filler.style.height = '2000px';
      document.body.append(filler);
    },
    // Its top in view, the rest below the fold.
    'partly below the fold': () => {
      const editor = document.querySelector('.config-editor').parentElement;
      const before = document.createElement('div');
      before.style.height = `${innerHeight - 260}px`;
      editor.before(before);
      const filler = document.createElement('div');
      filler.style.height = '2000px';
      document.body.append(filler);
    },
    // In a host's scroll box, which has room to scroll.
    'in a host box': () => {
      const editor = document.querySelector('.config-editor').parentElement;
      const box = document.createElement('div');
      box.dataset.hostBox = '';
      box.style.cssText = 'height: 600px; overflow: auto';
      editor.before(box);
      box.append(editor);
      const filler = document.createElement('div');
      filler.style.height = '800px';
      box.append(filler);
      const after = document.createElement('div');
      after.style.height = '2000px';
      document.body.append(after);
    },
  };

  /** Records every scroll outside the editor, and each frame whose scroll positions outside it differ from now. */
  const watchHost = (page) =>
    page.evaluate(() => {
      const outside = (node) => !(node instanceof Element && node.closest('.config-editor, .config-editor-documents'));
      const boxes = () =>
        [document.scrollingElement, ...document.querySelectorAll('[data-host-box]')].map((box) => `${box.scrollTop},${box.scrollLeft}`).join(' ');
      window.__moves = [];
      window.__still = boxes();
      if (!window.__watching) {
        window.__watching = true;
        addEventListener('scroll', (event) => outside(event.target) && window.__moves.push(`scroll on ${event.target.nodeName}`), true);
        const frame = () => {
          const now = boxes();
          if (now !== window.__still) window.__moves.push(`frame at ${now}`);
          requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      }
    });

  const hostMoves = (page) => page.evaluate(() => window.__moves);

  /** Presses each of `keys` and expects nothing outside the editor to have moved. */
  async function pressStill(page, keys) {
    for (const key of keys) {
      await watchHost(page);
      await page.keyboard.press(key);
      await frames(page);
      expect(await hostMoves(page), key).toEqual([]);
    }
  }

  const caretLine = (page) => input(page).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('L25-A1, A2, H25-L4: page keys never move the host page', () => {
    for (const [layout, setUp] of Object.entries(LAYOUTS)) {
      for (const [language, value] of Object.entries(SIXTY)) {
        test(`${layout}, ${language}: the text pages, the host stays`, async ({ page }) => {
          const errors = await open(page, { value, language });
          await page.evaluate(setUp);
          await input(page).focus();
          await pressStill(page, ['PageDown', 'PageDown']);
          const paged = await caretLine(page);
          expect(paged).toBeGreaterThan(10);
          expect(await input(page).evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
          await pressStill(page, ['PageUp']);
          expect(await caretLine(page)).toBeLessThan(paged);
          // Shift selects a page.
          await pressStill(page, ['Shift+PageDown']);
          const [start, end] = await input(page).evaluate((element) => [element.selectionStart, element.selectionEnd]);
          expect(end).toBeGreaterThan(start);
          await pressStill(page, ['PageUp', 'PageUp', 'PageUp']);
          expect(await caretLine(page)).toBe(1);
          expect(errors).toEqual([]);
        });
      }

      test(`${layout}: from the find and Go to Line fields, the find buttons, the hover card and the change preview`, async ({ page }) => {
        const value = SIXTY.yaml;
        const schema = JSON.stringify({
          type: 'object',
          properties: { k0: { type: 'integer', description: Array.from({ length: 40 }, (_, i) => `Paragraph ${i}.`).join('\n\n') } },
        });
        const errors = await open(page, { value, schema, baseline: value.replace('k3: 3', 'k3: 30') });
        await page.evaluate(setUp);
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await page.keyboard.press('Control+f');
        await expect(page.locator('.config-editor__find .config-editor__find-input').first()).toBeFocused();
        await page.keyboard.type('k');
        await pressStill(page, ['PageDown', 'PageUp', 'Shift+PageDown']);
        // Each button of the find widget.
        const buttons = await page.locator('.config-editor__find button:visible').count();
        expect(buttons).toBeGreaterThan(0);
        for (let index = 0; index < buttons; index += 1) {
          await page.locator('.config-editor__find button:visible').nth(index).focus();
          await pressStill(page, ['PageDown', 'PageUp']);
        }
        await page.keyboard.press('Escape');
        await input(page).focus();
        await page.keyboard.press('Control+g');
        await page.keyboard.type('30');
        await pressStill(page, ['PageDown', 'PageUp']);
        await page.keyboard.press('Escape');
        // The focused hover card scrolls itself.
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        await chord(page);
        await expect(card(page)).toBeVisible();
        await chord(page);
        await expect(card(page)).toBeFocused();
        await pressStill(page, ['PageDown', 'Shift+PageDown', 'PageUp']);
        await page.keyboard.press('Escape');
        // The change preview, its revert button focused.
        await input(page).focus();
        await page.keyboard.press('Alt+F3');
        await expect(page.locator('.config-editor__diff-peek')).toBeVisible();
        await pressStill(page, ['PageDown', 'PageUp']);
        await page.locator('.config-editor__diff-peek-close').focus();
        await pressStill(page, ['PageDown', 'PageUp']);
        expect(errors).toEqual([]);
      });
    }

    test('a documents tab, and the editor in the documents view', async ({ page }) => {
      const errors = await open(page, undefined, 'documents.html');
      await page.evaluate(LAYOUTS['in view']);
      await page.locator('[role=tab][aria-selected=true]').focus();
      await pressStill(page, ['PageDown', 'PageUp', 'Shift+PageDown']);
      await page.locator('.config-editor-documents__add, [role=tablist] ~ button').first().focus();
      await pressStill(page, ['PageDown', 'PageUp']);
      await input(page).focus();
      await pressStill(page, ['PageDown', 'PageUp']);
      expect(errors).toEqual([]);
    });

    test('a long change preview pages itself', async ({ page }) => {
      const value = SIXTY.yaml;
      const baseline = value.replace('k3: 3\n', `${Array.from({ length: 80 }, (_, i) => `gone${i}: ${i}\n`).join('')}k3: 3\n`);
      const errors = await open(page, { value, baseline });
      await page.evaluate(LAYOUTS['in view']);
      await input(page).focus();
      await page.keyboard.press('Alt+F3');
      const peek = page.locator('.config-editor__diff-peek');
      await expect(peek).toBeVisible();
      expect(await peek.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
      await pressStill(page, ['PageDown']);
      expect(await peek.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
      await pressStill(page, ['PageUp']);
      expect(await peek.evaluate((element) => element.scrollTop)).toBe(0);
      expect(errors).toEqual([]);
    });
  });

  test.describe('U25-3: the wheel and a swipe go on to the page once the text has nowhere to scroll', () => {
    for (const lines of [2, 100]) {
      test(`${lines} lines: the wheel over the editor scrolls the page at the text's end`, async ({ page }) => {
        const value = Array.from({ length: lines }, (_, i) => `k${i}: ${i}\n`).join('');
        const errors = await open(page, { value });
        await page.evaluate(() => (document.body.style.minHeight = '4000px'));
        await input(page).evaluate((element) => (element.scrollTop = element.scrollHeight));
        const box = await input(page).boundingBox();
        await page.mouse.move(box.x + box.width / 2, box.y + Math.min(box.height / 2, 200));
        await page.mouse.wheel(0, 300);
        await expect.poll(() => page.evaluate(() => scrollY)).toBeGreaterThan(0);
        expect(errors).toEqual([]);
      });
    }

    test('a swipe over a short editor scrolls the page', async ({ browser }) => {
      const context = await browser.newContext({ viewport: { width: 390, height: 700 }, hasTouch: true, isMobile: true });
      const page = await context.newPage();
      const errors = await open(page, { value: 'a: 1\nb: 2\n' });
      await page.evaluate(() => (document.body.style.minHeight = '3000px'));
      const box = await input(page).boundingBox();
      const cdp = await context.newCDPSession(page);
      const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
      for (let step = 1; step <= 15; step += 1) {
        await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y: y - step * 15 }] });
        await page.waitForTimeout(16);
      }
      await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      await expect.poll(() => page.evaluate(() => scrollY)).toBeGreaterThan(0);
      expect(errors).toEqual([]);
      await context.close();
    });
  });
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
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

  const input = (page) => page.locator('.config-editor__input:visible').first();

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

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

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
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  // QA round 34, UX: the wheel and a finger over the line numbers scroll the
  // text as over the text itself, the page only at the text's ends (UX34-1);
  // clearing a find query that said "No results" takes that back from the
  // live region (UX34-5); the wheel over the suggestion list (UX35-1). Ctrl+Enter on a folded line is in
  // line-commands.spec.mjs (UX34-4). Through the npm package's page (run
  // `npm run build` in js/ first).
  const js = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "js");

  const built = existsSync(
    join(js, "config-editor", "pkg", "config_editor_bg.wasm"),
  );

  const pageUrl = (options) =>
    `${pathToFileURL(join(js, "examples", "vanilla.html")).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator(".config-editor__input").first();

  const gutter = (page) => page.locator(".config-editor__gutter").first();

  async function open(page, options) {
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on(
      "console",
      (message) => message.type() === "error" && errors.push(message.text()),
    );
    await page.goto(pageUrl({ schema: undefined, ...options }));
    await expect(page.locator("body")).toHaveAttribute("data-ready", "true", {
      timeout: 20000,
    });
    await input(page).waitFor();
    // A page that scrolls, the editor at its top.
    await page.addStyleTag({ content: "body { padding-bottom: 3000px; }" });
    await page.evaluate(() => scrollTo(0, 0));
    return errors;
  }

  const lines = (count) =>
    Array.from({ length: count }, (_, i) => `key_${i}: ${i}\n`).join("");

  const scrolled = (page) =>
    page.evaluate(() => ({
      page: Math.round(scrollY),
      text: Math.round(document.querySelector(".config-editor__input").scrollTop),
    }));

  /** A one-finger drag from `(x, y)` by `dy` pixels. */
  async function drag(page, cdp, [x, y], dy) {
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [{ x, y }],
    });
    for (let step = 1; step <= 12; step += 1) {
      await cdp.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: [{ x, y: y + (dy * step) / 12 }],
      });
      await page.waitForTimeout(16);
    }
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchEnd",
      touchPoints: [],
    });
  }

  test.skip(!built, "run `npm run build` in js/ first");

  test.describe("the line numbers scroll the text (UX34-1)", () => {
    for (const language of ["yaml", "json"]) {
      test(`${language}: the wheel over the gutter scrolls the text, then the page at its end`, async ({
        page,
      }) => {
        const value =
          language === "yaml"
            ? lines(200)
            : `{\n${Array.from({ length: 200 }, (_, i) => `  "k${i}": ${i}`).join(",\n")}\n}\n`;
        const errors = await open(page, { value, language });
        await page.setViewportSize({ width: 1000, height: 700 });
        const box = await gutter(page).boundingBox();
        await page.mouse.move(box.x + 12, box.y + 120);
        await page.mouse.wheel(0, 200);
        await expect.poll(async () => (await scrolled(page)).text).toBe(200);
        expect((await scrolled(page)).page, "the page stays").toBe(0);
        await page.mouse.wheel(0, -120);
        await expect.poll(async () => (await scrolled(page)).text).toBe(80);
        // At the text's end, as over the text: the page goes on.
        await input(page).evaluate(
          (element) => (element.scrollTop = element.scrollHeight),
        );
        const end = (await scrolled(page)).text;
        await page.mouse.wheel(0, 150);
        await expect.poll(async () => (await scrolled(page)).page).toBe(150);
        expect((await scrolled(page)).text).toBe(end);
        expect(errors).toEqual([]);
      });
    }

    test("a finger dragged over the gutter scrolls the text, not the page; at a short text, the page", async ({
      browser,
    }) => {
      const context = await browser.newContext({
        viewport: { width: 390, height: 700 },
        hasTouch: true,
        isMobile: true,
      });
      const page = await context.newPage();
      const errors = await open(page, { value: lines(200) });
      const cdp = await context.newCDPSession(page);
      const box = await gutter(page).boundingBox();
      const at = [box.x + Math.min(12, box.width / 2), box.y + 220];
      await drag(page, cdp, at, -180);
      await expect
        .poll(async () => (await scrolled(page)).text)
        .toBeGreaterThan(100);
      expect((await scrolled(page)).page, "the page stays").toBe(0);
      // Back down: the text scrolls back to its top, the page stays.
      await drag(page, cdp, at, 400);
      await expect.poll(async () => (await scrolled(page)).text).toBe(0);
      expect((await scrolled(page)).page).toBe(0);
      // A text with nowhere to scroll: the drag goes on to the page, as a
      // drag over the text does.
      await page.evaluate(() => window.editor.update({ value: "a: 1\nb: 2\n" }));
      await expect.poll(async () => (await scrolled(page)).text).toBe(0);
      const short = await gutter(page).boundingBox();
      await drag(page, cdp, [at[0], short.y + short.height / 2], -150);
      await expect
        .poll(async () => (await scrolled(page)).page)
        .toBeGreaterThan(0);
      expect(errors).toEqual([]);
      await context.close();
    });
  });
});
