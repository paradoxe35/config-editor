import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, FRAMEWORK_PAGES, pointAt, setupEditor } from './helpers.mjs';

// Secret placeholders against the protected-fields fixture
// (`?schema=protected`): `tls.key` (writeOnly, minLength and pattern) and
// `tls.pin` (x-secret integer) hold the host's tokens `«secret 3f2a»` and
// `«secret 0b1c»`.

const KEY = '«secret 3f2a»';
const PIN = '«secret 0b1c»';
const saved = (page) => page.locator('.demo__output-text');
const chips = (page) => page.locator('.config-editor__code-line .config-editor__secret');
const selection = (page) =>
  page.locator('.config-editor__input').evaluate((element) => [element.selectionStart, element.selectionEnd]);

// UTF-16 offsets of `text` in the document.
async function offsetOf(page, text) {
  const value = await editor(page).value();
  const at = value.indexOf(text);
  expect(at, `"${text}" is in the document`).toBeGreaterThanOrEqual(0);
  return at;
}

async function clickAt(page, text, offset) {
  const lines = (await editor(page).value()).split('\n');
  const line = lines.findIndex((candidate) => candidate.includes(text));
  const at = await pointAt(page, line, lines[line].indexOf(text) + offset);
  await page.mouse.click(at.x, at.y);
}

test.describe('secret placeholders', () => {
  setupEditor(test, '?schema=protected');

  test('each placeholder is a chip with a key in the gutter, and hover explains it', async ({ page }) => {
    await expect(chips(page)).toHaveText([KEY, PIN]);
    const keyLine = (await editor(page).value()).split('\n').findIndex((line) => line.includes(KEY)) + 1;
    await expect(page.locator(`[data-line="${keyLine}"] .config-editor__secret-marker`)).toHaveAttribute(
      'title',
      /Keeps the current secret unless you replace it/,
    );
    await clickAt(page, KEY, 0);
    const lines = (await editor(page).value()).split('\n');
    const at = await pointAt(page, keyLine - 1, lines[keyLine - 1].indexOf(KEY) + 4);
    await page.mouse.move(at.x, at.y, { steps: 4 });
    await expect(page.locator('.config-editor__protect-tip')).toHaveText('Keeps the current secret unless you replace it');
  });

  test('the caret never lands inside: clicks, arrows and Shift+arrows step over it whole', async ({ page }) => {
    const start = await offsetOf(page, KEY);
    const end = start + KEY.length;
    await clickAt(page, KEY, 3);
    expect([[start, start], [end, end]]).toContainEqual(await selection(page));
    await clickAt(page, KEY, 10);
    expect(await selection(page)).toEqual([end, end]);
    await page.keyboard.press('ArrowLeft');
    expect(await selection(page)).toEqual([start, start]);
    await page.keyboard.press('ArrowRight');
    expect(await selection(page)).toEqual([end, end]);
    await page.keyboard.press('Shift+ArrowLeft');
    expect(await selection(page)).toEqual([start, end]);
    // A drag ending inside takes the token whole.
    const lines = (await editor(page).value()).split('\n');
    const line = lines.findIndex((candidate) => candidate.includes(KEY));
    const from = await pointAt(page, line, 2);
    const to = await pointAt(page, line, lines[line].indexOf(KEY) + 5);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 5 });
    await page.mouse.up();
    expect((await selection(page))[1]).toBe(end);
  });

  test('Backspace and Delete remove the whole token; undo brings it back as a placeholder', async ({ page }) => {
    const ed = editor(page);
    await clickAt(page, KEY, KEY.length);
    await page.keyboard.press('Backspace');
    await expect.poll(ed.value).not.toContain('«secret 3f2a');
    expect(await ed.value()).toContain('  key: \n');
    await expect(chips(page)).toHaveText([PIN]);
    await page.keyboard.press('Control+Z');
    await expect.poll(ed.value).toContain(`key: ${KEY}`);
    await expect(chips(page)).toHaveText([KEY, PIN]);
    await clickAt(page, PIN, 0);
    await page.keyboard.press('Delete');
    await expect.poll(ed.value).toContain('  pin: \n');
    expect(await ed.value()).not.toContain('secret 0b1c');
  });

  test('typing over a selection that cuts the token replaces the whole token', async ({ page }) => {
    const ed = editor(page);
    const start = await offsetOf(page, KEY);
    await ed.input.evaluate((element, [from, to]) => {
      element.focus();
      element.setSelectionRange(from, to);
    }, [start - 2, start + 4]);
    await page.keyboard.type('Z');
    await expect.poll(ed.value).toContain('  keyZ\n');
    expect(await ed.value()).not.toContain('secret 3f2a');
  });

  test('copying keeps the token text; a pasted copy stays plain text', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const ed = editor(page);
    const start = await offsetOf(page, KEY);
    await ed.input.evaluate((element, [from, to]) => {
      element.focus();
      element.setSelectionRange(from, to);
    }, [start + 3, start + 5]);
    await page.keyboard.press('Control+C');
    await page.keyboard.press('Control+End');
    await page.keyboard.type('name2: ');
    await page.keyboard.press('Control+V');
    // The copy took the whole token, as it is.
    await expect.poll(ed.value).toContain(`name2: ${KEY}`);
    // It is not a placeholder: no chip, and the caret can enter it.
    await expect(chips(page)).toHaveCount(2);
    await page.keyboard.press('ArrowLeft');
    const value = await ed.value();
    expect(await selection(page)).toEqual([value.length - 1, value.length - 1]);
  });

  test('a placeholder pasted from the clipboard stays whole: over another, into an empty value, over a cut', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const ed = editor(page);
    const original = await ed.value();
    const pin = original.indexOf(PIN);
    const select = (from, to) =>
      ed.input.evaluate((element, [from, to]) => {
        element.focus();
        element.setSelectionRange(from, to);
      }, [from, to]);
    const pasteKey = async () => {
      await page.evaluate((text) => navigator.clipboard.writeText(text), KEY);
      await page.keyboard.press('Control+V');
    };
    // Over the pin's token selected whole (they share `«secret `).
    await select(pin, pin + PIN.length);
    await pasteKey();
    await expect.poll(ed.value).toBe(original.slice(0, pin) + KEY + original.slice(pin + PIN.length));
    await page.keyboard.press('Control+Z');
    await expect.poll(ed.value).toBe(original);
    // Into the value emptied first.
    await select(pin, pin + PIN.length);
    await page.keyboard.press('Backspace');
    await pasteKey();
    await expect.poll(ed.value).toBe(original.slice(0, pin) + KEY + original.slice(pin + PIN.length));
    await page.keyboard.press('Control+Z');
    await page.keyboard.press('Control+Z');
    await expect.poll(ed.value).toBe(original);
    // Over a selection from before the token into it: the token goes whole.
    await select(pin - 1, pin + 5);
    await pasteKey();
    await expect.poll(ed.value).toBe(original.slice(0, pin - 1) + KEY + original.slice(pin + PIN.length));
  });

  test('an insertFromPaste input without a beforeinput keeps the pasted placeholder whole', async ({ page }) => {
    const ed = editor(page);
    const original = await ed.value();
    const pin = original.indexOf(PIN);
    // What a synthetic paste does: the textarea's text changed, one input
    // event, no beforeinput telling what it replaced.
    const synthetic = (from, to) =>
      ed.input.evaluate(
        (element, [from, to, text]) => {
          element.focus();
          element.value = element.value.slice(0, from) + text + element.value.slice(to);
          element.setSelectionRange(from + text.length, from + text.length);
          element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: text }));
        },
        [from, to, KEY],
      );
    await synthetic(pin, pin + PIN.length);
    await expect.poll(ed.value).toBe(original.slice(0, pin) + KEY + original.slice(pin + PIN.length));
  });

  test('an untouched placeholder passes the schema check; a look-alike in a secret field is flagged', async ({ page }) => {
    const ed = editor(page);
    await page.waitForTimeout(400);
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(0);
    // Typed, the same-looking token is not the host's.
    await clickAt(page, PIN, PIN.length);
    await page.keyboard.press('Backspace');
    await page.keyboard.type('«secret beef»');
    await page.waitForTimeout(400);
    await expect(page.locator('.config-editor__diagnostic-marker')).not.toHaveCount(0);
    // One problem says so (its message also shows on the underline of a
    // problem at the same place, whose message lists both).
    await expect(
      page.locator('.config-editor__diagnostic-marker[aria-label*="unknown secret placeholder"]'),
    ).toHaveCount(1);
    // Problems with a secret field never quote what was typed there.
    const messages = await page
      .locator('.config-editor__diagnostic-marker')
      .evaluateAll((markers) => markers.map((marker) => marker.dataset.message));
    expect(messages.join('\n')).not.toContain('beef');
    expect(await ed.value()).toContain('pin: «secret beef»');
  });

  test('a secret field suggests ${VAR} and file: references', async ({ page }) => {
    const ed = editor(page);
    await clickAt(page, KEY, KEY.length);
    await page.keyboard.press('Backspace');
    await page.keyboard.press('Control+Space');
    await expect.poll(ed.keys).toEqual(['${VAR}', 'file:path']);
    await page.keyboard.press('Enter');
    await expect.poll(ed.value).toContain('key: "${}"');
    await page.keyboard.type('TLS_KEY');
    await expect.poll(ed.value).toContain('key: "${TLS_KEY}"');
    await expect(saved(page)).toContainText('key: "${TLS_KEY}"');
  });

  test('replace all with every match in a placeholder replaces nothing and says why', async ({ page }) => {
    const before = await editor(page).value();
    await editor(page).input.click();
    await page.keyboard.press('Control+H');
    const widget = page.locator('.config-editor__find');
    await widget.getByRole('textbox', { name: 'Find' }).fill('secret 3f2a');
    await widget.getByRole('textbox', { name: 'Replace' }).fill('x');
    await widget.getByRole('button', { name: 'Replace all' }).click();
    await expect(widget).toContainText('Every match is in read-only text or secret placeholders; nothing was replaced');
    expect(await editor(page).value()).toBe(before);
  });

  test('saving keeps the placeholders byte-for-byte', async ({ page }) => {
    await clickAt(page, 'edge', 4);
    await page.keyboard.type('2');
    await expect(saved(page)).toContainText('name: edge2');
    await expect(saved(page)).toContainText(`key: ${KEY}`);
    await expect(saved(page)).toContainText(`pin: ${PIN}`);
  });
});

for (const language of ['json', 'toml', 'hcl', 'xml']) {
  test.describe(`secret placeholders in ${language}`, () => {
    setupEditor(test, `?schema=protected&language=${language}`);

    test('a chip that Backspace removes whole', async ({ page }) => {
      const ed = editor(page);
      await expect(chips(page)).toHaveText([KEY, PIN]);
      await clickAt(page, KEY, KEY.length);
      await page.keyboard.press('Backspace');
      await expect.poll(ed.value).not.toContain('secret 3f2a');
      // The other token is untouched, and no piece of this one is left.
      expect(await ed.value()).toContain(PIN);
      expect((await ed.value()).split('«').length - 1).toBe(1);
    });
  });
}

test.describe('secret placeholders while editing as another language', () => {
  setupEditor(test, '?schema=protected&edit=toml');

  test('the placeholders stay atomic and are saved back unchanged', async ({ page }) => {
    const ed = editor(page);
    await expect.poll(ed.value).toContain(`key = "${KEY}"`);
    await expect(chips(page)).toHaveText([KEY, PIN]);
    await clickAt(page, KEY, 5);
    await page.keyboard.type('x');
    // Typing at the token's edge (the caret snapped there) leaves it whole.
    expect(await ed.value()).toContain(KEY);
    await clickAt(page, 'edge', 4);
    await page.keyboard.type('3');
    await expect(saved(page)).toContainText('name: edge3');
    await expect(saved(page)).toContainText(`pin: ${PIN}`);
  });
});

// The npm packages' example pages: a host value with a placeholder, and a
// custom hint (`secrets: { hint }`).
const examples = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(examples, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

for (const name of ['vanilla.html', ...FRAMEWORK_PAGES]) {
  test.describe(`secret placeholders in ${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('a host placeholder is atomic, with the custom hint', async ({ page }) => {
      await page.goto(pathToFileURL(join(examples, 'examples', name)).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const input = page.locator('.config-editor__input');
      await page.locator('#load-secret').click();
      await expect(input).toHaveValue(/secret: «secret 3f2a»/);
      await expect(page.locator('.config-editor__secret-marker')).toHaveAttribute('title', /Keeps the stored secret/);
      await clickAt(page, KEY, KEY.length);
      await page.keyboard.press('Backspace');
      await expect(input).toHaveValue(/secret: \n/);
      await expect(page.locator('#saved')).not.toContainText('secret 3f2a');
    });

    test('a placeholder pasted back stays whole, from the clipboard or a bare insertFromPaste', async ({ page, context }) => {
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);
      await page.goto(pathToFileURL(join(examples, 'examples', name)).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const input = page.locator('.config-editor__input');
      await page.locator('#load-secret').click();
      await expect(input).toHaveValue(/secret: «secret 3f2a»/);
      const original = await input.inputValue();
      const at = original.indexOf(KEY);
      // Emptied, then pasted back from the clipboard: whole, and real again.
      await clickAt(page, KEY, KEY.length);
      await page.keyboard.press('Backspace');
      await expect(input).toHaveValue(/secret: \n/);
      await page.evaluate((text) => navigator.clipboard.writeText(text), KEY);
      await page.keyboard.press('Control+V');
      await expect(input).toHaveValue(original);
      await expect(page.locator('.config-editor__code-line .config-editor__secret')).toHaveCount(1);
      // Over a selection cutting into it, with no beforeinput.
      await input.evaluate(
        (element, [from, to, text]) => {
          element.focus();
          element.value = element.value.slice(0, from) + text + element.value.slice(to);
          element.setSelectionRange(from + text.length, from + text.length);
          element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: text }));
        },
        [at - 1, at + 5, KEY],
      );
      await expect(input).toHaveValue(original.slice(0, at - 1) + KEY + original.slice(at + KEY.length));
    });
  });
}

// Folding: the textarea holds the text on screen, folded lines cut out.
const foldRow = (page, line) => page.locator(`.config-editor__gutter-content > div[data-line="${line}"]`);
const foldToggle = (page, line) => page.locator(`.config-editor__fold-toggle[data-line="${line}"]`);
async function fold(page, line) {
  await foldRow(page, line).hover();
  await foldToggle(page, line).click();
  await expect(foldToggle(page, line)).toHaveClass(/config-editor__fold-toggle-closed/);
}

for (const [query, tlsLine] of [['?schema=protected', 7], ['?schema=protected&edit=json', 9]]) {
  test.describe(`secret placeholders in a fold (${query})`, () => {
    setupEditor(test, query);

    test('a folded placeholder shows on its fold, and stays atomic after unfolding', async ({ page }) => {
      const ed = editor(page);
      await expect(chips(page)).toHaveCount(2);
      await fold(page, tlsLine);
      // Hidden: no chip drawn, the key shows on the fold's row.
      await expect(chips(page)).toHaveCount(0);
      await expect(page.locator(`[data-line="${tlsLine}"] .config-editor__secret-marker`)).toHaveCount(1);
      expect(await ed.value()).not.toContain('«');
      await page.locator(`.config-editor__fold-chip[data-line="${tlsLine}"]`).click();
      await expect(chips(page)).toHaveText([KEY, PIN]);
      // After unfolding, a click inside still snaps to an edge.
      const start = await offsetOf(page, KEY);
      await clickAt(page, KEY, 4);
      expect([[start, start], [start + KEY.length, start + KEY.length]]).toContainEqual(await selection(page));
      await clickAt(page, KEY, KEY.length);
      await page.keyboard.press('Backspace');
      await expect.poll(ed.value).not.toContain('secret 3f2a');
      expect(await ed.value()).toContain(PIN);
    });
  });
}
