import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 32 (host API), one section per finding below.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

function listen(page) {
  page.errors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page) {
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.documentsView?.destroy();
    window.module = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    window.mountPoint = () => {
      window.mounted?.destroy();
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '400px';
      document.body.prepend(element);
      return element;
    };
  });
}

const input = (page) => page.locator('[data-host] .config-editor__input:visible');

/** The caret at the end of the `nth` `marker` in the text on screen (or at `offset` past its start). */
async function caretAt(page, marker, offset = marker.length) {
  await input(page).evaluate(
    (element, [marker, offset]) => {
      const at = element.value.indexOf(marker);
      if (at < 0) throw new Error(`no ${marker} in ${element.value}`);
      element.focus();
      element.setSelectionRange(at + offset, at + offset);
    },
    [marker, offset],
  );
}

// --- HO32-2: ranges the host gives again unchanged (a ConfigEditor host
// passing all its options again with the `value` onChange gave; the README's
// ConfigDocuments `{ ...document, ...change }` list) keep where the editor
// moved them: the lock stays on its field and its value saves unchanged, in
// every language, for index, line/column and `rangeOf` ranges, after edits
// above, on the line before, and below the locked line. ------------------------

const LOCKED = {
  yaml: {
    fileName: 'm.yaml',
    text: 'name: a\nhost: h\nport: 80\n',
    path: '/host',
    locked: 'host: h',
    above: ['name: a', '\nx: 1'],
    before: ['name: a', 'bc'],
    below: ['port: 80', '\nz: 9'],
  },
  json: {
    fileName: 'm.json',
    text: '{\n  "name": "a",\n  "host": "h",\n  "port": 80\n}\n',
    path: '/host',
    locked: '"host": "h"',
    above: ['"name": "a",', '\n"x": 1,'],
    before: ['"name": "a', 'bc'],
    below: ['"port": 80', ', "z": 9'],
  },
  toml: {
    fileName: 'm.toml',
    text: 'name = "a"\nhost = "h"\nport = 80\n',
    path: '/host',
    locked: 'host = "h"',
    above: ['name = "a"', '\nx = 1'],
    before: ['name = "a', 'bc'],
    below: ['port = 80', '\nz = 9'],
  },
  hcl: {
    fileName: 'm.tf',
    text: 'name = "a"\nhost = "h"\nport = 80\n',
    path: '/host',
    locked: 'host = "h"',
    above: ['name = "a"', '\nx = 1'],
    before: ['name = "a', 'bc'],
    below: ['port = 80', '\nz = 9'],
  },
  xml: {
    fileName: 'm.xml',
    text: '<c>\n  <name>a</name>\n  <host>h</host>\n  <port>80</port>\n</c>\n',
    path: '/c/host',
    locked: '<host>h</host>',
    above: ['<name>a</name>', '\n<x>1</x>'],
    before: ['<name>a', 'bc'],
    below: ['<port>80</port>', '<z>9</z>'],
  },
};

/** Mounts `language`'s document locked by a range in `style`, its host giving every option again (`pattern`). */
async function mountLocked(page, language, style, pattern) {
  await page.evaluate(
    ([language, style, pattern, sample]) => {
      const { text, path, fileName } = sample;
      const found = tools.rangeOf(language, text, path);
      const range =
        style === 'indices'
          ? { start: found.start, end: found.end }
          : style === 'lines'
            ? { startLine: found.startLine, startColumn: found.startColumn, endLine: found.endLine, endColumn: found.endColumn }
            : found;
      window.saved = [];
      if (pattern === 'documents') {
        let files = [{ id: 'm', fileName, value: text, readOnlyRanges: [range] }];
        const update = (id, change) => {
          files = files.map((file) => (file.id === id ? { ...file, ...change } : file));
          mounted.update({ documents: files });
        };
        window.mounted = new module.ConfigDocuments(mountPoint(), {
          documents: files,
          onChange: (id, value) => update(id, { value }),
          onSave: (id, value) => saved.push(value),
        });
        window.current = () => mounted.editor('m').value;
        window.saveNow = () => mounted.save('m');
      } else {
        const element = mountPoint();
        const props = { language, value: text, readOnlyRanges: [range], onSave: (value) => saved.push(value) };
        props.onChange = (value) => {
          props.value = value;
          queueMicrotask(() => mounted.update({ ...props }));
        };
        window.mounted = new module.ConfigEditor(element, props);
        window.current = () => mounted.value;
        window.saveNow = () => mounted.save();
      }
    },
    [language, style, pattern, LOCKED[language]],
  );
}

for (const pattern of ['editor', 'documents']) {
  test(`HO32-2: ranges given again unchanged keep the lock on its field (${pattern})`, async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    test.setTimeout(240_000);
    await open(page);
    const failures = [];
    for (const [language, sample] of Object.entries(LOCKED)) {
      for (const style of ['indices', 'lines', 'rangeOf']) {
        for (const edit of ['above', 'before', 'below']) {
          const name = `${language} ${style} ${edit}`;
          await mountLocked(page, language, style, pattern);
          const [marker, typed] = sample[edit];
          await caretAt(page, marker);
          for (const part of typed.split(/(\n)/)) {
            if (part === '\n') await page.keyboard.press('Enter');
            else if (part) await page.keyboard.type(part);
          }
          await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
          const edited = await page.evaluate(() => current());
          // Typing into the locked value is refused.
          await caretAt(page, sample.locked, sample.locked.lastIndexOf('h') + 1);
          await page.keyboard.type('ACKED');
          await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
          const after = await page.evaluate(() => current());
          if (after !== edited) failures.push(`${name}: the locked value took typing: ${JSON.stringify(after)}`);
          if (!edited.includes(sample.locked)) failures.push(`${name}: the locked text changed: ${JSON.stringify(edited)}`);
          const saved = await page.evaluate(() => {
            saveNow();
            return saved.at(-1) ?? null;
          });
          if (saved !== null && (!saved.includes(sample.locked) || saved.includes('ACKED'))) {
            failures.push(`${name}: saved ${JSON.stringify(saved)}`);
          }
        }
      }
    }
    expect(failures).toEqual([]);
    expect(page.errors).toEqual([]);
  });
}

// --- HO32-1: the shipped example's "Lock line 1", then Ctrl+Home, Enter,
// ArrowUp and a comment typed: the new line is free and the locked line
// stays locked (the line break used to join the lock, and every key after
// it was refused). -------------------------------------------------------------

test('HO32-1: a line added above the shipped example\'s locked line is free', async ({ page }) => {
  test.skip(!built, 'run `npm run build` in js/ first');
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('#lock-first-line').check();
  const box = page.locator('.config-editor__input').first();
  const before = await box.inputValue();
  const first = before.split('\n')[0];
  await box.click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Enter');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.type('# added above');
  await expect(box).toHaveValue(`# added above\n${before}`);
  // The original first line is still locked.
  await box.evaluate((element, first) => {
    const at = element.value.indexOf(first) + first.length;
    element.focus();
    element.setSelectionRange(at, at);
  }, first);
  await page.keyboard.type('Z');
  await expect(box).toHaveValue(`# added above\n${before}`);
  // And the user's own line takes Backspace.
  await box.evaluate((element) => {
    element.focus();
    element.setSelectionRange(13, 13);
  });
  await page.keyboard.press('Backspace');
  await expect(box).toHaveValue(`# added abov\n${before}`);
  expect(page.errors).toEqual([]);
});

// --- HO32-3: in Edit-as, `update({ value })` with the text the host stored
// from onChange, during the typing pause, keeps what was typed (the update
// first sends the edit to onChange, then used to apply the older value), with
// and without echoes; one call, and a host rendering on a timer while the
// user types. -------------------------------------------------------------------

for (const echoes of [false, true]) {
  test(`HO32-3: a stored text given back during the Edit-as pause keeps the typing (echoes ${echoes})`, async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    await open(page);
    // One call, right after a key.
    await page.evaluate((echoes) => {
      window.store = { text: '{\n  "name": "a",\n  "port": 1\n}\n' };
      window.mounted = new module.ConfigEditor(mountPoint(), {
        language: 'json',
        editAs: 'yaml',
        value: store.text,
        echoes,
        onChange: (text) => {
          store.text = text;
        },
      });
    }, echoes);
    await caretAt(page, 'name: a');
    await page.keyboard.type('X');
    await page.evaluate(() => mounted.update({ value: store.text }));
    await expect(input(page)).toHaveValue('name: aX\nport: 1\n');
    expect(await page.evaluate(() => [mounted.value, store.text])).toEqual([
      '{\n  "name": "aX",\n  "port": 1\n}\n',
      '{\n  "name": "aX",\n  "port": 1\n}\n',
    ]);
    // A host rendering every 120 ms while the user types slowly.
    await page.evaluate((echoes) => {
      window.store = { text: '{\n  "name": "a",\n  "port": 1\n}\n' };
      window.mounted = new module.ConfigEditor(mountPoint(), {
        language: 'json',
        editAs: 'yaml',
        value: store.text,
        echoes,
        onChange: (text) => {
          store.text = text;
        },
      });
      window.timer = setInterval(() => mounted.update({ value: store.text, showFooter: true }), 120);
    }, echoes);
    await caretAt(page, 'name: a');
    for (const character of 'hello world') {
      await page.keyboard.type(character);
      await page.waitForTimeout(45);
    }
    await expect(input(page)).toHaveValue('name: ahello world\nport: 1\n');
    await expect.poll(() => page.evaluate(() => store.text)).toBe('{\n  "name": "ahello world",\n  "port": 1\n}\n');
    await page.evaluate(() => clearInterval(timer));
    expect(page.errors).toEqual([]);
  });
}

// --- HO32-5: with echoes, a store that writes each text another way (the
// byte order mark dropped, LF for CRLF) hands back echoes the editor still
// tells apart: typing in a BOM and CRLF document is never rolled back, and
// the document keeps its own form. -----------------------------------------------

for (const store of ['strip-bom', 'crlf-to-lf']) {
  test(`HO32-5: echoes written another way keep the typing (${store})`, async ({ page }) => {
    test.skip(!built, 'run `npm run build` in js/ first');
    await open(page);
    await page.evaluate((store) => {
      const rewrite =
        store === 'strip-bom' ? (text) => text.replace(/^﻿/, '') : (text) => text.replace(/\r\n/g, '\n');
      // Late, out of order: each echo after a delay that shrinks as typing goes on.
      let delay = 400;
      window.mounted = new module.ConfigEditor(mountPoint(), {
        value: '﻿a: 1\r\nb: 2\r\n',
        echoes: true,
        onChange: (text) => {
          const echo = rewrite(text);
          setTimeout(() => mounted.update({ value: echo }), delay);
          delay = Math.max(50, delay - 80);
        },
      });
    }, store);
    await caretAt(page, 'a: 1');
    for (const character of '23456') {
      await page.keyboard.type(character);
      await page.waitForTimeout(40);
    }
    await page.waitForTimeout(900);
    expect(await page.evaluate(() => mounted.value)).toBe('﻿a: 123456\r\nb: 2\r\n');
    expect(page.errors).toEqual([]);
  });
}
