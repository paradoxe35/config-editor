import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// The list of uses as people meet it: started from a host's button (the
// focus goes to the text, the list keys work), a host search that fails
// (the list says so, never "No uses"; `onError` hears it), the row at the
// caret, a group's title kept in view, secrets hidden in rows; and every
// listbox's active option in forced colours, every popup's edge under more
// contrast.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const ID = 'https://example.com/server';
const SCHEMA = {
  $id: ID,
  type: 'object',
  properties: {
    clients: { type: 'object' },
    default_client: { type: 'string', 'x-config-editor-keys': '/clients' },
    members: { type: 'array', items: { type: 'string', 'x-config-editor-keys': '/clients' } },
    ann: {
      type: 'object',
      properties: {
        role: { type: 'string', 'x-config-editor-keys': '/clients' },
        password: { type: 'string', writeOnly: true },
      },
    },
    port: { type: 'integer' },
  },
};
const VALUE = 'clients:\n  main: {}\n  lab: {}\ndefault_client: main\nmembers: [lab, main]\nport: 80\n';

const vanilla = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
const field = (page) => page.locator('.config-editor__input').first();
const list = (page) => page.locator('.config-editor__uses');
const rows = (page) => list(page).locator('[role="option"]');
const said = (page) => page.locator('.config-editor__announcement').first();

async function openEditor(page, extra = {}) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla({ value: VALUE, schema: JSON.stringify(SCHEMA), ...extra }));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await field(page).waitFor();
  await field(page).click();
  return errors;
}

/** Puts the caret `into` characters into the `nth` `word` of the text. */
const caretOn = (page, word, nth = 0, into = 1) =>
  field(page).evaluate(
    (element, [word, nth, into]) => {
      let at = -1;
      for (let index = 0; index <= nth; index += 1) at = element.value.indexOf(word, at + 1);
      element.setSelectionRange(at + into, at + into);
    },
    [word, nth, into],
  );

const activeIndex = (page) =>
  rows(page).evaluateAll((all) => all.findIndex((row) => row.getAttribute('aria-selected') === 'true'));

test.describe('the list of uses, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test("findUses() from a host's button moves the focus to the text, and the list keys work", async ({ page }) => {
    const errors = await openEditor(page);
    await caretOn(page, 'main', 1);
    await page.evaluate(() => {
      const button = document.createElement('button');
      button.id = 'host-uses';
      button.textContent = 'Find uses';
      button.addEventListener('click', () => window.editor.findUses());
      document.body.prepend(button);
    });
    await page.locator('#host-uses').click();
    await expect(rows(page)).toHaveCount(2);
    await expect(field(page)).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(rows(page).nth(1)).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('Escape');
    await expect(list(page)).toHaveCount(0);
    await expect(field(page)).toBeFocused();
    // Enter goes to the use, not the button.
    await page.locator('#host-uses').click();
    await page.keyboard.press('Enter');
    await expect(list(page)).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('the list starts on the use at the caret; on the definition, on the first use', async ({ page }) => {
    const errors = await openEditor(page);
    // `main` in `members: [lab, main]`: the second use.
    await caretOn(page, 'main', 2);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    expect(await activeIndex(page)).toBe(1);
    await expect(field(page)).toHaveAttribute('aria-activedescendant', await rows(page).nth(1).getAttribute('id'));
    await page.keyboard.press('Escape');
    await caretOn(page, 'main', 0);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    expect(await activeIndex(page)).toBe(0);
    expect(errors).toEqual([]);
  });

  for (const [kind, logged] of [
    ['throws', 'search backend down'],
    ['rejects', 'search backend down'],
    ['answers no list', 'the answer must be an array'],
  ]) {
    test(`a host search that ${kind} says the other files could not be searched`, async ({ page }) => {
      const errors = await openEditor(page);
      await page.evaluate((kind) => {
        window.answerUses = () => {
          if (kind === 'throws') throw new Error('search backend down');
          if (kind === 'rejects') return Promise.reject(new Error('search backend down'));
          return Promise.resolve({ nope: true });
        };
      }, kind);
      // One open use of `lab` (the list item), then the hint.
      await caretOn(page, 'lab', 0);
      await page.keyboard.press('Shift+F12');
      await expect(list(page).locator('.config-editor__uses-empty')).toHaveText('The other files could not be searched');
      await expect(rows(page)).toHaveCount(1);
      await expect(said(page)).toContainText('1 use. The other files could not be searched');
      await page.keyboard.press('Escape');
      // Nothing open uses it: never "No uses".
      await page.evaluate(() => window.editor.replaceValue('clients:\n  zed: {}\n'));
      await caretOn(page, 'zed', 0);
      await page.keyboard.press('Shift+F12');
      await expect(list(page).locator('.config-editor__uses-empty')).toHaveText('The other files could not be searched');
      await expect(said(page)).toContainText('The other files could not be searched');
      await expect(list(page)).not.toContainText('No uses');
      const console = errors.join('\n');
      expect(console).toContain(logged);
      if (kind !== 'answers no list') expect(console).not.toContain('must be an array');
    });
  }

  test('a use beside a secret hides it in the row and its name', async ({ page }) => {
    const errors = await openEditor(page, { value: 'clients:\n  admin: {}\nann: {role: admin, password: hunter2}\n' });
    await caretOn(page, 'admin', 1);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(1);
    const text = await rows(page).first().textContent();
    expect(text).toContain('role: admin');
    expect(text).not.toContain('hunter2');
    expect(text).toContain('…');
    expect(errors).toEqual([]);
  });

  test('wrapping to the first row shows its group title', async ({ page }) => {
    const many = Array.from({ length: 36 }, (_, index) => `  - main # ${index}`).join('\n');
    const errors = await openEditor(page, { value: `clients:\n  main: {}\nmembers:\n${many}\n` });
    await caretOn(page, 'main', 0);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(36);
    const body = list(page).locator('.config-editor__uses-body');
    await page.keyboard.press('ArrowUp');
    await expect(rows(page).nth(35)).toHaveAttribute('aria-selected', 'true');
    await expect.poll(() => body.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
    await page.keyboard.press('ArrowDown');
    await expect(rows(page).nth(0)).toHaveAttribute('aria-selected', 'true');
    await expect.poll(() => body.evaluate((element) => element.scrollTop)).toBe(0);
    await expect(list(page).locator('.config-editor__uses-file')).toBeInViewport();
    expect(errors).toEqual([]);
  });

  test('a second Ctrl+K Ctrl+I says Tab reaches the card links when it has some', async ({ page }) => {
    const errors = await openEditor(page);
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+I');
    await expect(page.locator('.config-editor__hover-action')).toHaveText(['Definition', 'Uses']);
    await page.keyboard.press('Control+K');
    await page.keyboard.press('Control+I');
    await expect(said(page)).toContainText('Escape returns to the text. Tab reaches its links.');
    expect(errors).toEqual([]);
  });

  test('Ctrl+Space on a read-only value shows why, and does not open a list on the next line', async ({ page }) => {
    const schema = JSON.stringify({
      properties: {
        users: {
          type: 'object',
          additionalProperties: {
            type: 'object',
            properties: {
              owner: { type: 'string', readOnly: true, 'x-readonly-reason': 'Set by admins' },
              token: { type: 'string', writeOnly: true },
            },
          },
        },
      },
    });
    const value = 'users:\n  bob:\n    owner: admin\n    token: hunter2\n';
    const errors = await openEditor(page, { value, schema });
    // `admin` selected, as a double click leaves it.
    await caretOn(page, 'admin', 0, 0);
    await page.keyboard.press('Shift+End');
    await page.keyboard.press('Control+Space');
    const tip = page.locator('.config-editor__protect-tip--refused');
    await expect(tip).toBeVisible();
    await expect(tip).toContainText('Set by admins');
    await expect(said(page)).toContainText('Set by admins');
    await expect(page.locator('.config-editor__completions [role="option"]')).toHaveCount(0);
    // The request ended: the next line opens no list.
    await page.keyboard.press('ArrowDown');
    await page.waitForTimeout(400);
    await expect(page.locator('.config-editor__completions [role="option"]')).toHaveCount(0);
    expect(await field(page).inputValue()).toBe(value);
    expect(errors).toEqual([]);
  });
});

test.describe('the documents view, documents.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test("findUses() from a host's button moves the focus to the active editor's text", async ({ page }) => {
    await page.goto(`${pathToFileURL(join(js, 'examples', 'documents.html')).href}?keys`);
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const panel = page.locator('[role="tabpanel"][data-document-id="server"]');
    await page.locator('[role="tab"][data-document-id="server"]').click();
    const input = panel.locator('.config-editor__input');
    await input.click();
    await input.evaluate((element) => {
      const at = element.value.indexOf('  main') + 4;
      element.setSelectionRange(at, at);
    });
    await page.evaluate(() => {
      const button = document.createElement('button');
      button.id = 'host-uses';
      button.textContent = 'Find uses';
      button.addEventListener('click', () => window.documentsView.findUses());
      document.body.prepend(button);
    });
    await page.locator('#host-uses').click();
    const rows = panel.locator('.config-editor__uses [role="option"]');
    await expect(rows.first()).toBeVisible();
    await expect(input).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(panel.locator('.config-editor__uses')).toHaveCount(0);
  });
});

test.describe("onFindUses failing reaches the framework components' onError", () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const kind of ['throws', 'rejects']) {
    test(`EditorBinding: a search that ${kind}`, async ({ page }) => {
      await page.goto(pathToFileURL(join(js, 'examples', 'vanilla.html')).href);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const found = await page.evaluate(
        async ([kind, schema, value]) => {
          window.editor?.destroy();
          const { EditorBinding } = await import('../config-editor/binding.js');
          const host = document.createElement('div');
          document.body.prepend(host);
          const reported = [];
          const binding = new EditorBinding('Test', {
            value,
            schema,
            onFindUses: () => {
              if (kind === 'throws') throw new Error('backend down');
              return Promise.reject(new Error('backend down'));
            },
            onError: (error) => reported.push(String(error)),
          });
          binding.attach(host);
          await new Promise((resolve) => setTimeout(resolve, 500));
          const input = host.querySelector('.config-editor__input');
          input.focus();
          const at = input.value.indexOf('main', input.value.indexOf('default_client')) + 1;
          input.setSelectionRange(at, at);
          binding.handle.findUses();
          await new Promise((resolve) => setTimeout(resolve, 300));
          return { reported, list: host.querySelector('.config-editor__uses')?.textContent ?? '' };
        },
        [kind, JSON.stringify(SCHEMA), VALUE],
      );
      expect(found.reported).toEqual(['Error: backend down']);
      expect(found.list).toContain('The other files could not be searched');
    });
  }
});

test.describe('forced colours and more contrast, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test("every listbox's active option is told from the others in forced colours", async ({ browser }) => {
    const context = await browser.newContext({ forcedColors: 'active', viewport: { width: 1000, height: 600 } });
    const page = await context.newPage();
    // Two uses of `main`; two keys left to suggest at the end.
    const errors = await openEditor(page, { value: 'clients:\n  main: {}\ndefault_client: main\nmembers: [main]\n' });
    const checked = [];
    const failures = [];
    // Each listbox on screen: its active option's colours differ from an
    // inactive one's, drawn in system colours.
    const checkListboxes = async () => {
      const found = await page.evaluate(() =>
        [...document.querySelectorAll('[role="listbox"]')].map((box) => {
          const look = (element) => {
            const style = getComputedStyle(element);
            return { background: style.backgroundColor, color: style.color };
          };
          // Not on screen (an empty suggestion list kept hidden).
          const shown = box.getBoundingClientRect();
          if (!shown.width || !shown.height) return null;
          const active = box.querySelector('[role="option"][aria-selected="true"]');
          const other = box.querySelector('[role="option"][aria-selected="false"]');
          return { id: box.className, active: active && look(active), other: other && look(other) };
        }),
      );
      for (const box of found.filter(Boolean)) {
        checked.push(box.id);
        if (!box.active || !box.other || box.active.background === box.other.background) failures.push(box);
      }
    };
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    await checkListboxes();
    await page.keyboard.press('Escape');
    await field(page).evaluate((element) => element.setSelectionRange(element.value.length, element.value.length));
    await page.keyboard.press('Control+Space');
    await expect(page.locator('.config-editor__completion-active')).toBeVisible();
    await checkListboxes();
    expect(checked.length).toBeGreaterThanOrEqual(2);
    expect(failures).toEqual([]);
    expect(errors).toEqual([]);
    await context.close();
  });

  test('the list of uses gets the strong edge under more contrast, as every popup', async ({ page }) => {
    await page.emulateMedia({ contrast: 'more' });
    const errors = await openEditor(page);
    await caretOn(page, 'main', 1);
    await page.keyboard.press('Shift+F12');
    await expect(rows(page)).toHaveCount(2);
    const look = await list(page).evaluate((element) => {
      const style = getComputedStyle(element);
      return { border: style.borderTopColor, text: style.color };
    });
    expect(look.border).toBe(look.text);
    expect(errors).toEqual([]);
  });
});
