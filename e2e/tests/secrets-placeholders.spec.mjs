import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Secret placeholders: tokens pasted or dragged into other fields, many unknown ones, copies in XML.

/** Puts the caret at UTF-16 offset `at`. */
async function caretTo(page, at) {
  await editor(page).input.evaluate((element, at) => {
    element.focus();
    element.setSelectionRange(at, at);
  }, at);
}

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

/** Mounts a fresh ConfigEditor with `options` on the vanilla page. */
async function open(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const parent = document.querySelector('.config-editor').parentElement;
    window.editor.destroy();
    window.saved = null;
    window.problems = [];
    window.editor = new ConfigEditor(parent, {
      ...options,
      onSave: (text) => (window.saved = text),
      onProblems: (problems) => (window.problems = problems),
    });
  }, options);
  await page.locator('.config-editor__input').waitFor();
}

const input = (page) => page.locator('.config-editor__input');

/** Puts the caret at the start of the `nth` match of `needle` on screen. */
async function caretAt(page, needle, nth = 0) {
  await input(page).evaluate((element, [needle, nth]) => {
    let at = -1;
    for (let index = 0; index <= nth; index++) at = element.value.indexOf(needle, at + 1);
    if (at < 0) throw new Error(`no ${needle}`);
    element.focus();
    element.setSelectionRange(at, at);
  }, [needle, nth]);
}

async function press(page, ...keys) {
  for (const key of keys) await page.keyboard.press(key);
}

async function editAs(page, language) {
  await page.evaluate((language) => window.editor.update({ editAs: language }), language);
  await page.waitForTimeout(200);
}

test.describe('secret placeholders in another field', () => {
  setupEditor(test, '?schema=protected');

  test("the key's token pasted into the pin is plain text and flagged", async ({ page }) => {
    const KEY = '«secret 3f2a»';
    const PIN = '«secret 0b1c»';
    const text = await editor(page).value();
    const pin = text.indexOf(PIN);
    // Select the pin's token whole and put the key's over it, as a paste.
    await editor(page).input.evaluate((element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    }, [pin, pin + PIN.length]);
    await page.keyboard.insertText(KEY);
    await page.waitForTimeout(300);
    expect(await editor(page).value()).toContain(`pin: ${KEY}`);
    const chips = page.locator('.config-editor__code-line .config-editor__secret');
    await expect(chips).toHaveText([KEY]);
    await expect(
      page.locator('.config-editor__diagnostic-marker-warning[data-message*="stands for the secret of /tls/key"]'),
    ).toHaveCount(1);
  });

  test('a drag starting on a chip takes it whole, either way', async ({ page }) => {
    const KEY = '«secret 3f2a»';
    const start = (await editor(page).value()).indexOf(KEY);
    const end = start + KEY.length;
    // Pressed in the middle of the chip as drawn, as a user aims at it.
    const chip = await page.locator('.config-editor__code-line .config-editor__secret').first().boundingBox();
    const y = chip.y + chip.height / 2;
    for (const [x, check] of [
      [chip.x - 25, ([from, until]) => from < start && until === end],
      [chip.x + chip.width + 25, ([from, until]) => from === start && until >= end],
    ]) {
      await caretTo(page, 0);
      await page.mouse.move(chip.x + chip.width / 2, y);
      await page.mouse.down();
      await page.mouse.move(x, y, { steps: 5 });
      await page.mouse.up();
      await page.waitForTimeout(100);
      const selection = await editor(page).input.evaluate((element) => [element.selectionStart, element.selectionEnd]);
      expect(check(selection), `drag to x ${x}: selection ${selection} around the chip ${start}..${end}`).toBe(true);
    }
  });
});

test.describe('thousands of unknown secret placeholders', () => {
  setupEditor(test, '?schema=protected&language=yaml');

  test('show as many warnings as the cap, and the footer says more follow', async ({ page }) => {
    const e = editor(page);
    const token = (index) => `«secret ${index.toString(16).padStart(4, '0')}»`;
    const clients = Array.from(
      { length: 600 },
      (_, index) => `  c${index}:\n    secret: "${token(index)}"\n`,
    ).join('');
    await e.input.evaluate((element, text) => {
      element.focus();
      const at = element.value.length;
      element.setRangeText(text, at, at, 'end');
      element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: text }));
    }, `${(await e.value()).endsWith('\n') ? '' : '\n'}clients:\n${clients}`);
    const status = page.locator('.config-editor__diagnostic-status');
    await expect(status).toContainText('20+ warnings');
    await expect(status).not.toContainText('warnings+');
    const shown = await page.locator('.config-editor__diagnostic-marker').count();
    expect(shown, 'underlined problems').toBeLessThanOrEqual(60);
  });
});

test.describe('editing', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  const marked = {
    yaml: '# head\nname: a\nport: 1\n',
    json: '{\n  "name": "a",\n  "port": 1\n}\n',
    toml: '# head\nname = "a"\nport = 1\n',
    hcl: '# head\nname = "a"\nport = 1\n',
    xml: '<?xml version="1.0"?>\n<c>\n  <name>a</name>\n</c>\n',
  };
  const firstLineEdits = {
    'Shift+Down Delete': (page) => press(page, 'Shift+ArrowDown', 'Delete'),
    'Ctrl+X': (page) => press(page, 'Control+x'),
    'Ctrl+A and a paste': async (page) => {
      await press(page, 'Control+a');
      await page.keyboard.insertText('x: 1\n');
    },
    'Ctrl+Shift+K': (page) => press(page, 'Control+Shift+K'),
    'typing at the start': (page) => page.keyboard.type('x'),
    'Ctrl+A Tab': (page) => press(page, 'Control+a', 'Tab'),
    'Tab on the line': (page) => press(page, 'Shift+End', 'Tab'),
    'Ctrl+]': (page) => press(page, 'Control+BracketRight'),
    'Alt+Down': (page) => press(page, 'Alt+ArrowDown'),
    'Shift+Alt+Up': (page) => press(page, 'Shift+Alt+ArrowUp'),
  };
  // The copy is flagged, never the original (as in every language).
  test('a secret element copied in an XML view is flagged as a copy and the save waits', async ({ page }) => {
    const token = '«secret 3f2a»';
    const schema = JSON.stringify({ type: 'object', properties: { db: { type: 'object', properties: { password: { type: 'string', writeOnly: true } } } } });
    await open(page, { language: 'toml', value: `[db]\nhost = "h"\npassword = "${token}"\nname = "app"\n`, schema });
    await editAs(page, 'xml');
    await caretAt(page, token);
    await press(page, 'Shift+Alt+ArrowDown');
    await page.waitForTimeout(800);
    const problems = await page.evaluate(() => window.problems.map((problem) => problem.message));
    // The copy is flagged as a copy (once); the host's own line is not.
    const copies = problems.filter((message) => message.includes('this placeholder is in this field already'));
    expect(copies).toEqual(['/db/password: this placeholder is in this field already; another copy of it is plain text']);
    expect(problems.some((message) => message.startsWith('/db/password/0'))).toBe(false);
    expect(await page.evaluate(() => window.editor.save())).toBe(false);
    expect(await page.evaluate(() => window.saved)).toBe(null);
  });
});
