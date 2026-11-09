import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 1000, height: 700 } });

const input = (page) => page.locator('.config-editor__input').first();
const announcement = (page) => page.locator('.config-editor__announcement').first();
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

test.describe('L25-A3, A6: the focused hover card', () => {
  const schema = JSON.stringify({
    type: 'object',
    properties: { mode: { enum: ['a', 'b'], description: 'How it runs.', 'x-docs-url': 'https://example.com/mode' } },
  });

  for (const key of ['Enter', 'Alt+Enter']) {
    test(`${key} on the focused card opens its docs`, async ({ page, context }) => {
      const errors = await open(page, { value: 'mode: a\n', schema });
      await context.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
      await input(page).focus();
      await page.keyboard.press('Control+Home');
      await chord(page);
      await chord(page);
      await expect(card(page)).toBeFocused();
      const popup = context.waitForEvent('page', { timeout: 3000 });
      await page.keyboard.press(key);
      expect((await popup).url()).toBe('https://example.com/mode');
      expect(errors).toEqual([]);
    });
  }

  test('the focused card is a dialog named after its key; shown on hover, a tooltip', async ({ page }) => {
    const errors = await open(page, { value: 'mode: a\n', schema });
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await chord(page);
    await expect(card(page)).toHaveAttribute('role', 'tooltip');
    await chord(page);
    await expect(card(page)).toBeFocused();
    await expect(card(page)).toHaveAttribute('role', 'dialog');
    const name = await card(page).getAttribute('aria-label');
    expect(name).toContain('mode');
    expect(name.length).toBeLessThan(40);
    // Tab goes on to its link.
    await page.keyboard.press('Tab');
    await expect(page.locator('.config-editor__hover-link')).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(input(page)).toBeFocused();
    expect(errors).toEqual([]);
  });
});

test.describe('L25-A4: change and fold commands are announced', () => {
  test('Alt+F5 and Shift+Alt+F5 say which change, where', async ({ page }) => {
    const value = 'name: a\nport: 2\nmode: fast\nextra: 1\n';
    const errors = await open(page, { value, baseline: 'name: a\nport: 1\nmode: fast\n' });
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Alt+F5');
    await expect(announcement(page)).toHaveText(/Modified line, line 2.*1 of 2/);
    await page.keyboard.press('Alt+F5');
    await expect(announcement(page)).toHaveText(/Added line, line 4.*2 of 2/);
    await page.keyboard.press('Shift+Alt+F5');
    await expect(announcement(page)).toHaveText(/Modified line, line 2.*1 of 2/);
    expect(errors).toEqual([]);
  });

  test('folding and unfolding say which lines', async ({ page }) => {
    const value = 'a: 1\nb:\n  c: 2\n  d: 3\ne:\n  f: 4\n';
    const errors = await open(page, { value });
    await input(page).focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Control+Shift+BracketLeft');
    await expect(announcement(page)).toHaveText(/Folded lines 2–4/);
    await page.keyboard.press('Control+Shift+BracketRight');
    await expect(announcement(page)).toHaveText(/Unfolded lines 2–4/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+0');
    await expect(announcement(page)).toHaveText(/Folded 2 ranges/);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(announcement(page)).toHaveText(/Unfolded 2 ranges/);
    // A gutter chevron too.
    await page.locator('.config-editor__fold-toggle').first().click();
    await expect(announcement(page)).toHaveText(/Folded lines 2–4/);
    expect(errors).toEqual([]);
  });
});

test.describe('L25-A5: typing in a read-only editor', () => {
  for (const [what, act] of [
    ['a character', (page) => page.keyboard.type('x')],
    ['Backspace', (page) => page.keyboard.press('Backspace')],
    ['Enter', (page) => page.keyboard.press('Enter')],
  ]) {
    test(`${what}: says it cannot edit, at the caret, and changes nothing`, async ({ page }) => {
      const value = 'name: a\nport: 1\n';
      const errors = await open(page, { value, readOnly: true });
      await input(page).focus();
      await page.keyboard.press('Control+End');
      await act(page);
      await expect(announcement(page)).toHaveText(/read-only/);
      const notice = page.locator('.config-editor__read-only-notice');
      await expect(notice).toBeVisible();
      await expect(notice).toHaveText(/read-only/);
      expect(await input(page).inputValue()).toBe(value);
      // A move of the caret puts it away.
      await page.keyboard.press('ArrowUp');
      await expect(notice).toBeHidden();
      expect(errors).toEqual([]);
    });
  }
});

test.describe('U25-6: the problems tip takes the focus as the docs card does', () => {
  const many = Array.from({ length: 25 }, (_, i) => ({
    path: '/z',
    severity: 'warning',
    message: `host problem ${i} ${'words '.repeat(12)}`,
    ...(i === 0 ? { docsUrl: 'https://example.com/z' } : {}),
  }));
  for (const [language, value, down] of [
    ['yaml', 'a: 1\nz: 1\n', 1],
    ['json', '{\n  "a": 1,\n  "z": 1\n}\n', 2],
  ]) {
    for (const how of ['Ctrl+K Ctrl+I', 'F8']) {
      test(`${language}, shown by ${how}: a second Ctrl+K Ctrl+I focuses it; keys scroll it, Enter opens its docs, Escape returns`, async ({ page, context }) => {
        const errors = await open(page, { language, value, schema: '{}' });
        await context.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
        await page.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), many);
        const tip = page.locator('.config-editor__problem-tip');
        await input(page).focus();
        await page.keyboard.press('Control+Home');
        if (how === 'F8') {
          await page.keyboard.press('F8');
        } else {
          for (let i = 0; i < down; i += 1) await page.keyboard.press('ArrowDown');
          await page.keyboard.press('End');
          await chord(page);
        }
        await expect(tip).toBeVisible();
        await expect(tip).toHaveAttribute('role', 'tooltip');
        await chord(page);
        await expect(tip).toBeFocused();
        await expect(tip).toHaveAttribute('role', 'dialog');
        expect((await tip.getAttribute('aria-label')).length).toBeGreaterThan(0);
        expect(await tip.evaluate((element) => element.scrollHeight > element.clientHeight)).toBe(true);
        await page.keyboard.press('Home');
        await pressStill(page, ['PageDown']);
        expect(await tip.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
        await page.keyboard.press('Home');
        expect(await tip.evaluate((element) => element.scrollTop)).toBe(0);
        const popup = context.waitForEvent('page', { timeout: 3000 });
        await page.keyboard.press('Enter');
        expect((await popup).url()).toBe('https://example.com/z');
        await page.bringToFront();
        await page.keyboard.press('Escape');
        await expect(input(page)).toBeFocused();
        await expect(tip).toBeHidden();
        expect(errors).toEqual([]);
      });
    }
  }
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
