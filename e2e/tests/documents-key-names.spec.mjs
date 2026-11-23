import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// Key names across the documents of a view, on the example pages' `?keys`
// setup (js/examples/key-names.js): a server file and two drop-ins, two
// schemas with an `$id` each, whose `default_client` and a route's `client`
// name a key of either kind of file's `/clients`. The host gives every
// file's names once (`home-saved` from the saved home.yaml, `office` from a
// file that is not open); the open files' live names replace theirs.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => `${pathToFileURL(join(js, 'examples', name)).href}?keys`;

// The documents' ids; their file names are `server.yaml`,
// `server.d/home.yaml` and `server.d/lab.yaml`.
const SERVER = 'server';
const HOME = 'home';
const LAB = 'lab';

const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const panel = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"]`);
const input = (page, id) => panel(page, id).locator('.config-editor__input');
const suggested = (page, id) =>
  panel(page, id).locator('.config-editor__completions .config-editor__completion-key').allTextContents();
/** What the open list shows beside `name`: its source. */
const detailOf = (page, id, name) =>
  panel(page, id)
    .locator('.config-editor__completions .config-editor__completion')
    .filter({ has: page.locator('.config-editor__completion-key', { hasText: new RegExp(`^${name}$`) }) })
    .locator('.config-editor__completion-detail')
    .textContent();

/** Puts the caret of document `id` right after the first `after` in its text. */
async function caretAfter(page, id, after) {
  await tab(page, id).click();
  await input(page, id).click();
  await input(page, id).evaluate((element, text) => {
    const at = element.value.indexOf(text) + text.length;
    element.setSelectionRange(at, at);
  }, after);
}

async function open(page, name) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(example(name));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').first().waitFor();
  await expect(page.locator('[role="tab"]')).toHaveCount(3);
  return errors;
}

for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
  test.describe(`key names across documents, ${name}`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('a key typed in one document, unsaved, is suggested in another at once', async ({ page }) => {
      const errors = await open(page, name);
      // A new client in the drop-in, not saved.
      await caretAfter(page, HOME, 'clients:\n');
      await page.keyboard.insertText('  typed-now: {}\n');
      await expect(tab(page, HOME)).toHaveAttribute('data-dirty', 'true');
      // At once, in the server file: the drop-ins' live names, its own, the
      // host's for the file not open, and the reserved word; the saved
      // home.yaml's names stand replaced by its live ones.
      await caretAfter(page, SERVER, 'default_client: ');
      await page.keyboard.press('Control+Space');
      // A list asked for while the page was still busy may not open: ask
      // again until it shows (what it lists is the test).
      await expect
        .poll(async () => {
          const names = await suggested(page, SERVER);
          if (names.length === 0) await page.keyboard.press('Control+Space');
          return names;
        })
        .toContain('typed-now');
      const names = await suggested(page, SERVER);
      for (const wanted of ['main', 'typed-now', 'home', 'lab', 'office', 'self']) expect(names).toContain(wanted);
      expect(names).not.toContain('home-saved');
      expect(names.filter((name) => name === 'home')).toHaveLength(1);
      // Beside an open document's name, its file name (not its id).
      expect(await detailOf(page, SERVER, 'typed-now')).toContain('server.d/home.yaml');
      expect(await detailOf(page, SERVER, 'lab')).toContain('server.d/lab.yaml');
      expect(errors).toEqual([]);
    });

    test('go to definition of a name another open document defines switches to it', async ({ page }) => {
      const errors = await open(page, name);
      await caretAfter(page, SERVER, 'default_client: ');
      await page.keyboard.type('lab');
      await page.keyboard.press('Escape');
      await page.keyboard.press('ArrowLeft');
      await page.keyboard.press('F12');
      await expect(tab(page, LAB)).toHaveAttribute('aria-selected', 'true');
      await expect
        .poll(() => input(page, LAB).evaluate((element) => element.value.slice(element.selectionStart).startsWith('lab')))
        .toBe(true);
      await expect(page.locator('#log')).not.toContainText('goto');
      expect(errors).toEqual([]);
    });

    test('go to definition of a name only the host knows asks the host', async ({ page }) => {
      const errors = await open(page, name);
      await caretAfter(page, SERVER, 'default_client: ');
      await page.keyboard.type('office');
      await page.keyboard.press('Escape');
      await page.keyboard.press('ArrowLeft');
      await page.keyboard.press('F12');
      await expect(page.locator('#log')).toContainText('goto server.d/office.yaml office');
      await expect(tab(page, SERVER)).toHaveAttribute('aria-selected', 'true');
      expect(errors).toEqual([]);
    });
  });
}

// One `ConfigEditor` (vanilla.html, its options given by `?options=`): the
// document's own keys and the host's `keyNames`, and go to definition by
// `F12` and by `Ctrl`+click.
test.describe('key names in one editor, vanilla.html', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  const ID = 'https://example.com/server';
  const options = {
    value: 'default_client: \nclients:\n  main: {}\n  lab: {}\n',
    schema: JSON.stringify({
      $id: ID,
      type: 'object',
      properties: { default_client: { type: 'string', 'x-config-editor-keys': '/clients' }, clients: { type: 'object' } },
    }),
    keyNames: [{ schema: ID, pointer: '/clients', names: [{ name: 'office', source: 'server.d/office.yaml' }] }],
  };
  const url = () =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;
  const field = (page) => page.locator('.config-editor__input');
  const VALUE_AT = 'default_client: '.length;

  async function openEditor(page) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(url());
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await field(page).waitFor();
    await field(page).click();
    await field(page).evaluate((element, at) => element.setSelectionRange(at, at), VALUE_AT);
    return errors;
  }

  /** Types `name` as `default_client`'s value, the list closed, the caret inside it. */
  async function typeValue(page, name) {
    await page.keyboard.type(name);
    await page.keyboard.press('Escape');
    await page.keyboard.press('ArrowLeft');
  }

  const caretAt = (page, text) =>
    field(page).evaluate((element, wanted) => element.value.slice(element.selectionStart).startsWith(wanted), text);
  const wentTo = (page) => page.evaluate(() => window.wentTo ?? []);

  test("the document's own keys and the host's are suggested", async ({ page }) => {
    const errors = await openEditor(page);
    await page.keyboard.press('Control+Space');
    const keys = () => page.locator('.config-editor__completions .config-editor__completion-key').allTextContents();
    await expect.poll(keys).toContain('office');
    for (const wanted of ['main', 'lab']) expect(await keys()).toContain(wanted);
    expect(errors).toEqual([]);
  });

  test('F12 on a name the document defines moves the caret to its key', async ({ page }) => {
    const errors = await openEditor(page);
    await typeValue(page, 'lab');
    await page.keyboard.press('F12');
    await expect.poll(() => caretAt(page, 'lab: {}')).toBe(true);
    expect(await wentTo(page)).toEqual([]);
    expect(errors).toEqual([]);
  });

  test('F12 and Ctrl+click on a name only the host knows ask the host', async ({ page }) => {
    const errors = await openEditor(page);
    await typeValue(page, 'office');
    const target = { schema: ID, pointer: '/clients', name: 'office', source: 'server.d/office.yaml' };
    await page.keyboard.press('F12');
    await expect.poll(() => wentTo(page)).toEqual([target]);
    // Inside `office` on the first line, measured in the text area's font.
    const position = await field(page).evaluate((element, before) => {
      const style = getComputedStyle(element);
      const probe = document.createElement('span');
      probe.style.font = style.font;
      probe.style.whiteSpace = 'pre';
      probe.textContent = before;
      document.body.append(probe);
      const width = probe.getBoundingClientRect().width;
      probe.remove();
      const line = parseFloat(style.lineHeight) || parseFloat(style.fontSize) * 1.4;
      return { x: parseFloat(style.paddingLeft) + width, y: parseFloat(style.paddingTop) + line / 2 };
    }, 'default_client: off');
    await field(page).click({ modifiers: ['Control'], position });
    await expect.poll(() => wentTo(page)).toEqual([target, target]);
    expect(errors).toEqual([]);
  });

  test("an object's keys named by its propertyNames are suggested, and F12 on one goes to its key", async ({
    page,
  }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    const reach = {
      ...options,
      value: 'clients:\n  main: {}\n  lab: {}\nreach:\n  main: []\n  \n',
      schema: JSON.stringify({
        $id: ID,
        type: 'object',
        properties: {
          clients: { type: 'object' },
          reach: {
            type: 'object',
            propertyNames: { 'x-config-editor-keys': '/clients', examples: ['self'] },
            additionalProperties: { type: 'array' },
          },
        },
      }),
    };
    await page.goto(
      `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(reach))}`,
    );
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await field(page).click();
    // A new key of `reach`: the names it does not have yet, in order.
    await field(page).evaluate((element) => {
      const at = element.value.length - 1;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('Control+Space');
    const keys = () => page.locator('.config-editor__completions .config-editor__completion-key').allTextContents();
    await expect.poll(keys).toEqual(['lab', 'office', 'self']);
    await page.keyboard.press('Escape');
    // F12 on the key `main` of `reach` goes to the client `main`.
    await field(page).evaluate((element) => {
      const at = element.value.indexOf('reach:\n  main') + 'reach:\n  ma'.length;
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('F12');
    await expect.poll(() => caretAt(page, 'main: {}')).toBe(true);
    expect(await wentTo(page)).toEqual([]);
    expect(errors).toEqual([]);
  });
});
