import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 28, accessibility: Escape closes the hover card or the problem
// tip with the pointer on what it shows for while the focus is on a
// documents view's tabs or "+" (controls with no Escape of their own); the
// line commands (Alt+↑/↓ move lines, Shift+Alt+↑/↓ copy them,
// Ctrl+Shift+K deletes them, Ctrl+] / Ctrl+[ indent / outdent them) work,
// and next to a lock or in a read-only editor say why they changed
// nothing, as typing does. Through the npm package's pages (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (options, name = 'vanilla') =>
  `${pathToFileURL(join(js, 'examples', `${name}.html`)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

test.skip(!built, 'run `npm run build` in js/ first');
test.use({ viewport: { width: 1000, height: 700 } });

const input = (page) => page.locator('.config-editor__input').first();
const said = (page) =>
  input(page).evaluate((element) => element.closest('.config-editor').querySelector('.config-editor__announcement').textContent);

async function open(page, options, name) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(pageUrl(options, name));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

// The middle of the first drawn `word`.
const wordAt = (page, word) =>
  page.evaluate((word) => {
    const rows = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')];
    const row = rows.find((row) => row.textContent.includes(word));
    let left = row.textContent.indexOf(word) + 1;
    const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode()) && left >= node.length) left -= node.length;
    const range = document.createRange();
    range.setStart(node, left);
    range.setEnd(node, left + 1);
    const box = range.getBoundingClientRect();
    return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  }, word);

// L28-A1: with the focus on a documents view's tab or "+", Escape left the
// popup under the pointer open (it acted only with the focus nowhere).
test.describe('L28-A1', () => {
  const schema = JSON.stringify({
    type: 'object',
    properties: { listen: { type: 'integer', description: 'Listen docs' }, port: { type: 'integer' } },
  });
  for (const [what, focus] of [
    ['the selected tab', (page) => page.locator('[role="tab"][aria-selected="true"]').focus()],
    ['the "+" button', (page) => page.locator('.config-editor-documents__add').focus()],
  ]) {
    test(`Escape closes the hover card and the problem tip with the focus on ${what}`, async ({ page }) => {
      const errors = await open(page, undefined, 'documents');
      await page.evaluate((schema) => {
        window.documentsView.update({
          documents: [
            { id: 'a', fileName: 'service.yaml', value: 'a: 1\nlisten: 7835\nport: x\n', schema },
            { id: 'b', fileName: 'other.yaml', value: 'c: 3\n' },
          ],
        });
      }, schema);
      for (const [word, popup] of [
        ['listen', '.config-editor__hover'],
        ['x', '.config-editor__problem-tip'],
      ]) {
        await page.mouse.move(990, 690);
        await focus(page);
        const at = await wordAt(page, word === 'x' ? 'port: x' : word);
        await page.mouse.move(word === 'x' ? at.x + 40 : at.x, at.y, { steps: 3 });
        await expect(page.locator(popup)).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(page.locator(popup)).toBeHidden();
      }
      expect(errors).toEqual([]);
    });
  }

  test('a field on the page keeps its Escape', async ({ page }) => {
    const errors = await open(page, undefined, 'documents');
    await page.evaluate((schema) => {
      window.documentsView.update({ documents: [{ id: 'a', fileName: 'service.yaml', value: 'listen: 7835\n', schema }] });
      const field = document.createElement('input');
      field.className = 'page-field';
      document.body.prepend(field);
    }, schema);
    await page.locator('.page-field').focus();
    const at = await wordAt(page, 'listen');
    await page.mouse.move(at.x, at.y, { steps: 3 });
    await expect(page.locator('.config-editor__hover')).toBeVisible();
    await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
    await expect(page.locator('.config-editor__hover')).toBeVisible();
    expect(errors).toEqual([]);
  });
});

// L28-A2: the line commands did nothing at all, so next to a lock or in a
// read-only editor they said nothing either.
const COMMANDS = {
  'Alt+↑': 'Alt+ArrowUp',
  'Alt+↓': 'Alt+ArrowDown',
  'Shift+Alt+↑': 'Shift+Alt+ArrowUp',
  'Shift+Alt+↓': 'Shift+Alt+ArrowDown',
  'Ctrl+Shift+K': 'Control+Shift+K',
  'Ctrl+]': 'Control+BracketRight',
  'Ctrl+[': 'Control+BracketLeft',
};
// `name` on line 2, between two other fields, its value locked.
const LOCKED = {
  yaml: 'a: 1\nname: edge\nb: 2\n',
  json: '{\n  "name": "edge",\n  "b": 2\n}\n',
  toml: 'a = 1\nname = "edge"\nb = 2\n',
  hcl: 'a = 1\nname = "edge"\nb = 2\n',
  xml: '<a>1</a>\n<name>edge</name>\n<b>2</b>\n',
};

async function caretOnLine2(page) {
  await input(page).focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
}

test.describe('L28-A2', () => {
  test('the line commands edit the lines, each one undo step', async ({ page }) => {
    const errors = await open(page, { value: 'a: 1\nb: 2\nc: 3\n' });
    for (const [key, expected] of [
      ['Alt+ArrowUp', 'b: 2\na: 1\nc: 3\n'],
      ['Alt+ArrowDown', 'a: 1\nc: 3\nb: 2\n'],
      ['Shift+Alt+ArrowDown', 'a: 1\nb: 2\nb: 2\nc: 3\n'],
      ['Shift+Alt+ArrowUp', 'a: 1\nb: 2\nb: 2\nc: 3\n'],
      ['Control+Shift+K', 'a: 1\nc: 3\n'],
      ['Control+BracketRight', 'a: 1\n  b: 2\nc: 3\n'],
    ]) {
      await caretOnLine2(page);
      await page.keyboard.press(key);
      await expect(input(page), key).toHaveValue(expected);
      await page.keyboard.press('Control+z');
      await expect(input(page), `${key} undone`).toHaveValue('a: 1\nb: 2\nc: 3\n');
    }
    expect(errors).toEqual([]);
  });

  for (const [language, value] of Object.entries(LOCKED)) {
    test(`${language}: next to a lock, a line command changes the locked value never, and says why when refused`, async ({ page }) => {
      const errors = await open(page, { value, language, readOnlyPaths: ['/name'] });
      let refused = 0;
      for (const [command, key] of Object.entries(COMMANDS)) {
        // An unindented line has nothing to outdent: nothing to refuse.
        if (command === 'Ctrl+[') continue;
        await caretOnLine2(page);
        await page.keyboard.press(key);
        await page.waitForTimeout(100);
        const now = await input(page).inputValue();
        if (now === value) {
          refused += 1;
          // Refused: why, at the caret and to screen readers.
          const tip = page.locator('.config-editor__protect-tip--refused');
          await expect(tip, command).toBeVisible();
          await expect.poll(() => said(page), command).toMatch(/name|read-only|locked/i);
        } else {
          // Made: the locked value is still there, as it was.
          expect(now, command).toContain('edge');
          // Back to the text the host gave (an undo may itself be refused:
          // the lock check reads a swap of two lines one way only).
          await page.evaluate((value) => window.editor.update({ value }), value);
          await expect(input(page), `${command}, then the host's text`).toHaveValue(value);
        }
      }
      // Deleting the locked line is refused, at least.
      expect(refused).toBeGreaterThan(0);
      expect(errors).toEqual([]);
    });
  }

  for (const [command, key] of Object.entries(COMMANDS)) {
    test(`a read-only editor: ${command} says it cannot edit`, async ({ page }) => {
      const errors = await open(page, { value: 'a: 1\nb: 2\n', readOnly: true });
      await caretOnLine2(page);
      await page.keyboard.press(key);
      await expect.poll(() => said(page)).toMatch(/Cannot edit in a read-only editor/);
      await expect(page.locator('.config-editor__read-only-notice:visible')).toBeVisible();
      await expect(input(page)).toHaveValue('a: 1\nb: 2\n');
      expect(errors).toEqual([]);
    });
  }
});
