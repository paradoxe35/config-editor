import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, scrollToLine, setupEditor } from './helpers.mjs';

// Host-controlled editing: the read-only switch, explicit saves (Ctrl+S),
// the dirty flag, the scroll kept on a host's value update, and the notice
// when the baseline changes under unsaved edits. The demo's toolbar holds a
// Read-only box, "Server change" (moves the baseline on) and "Host edit"
// (sets a new value); its status span reports saves, dirty and the
// conflict events.

const hint = (page) => page.locator('.config-editor__hint kbd');
const notice = (page) => page.locator('.config-editor__notice');
const status = (page) => page.locator('.demo__host-status');
const output = (page) => page.locator('.demo__output-text');
const focusedClass = (page) => page.evaluate(() => document.activeElement?.className ?? '');

// Puts the caret at the end of one-based `line` and focuses the text.
async function caretAtEnd(page, line) {
  await page.locator('.config-editor__input').evaluate((element, target) => {
    const lines = element.value.split('\n');
    const offset = lines.slice(0, target).reduce((sum, text) => sum + text.length + 1, 0) - 1;
    element.focus();
    element.setSelectionRange(offset, offset);
  }, line);
}

// Clicks a demo button without taking the focus from the editor, as a
// server push arriving while the user types would.
const pushFromServer = (page) => page.locator('.demo__server-change').evaluate((button) => button.click());
const editOnHost = (page) => page.locator('.demo__host-edit').evaluate((button) => button.click());

test.describe('a read-only editor switched to editable', () => {
  setupEditor(test, '?readonly');

  test('offers completions and the hint once editable, and stops again when read-only', async ({ page }) => {
    const e = editor(page);
    await expect(hint(page)).toHaveCount(0);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);

    await page.locator('.demo__read-only').uncheck();
    await expect(hint(page)).toHaveCount(2);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('lim');
    await expect.poll(() => e.keys()).toContain('limits');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+Space');
    await expect.poll(() => e.isOpen()).toBe(true);
    await page.keyboard.press('Escape');

    await page.locator('.demo__read-only').check();
    await expect(hint(page)).toHaveCount(0);
    await e.input.click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    await page.waitForTimeout(150);
    expect(await e.isOpen()).toBe(false);
  });
});

test.describe('saving and the dirty flag', () => {
  setupEditor(test);

  test('Ctrl+S saves the text, prevents the browser dialog, and clears dirty', async ({ page }) => {
    const e = editor(page);
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
    await page.evaluate(() => {
      window.addEventListener('keydown', (event) => {
        if (event.key === 's') window.savePrevented = event.defaultPrevented;
      });
    });
    await caretAtEnd(page, 2);
    await page.keyboard.type('0');
    await expect(status(page)).toHaveAttribute('data-dirty', 'true');
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', '1');
    expect(await page.evaluate(() => window.savePrevented)).toBe(true);
    await expect(output(page)).toContainText('listen: 78350');
    // The host took the save as its baseline: nothing is unsaved, no notice.
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
    await expect(notice(page)).toBeHidden();
    await expect(page.locator('.config-editor__diff-marker-modified')).toHaveCount(0);
    expect(await e.value()).toContain('listen: 78350');
  });

  test('undoing back to the baseline is clean again; dirty fires on changes only', async ({ page }) => {
    await caretAtEnd(page, 2);
    await page.keyboard.type('12');
    await expect(status(page)).toHaveAttribute('data-dirty', 'true');
    await page.keyboard.press('Control+z');
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
  });
});

test.describe('saving while editing as another language', () => {
  setupEditor(test, '?edit=json');

  test('Ctrl+S right after typing saves the converted text, comments kept', async ({ page }) => {
    await page.locator('.config-editor__input').evaluate((element) => {
      const at = element.value.indexOf('7835') + 4;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('1');
    // Within the typing pause: the save converts the edit first.
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', '1');
    await expect(output(page)).toContainText('# Burrow server configuration');
    await expect(output(page)).toContainText('listen: 78351');
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
    await expect(notice(page)).toBeHidden();
  });
});

test.describe('saving a value the host language cannot hold', () => {
  setupEditor(test, '?language=toml&edit=json');

  test('is refused: nothing is sent and the caret goes to the problem', async ({ page }) => {
    await page.locator('.config-editor__input').evaluate((element) => {
      const at = element.value.indexOf('7835');
      element.focus();
      element.setSelectionRange(at, at + 4);
    });
    await page.keyboard.type('null');
    await page.keyboard.press('Control+s');
    await page.waitForTimeout(300);
    await expect(status(page)).toHaveAttribute('data-saves', '0');
    await expect(page.locator('.config-editor__diagnostic-marker-shown')).toHaveCount(1);
    expect(await editor(page).caret()).toMatchObject({ line: 2 });
    // Fixed, it saves.
    await page.keyboard.press('Control+z');
    await page.keyboard.press('Control+s');
    await expect(status(page)).toHaveAttribute('data-saves', '1');
  });
});

test.describe('the baseline changing under unsaved edits', () => {
  setupEditor(test);

  test('is taken silently without unsaved edits', async ({ page }) => {
    await pushFromServer(page);
    await page.waitForTimeout(150);
    await expect(notice(page)).toBeHidden();
    await expect(status(page)).toHaveAttribute('data-conflicts', '');
    // The text is the old version now: one modified line against the new one.
    await expect(page.locator('.config-editor__diff-marker-modified')).toHaveCount(1);
  });

  test('asks, with the focus on Keep mine; Escape keeps the text and diffs it against the new baseline', async ({ page }) => {
    const e = editor(page);
    await caretAtEnd(page, 3);
    await page.keyboard.type('.org');
    await pushFromServer(page);
    await expect(notice(page)).toBeVisible();
    await expect(notice(page)).toHaveAttribute('role', 'alert');
    await expect(notice(page)).toContainText('The file changed on the server.');
    await expect.poll(() => focusedClass(page)).toContain('config-editor__notice-keep');
    await page.keyboard.press('Escape');
    await expect(notice(page)).toBeHidden();
    await expect.poll(() => focusedClass(page)).toContain('config-editor__input');
    expect(await e.value()).toContain('tunnel.example.com.org');
    expect(await e.value()).toContain('listen: 7835');
    await expect(status(page)).toHaveAttribute('data-conflicts', 'Opened,KeptMine');
    // Two changes now: the user's line, and the server's that the text lacks.
    await expect(page.locator('.config-editor__diff-marker-modified')).toHaveCount(2);
    // Typing goes on in the text.
    await page.keyboard.type('x');
    expect(await e.value()).toContain('.orgx');
  });

  test('Reload makes the new baseline the text in one undo step, sent to the host', async ({ page }) => {
    const e = editor(page);
    await caretAtEnd(page, 3);
    await page.keyboard.type('.org');
    await pushFromServer(page);
    await expect.poll(() => focusedClass(page)).toContain('config-editor__notice-keep');
    // From the keyboard: Shift+Tab to Reload, Enter.
    await page.keyboard.press('Shift+Tab');
    await expect.poll(() => focusedClass(page)).toContain('config-editor__notice-reload');
    await page.keyboard.press('Enter');
    await expect(notice(page)).toBeHidden();
    expect(await e.value()).toContain('listen: 7836');
    expect(await e.value()).not.toContain('.org');
    await expect(output(page)).toContainText('listen: 7836');
    await expect(status(page)).toHaveAttribute('data-conflicts', 'Opened,Reloaded');
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
    await expect.poll(() => focusedClass(page)).toContain('config-editor__input');
    await page.keyboard.press('Control+z');
    expect(await e.value()).toContain('tunnel.example.com.org');
    expect(await e.value()).toContain('listen: 7835');
  });

  test('the Reload button works with the mouse', async ({ page }) => {
    const e = editor(page);
    await caretAtEnd(page, 3);
    await page.keyboard.type('.org');
    await pushFromServer(page);
    await page.locator('.config-editor__notice-reload').click();
    await expect(notice(page)).toBeHidden();
    expect(await e.value()).toContain('listen: 7836');
  });

  test('typing the same change as the server closes the notice by itself', async ({ page }) => {
    await caretAtEnd(page, 3);
    await page.keyboard.type('.org');
    await pushFromServer(page);
    await expect(notice(page)).toBeVisible();
    await page.locator('.config-editor__input').evaluate((element) => element.focus());
    await page.keyboard.press('Control+z');
    await caretAtEnd(page, 2);
    await page.keyboard.press('Backspace');
    await page.keyboard.type('6');
    await expect(notice(page)).toBeHidden();
    await expect(status(page)).toHaveAttribute('data-conflicts', 'Opened,Settled');
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
  });

  test('the notice does not take the focus from outside the editor', async ({ page }) => {
    await caretAtEnd(page, 3);
    await page.keyboard.type('.org');
    await page.locator('.demo__server-change').click();
    await expect(notice(page)).toBeVisible();
    await page.waitForTimeout(100);
    expect(await focusedClass(page)).toContain('demo__server-change');
  });
});

test.describe('Reload while editing as another language', () => {
  setupEditor(test, '?edit=json');

  test('sends the new baseline exactly, comments included', async ({ page }) => {
    await page.locator('.config-editor__input').evaluate((element) => {
      const at = element.value.indexOf('tunnel.example.com') + 'tunnel.example.com'.length;
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.type('.org');
    await page.waitForTimeout(400);
    await pushFromServer(page);
    await expect(notice(page)).toBeVisible();
    await page.locator('.config-editor__notice-reload').click();
    await expect(editor(page).input).toHaveValue(/"listen": 7836/);
    await expect(output(page)).toHaveText(
      '# Burrow server configuration\nlisten: 7836\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n',
    );
    await expect(status(page)).toHaveAttribute('data-dirty', 'false');
  });
});

test.describe('a value from the host on a long document', () => {
  setupEditor(test, '?sample=large');

  for (const engine of ['chromium', 'an engine that resets the scroll on a new value (Firefox)']) {
    test(`keeps the scroll position and the caret, in ${engine}`, async ({ page }) => {
      const e = editor(page);
      if (engine !== 'chromium') {
        // Firefox puts a textarea back at the top when its value changes.
        await page.evaluate(() => {
          const property = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
          Object.defineProperty(HTMLTextAreaElement.prototype, 'value', {
            configurable: true,
            get() {
              return property.get.call(this);
            },
            set(text) {
              const changed = property.get.call(this) !== text;
              property.set.call(this, text);
              if (changed) this.scrollTop = 0;
            },
          });
        });
      }
      // The caret near the top, then the view scrolled to the middle with the
      // wheel, as a user reading on does.
      await e.input.click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowDown');
      const caret = await e.caret();
      await page.mouse.move(500, 300);
      for (let step = 0; step < 20; step += 1) await page.mouse.wheel(0, 1000);
      await page.waitForTimeout(200);
      const before = await e.input.evaluate((element) => element.scrollTop);
      expect(before).toBeGreaterThan(5000);
      const middle = await e.input.evaluate((element) => {
        const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
        return Math.round((element.scrollTop + element.clientHeight / 2) / lineHeight);
      });
      await page.locator('.demo__host-edit').click();
      await expect(output(page)).not.toContainText('listen: 7835\n');
      await page.waitForTimeout(200);
      expect(await e.input.evaluate((element) => element.scrollTop)).toBe(before);
      expect(await e.caret()).toEqual(caret);
      // The rendered rows follow the kept view.
      await expect(page.locator(`.config-editor__gutter [data-line="${middle}"]`)).toBeInViewport();
    });
  }
});

// The JS packages, through their example pages (built with `npm run build`
// in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const examplePage = (name) => {
  const [file, query] = name.split('?');
  return pathToFileURL(join(js, 'examples', file)).href + (query === undefined ? '' : `?${query}`);
};

async function open(page, name) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(examplePage(name));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  return errors;
}

for (const name of ['vanilla.html', 'react.html', 'react.html?late', 'react.html?slow']) {
  test.describe(`${name}: host-controlled editing`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('Ctrl+S saves, dirty follows, and a save never conflicts with typing on', async ({ page }) => {
      const errors = await open(page, name);
      const input = page.locator('.config-editor__input');
      await expect(page.locator('#dirty')).toHaveText('saved');
      await input.click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('limits:');
      await expect(page.locator('#dirty')).toHaveText('unsaved');
      await page.keyboard.press('Control+s');
      // Typing on while the save travels: the baseline that comes back is
      // the user's own save, not a server change.
      await page.keyboard.type(' {}');
      await page.waitForTimeout(name.includes('slow') ? 1800 : 500);
      await expect(page.locator('.config-editor__notice')).toBeHidden();
      await expect(page.locator('#dirty')).toHaveText('unsaved');
      const saved = name.startsWith('vanilla')
        ? await page.evaluate(() => window.saves.map((save) => save.text))
        : [await page.locator('#saves').textContent()];
      expect(saved.length).toBe(1);
      expect(saved[0]).toContain('limits:');
      expect(saved[0]).not.toContain('{}');
      if (name.startsWith('vanilla')) {
        expect(await page.evaluate(() => window.saves[0].language)).toBe('yaml');
      }
      expect(await input.inputValue()).toContain('limits: {}');
      expect(errors).toEqual([]);
    });

    test('the handle saves and reports dirty; a server change asks', async ({ page }) => {
      const errors = await open(page, name);
      const input = page.locator('.config-editor__input');
      const handle = name.startsWith('vanilla') ? 'window.editor' : 'window.editorHandle.current';
      expect(await page.evaluate(`${handle}.isDirty`)).toBe(false);
      await input.click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('limits:');
      expect(await page.evaluate(`${handle}.isDirty`)).toBe(true);
      await page.locator('#server-change').evaluate((button) => button.click());
      await expect(page.locator('.config-editor__notice')).toBeVisible();
      await expect.poll(() => focusedClass(page)).toContain('config-editor__notice-keep');
      await page.keyboard.press('Escape');
      await expect(page.locator('.config-editor__notice')).toBeHidden();
      if (name.startsWith('vanilla')) {
        expect(await page.evaluate(() => window.conflicts)).toEqual(['opened', 'kept-mine']);
      } else {
        await expect(page.locator('#conflicts')).toHaveText('opened,kept-mine');
      }
      expect(await page.evaluate(`${handle}.save()`)).toBe(true);
      await page.waitForTimeout(name.includes('slow') ? 1800 : 300);
      expect(await page.evaluate(`${handle}.isDirty`)).toBe(false);
      await expect(page.locator('.config-editor__notice')).toBeHidden();
      expect(errors).toEqual([]);
    });
  });
}

test.describe('vanilla: without onSave', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('Ctrl+S is left to the browser, and save() sends nothing', async ({ page }) => {
    await open(page, 'vanilla.html');
    await page.evaluate(() => {
      window.editor.setCallbacks({ onSave: null });
      window.addEventListener('keydown', (event) => {
        if (event.key === 's') window.savePrevented = event.defaultPrevented;
      });
    });
    await page.locator('.config-editor__input').click();
    await page.keyboard.press('Control+s');
    expect(await page.evaluate(() => window.savePrevented)).toBe(false);
    expect(await page.evaluate(() => window.editor.save())).toBe(false);
    expect(await page.evaluate(() => window.saves.length)).toBe(0);
  });
});
