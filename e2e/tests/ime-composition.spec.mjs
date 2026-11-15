import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// While an input method composes, its keys and its text are its own: the
// editor takes no key (Enter does not accept a suggestion over the composed
// text, Tab does not indent), and the composition is applied once, when it
// ends (each update written back mid-composition cost seconds on a long
// text, and over part of a secret placeholder landed twice). Round 6's
// `D6.a11y-1`, round 5's `D5.ux-1` and round 6's `D6.perf-1`.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

const SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    mode: { type: 'string', enum: ['fast', 'safe'] },
    metrics: { type: 'boolean' },
    a: { type: 'integer' },
  },
});

async function mount(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await page.evaluate(async (options) => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.changes = [];
    window.qa = new module.ConfigEditor(host, {
      ...options,
      onChange: (text) => window.changes.push(text),
    });
  }, options);
  await input(page).waitFor();
  return page.context().newCDPSession(page);
}

async function caretAt(page, start, end = start) {
  await input(page).evaluate(
    (element, [start, end]) => {
      element.focus();
      element.setSelectionRange(start, end);
    },
    [start, end],
  );
}

const value = (page) => page.evaluate(() => window.qa.value);

test.describe('keys during an IME composition', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const key of ['Enter', 'Tab', 'ArrowDown', 'Escape']) {
    test(`${key} is the input method's, not the editor's`, async ({ page }) => {
      const cdp = await mount(page, {
        language: 'yaml',
        value: 'a: 1\n',
        schema: SCHEMA,
      });
      await caretAt(page, 5);
      await cdp.send('Input.imeSetComposition', {
        text: 'm',
        selectionStart: 1,
        selectionEnd: 1,
      });
      await page.keyboard.press(key);
      await cdp.send('Input.insertText', { text: 'm' });
      await page.waitForTimeout(400);
      const text = await value(page);
      // No suggestion accepted over the composed text, no indent added.
      expect(text).not.toContain('mode');
      expect(text).not.toContain('metrics');
      expect(text).not.toMatch(/\n\s+m/);
      expect(text).toContain('m');
    });
  }
});

test.describe('a composition is applied once, when it ends', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('the document waits for the commit, then takes it once', async ({ page }) => {
    const cdp = await mount(page, { language: 'yaml', value: 'name: x\n' });
    await caretAt(page, 7);
    for (const text of ['n', 'ni', 'に', 'にほ']) {
      await cdp.send('Input.imeSetComposition', {
        text,
        selectionStart: text.length,
        selectionEnd: text.length,
      });
    }
    await page.waitForTimeout(300);
    expect(await value(page)).toBe('name: x\n');
    await cdp.send('Input.insertText', { text: '日本' });
    await expect.poll(() => value(page)).toBe('name: x日本\n');
    // One undo step takes the whole composition back.
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe('name: x\n');
  });

  test('a composition over part of a secret placeholder replaces it once', async ({ page }) => {
    const text = 'name: edge\nkey: «secret 3f2a»\n';
    const cdp = await mount(page, {
      language: 'yaml',
      value: text,
      schema: JSON.stringify({
        type: 'object',
        properties: { key: { type: 'string', writeOnly: true } },
      }),
    });
    const at = text.indexOf('«secret');
    await caretAt(page, at + 2, at + 8);
    await cdp.send('Input.imeSetComposition', {
      text: 'ü',
      selectionStart: 1,
      selectionEnd: 1,
    });
    await cdp.send('Input.insertText', { text: 'ü' });
    await expect.poll(() => value(page)).toBe('name: edge\nkey: ü\n');
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe(text);
  });
});

// P4: a host value arriving while the user composes rewrites the textarea
// under the composition; Chrome then ends it without a `compositionend`,
// and the commit arrives as plain typing. It lands on the host's new text,
// and so does what is typed after it (not left in the textarea only, with
// the editor waiting for an end that never comes).
test.describe('a host value during a composition', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  const cases = [
    { name: 'replaceValue', options: {}, set: 'window.qa.replaceValue("b: 2\\n")', yaml: true },
    { name: 'a value prop', options: {}, set: 'window.qa.update({ value: "b: 2\\n" })', yaml: true },
    { name: 'replaceValue, edited as XML', options: { editAs: 'xml' }, set: 'window.qa.replaceValue("b: 2\\n")', yaml: false },
  ];
  for (const { name, options, set, yaml } of cases) {
    test(`${name}: the commit and the typing after it land on the new text`, async ({ page }) => {
      const cdp = await mount(page, { language: 'yaml', value: 'a: 1\n', ...options });
      const text = () => page.evaluate(() => window.qa.text);
      const textarea = () => input(page).inputValue();
      const before = await text();
      await caretAt(page, before.length);
      // In this order: the composition starts, the host's value comes, the
      // composition is committed.
      await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
      await page.evaluate(set);
      const replaced = await text();
      expect(replaced).not.toBe(before);
      await cdp.send('Input.insertText', { text: '日本' });
      await expect.poll(text).toBe(`${replaced}日本`);
      expect(await textarea()).toBe(await text());
      await page.keyboard.type('z');
      await expect.poll(text).toBe(`${replaced}日本z`);
      expect(await textarea()).toBe(await text());
      if (yaml) await expect.poll(() => value(page)).toBe('b: 2\n日本z');
    });
  }
});
