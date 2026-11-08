import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 23 (accessibility): closing a tab with Delete keeps the focus on
// the tabs; the hover card's Documentation link is reached by Alt+Enter,
// named to screen readers and reached by Tab in `Ctrl+M` mode; the editor
// stays left to right in a right-to-left page; forced colors mark the
// selected tab; a diff marker's Enter focuses Revert and the preview sits
// right after the text in the Tab order; the textarea controls the list
// only while it is open. Through the npm package's example pages (run
// `npm run build` in js/ first).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name, options) =>
  `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

test.skip(!built, 'run `npm run build` in js/ first');

const input = (page) => page.locator('.config-editor__input');
const card = (page) => page.locator('.config-editor__hover');
const announcement = (page) => page.locator('.config-editor__announcement');
const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const documentInput = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"] .config-editor__input`);

async function open(page, name, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(example(name, options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).first().waitFor();
  return errors;
}

// Puts the caret before the `offset`-th character of `target` in the text,
// as a key press would (the editor notes the caret on keys).
async function caretIn(page, target, offset) {
  await input(page).evaluate(
    (element, [target, offset]) => {
      const at = element.value.indexOf(target) + offset - 1;
      element.focus();
      element.setSelectionRange(at, at);
    },
    [target, offset],
  );
  await page.keyboard.press('ArrowRight');
}

const showDocs = async (page) => {
  await page.keyboard.press('Control+k');
  await page.keyboard.press('Control+i');
  await expect(card(page)).toBeVisible();
};

test.describe('M23-A1: Delete on a tab', () => {
  for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
    test(`${name}: the focus stays on the tabs, so a second Delete closes the next tab, never edits it`, async ({ page }) => {
      const errors = await open(page, name);
      page.on('dialog', (dialog) => dialog.accept());
      const add = page.locator('.config-editor-documents__add');
      for (const [id, text] of [
        ['new1', 'one: 1'],
        ['new2', 'two: 2'],
      ]) {
        await add.click();
        await expect(tab(page, id)).toHaveAttribute('aria-selected', 'true');
        await expect(documentInput(page, id)).toBeFocused();
        await page.keyboard.type(text);
        await expect(tab(page, id)).toHaveAttribute('data-dirty', 'true');
      }
      const changes = async () => (await page.locator('#log').textContent()).split('\n').filter((line) => line === 'change new2').length;
      const before = await changes();
      // Both are dirty: each Delete asks, and the host's confirm is accepted.
      await tab(page, 'new1').click();
      await expect(tab(page, 'new1')).toBeFocused();
      await page.keyboard.press('Delete');
      await expect(tab(page, 'new1')).toHaveCount(0);
      await expect(tab(page, 'new2')).toHaveAttribute('aria-selected', 'true');
      await expect(tab(page, 'new2')).toBeFocused();
      await expect(documentInput(page, 'new2')).toHaveValue('two: 2');
      await page.keyboard.press('Delete');
      await expect(tab(page, 'new2')).toHaveCount(0);
      await expect(tab(page, 'clients')).toBeFocused();
      expect(await changes()).toBe(before);
      expect(errors).toEqual([]);
    });
  }

  test('a click on × leaves the focus in the editor it was in', async ({ page }) => {
    const errors = await open(page, 'documents.html');
    await tab(page, 'limits').click();
    await documentInput(page, 'limits').click();
    await tab(page, 'limits').locator('.config-editor-documents__close').click();
    await expect(tab(page, 'limits')).toHaveCount(0);
    await expect(documentInput(page, 'clients')).toBeFocused();
    expect(errors).toEqual([]);
  });
});

test.describe("M23-A2: the hover card's Documentation link, by keyboard", () => {
  const url = 'https://example.com/docs/listen';
  const schema = JSON.stringify({
    type: 'object',
    properties: {
      config: {
        type: 'object',
        properties: { listen: { type: 'integer', description: 'Port to listen on.', 'x-docs-url': url } },
      },
    },
  });
  const values = {
    yaml: 'config:\n  listen: 80\n',
    json: '{\n  "config": {\n    "listen": 80\n  }\n}\n',
    toml: '[config]\nlisten = 80\n',
    hcl: 'config {\n  listen = 80\n}\n',
    xml: '<config>\n  <listen>80</listen>\n</config>\n',
  };
  for (const [language, value] of Object.entries(values)) {
    test(`${language}: Alt+Enter opens it, the announcement names it, Tab in Ctrl+M mode reaches it`, async ({ page, context }) => {
      const opened = [];
      context.on('page', (popup) => opened.push(popup.url()));
      await page.route('https://example.com/**', (route) => route.fulfill({ body: 'docs' }));
      const errors = await open(page, 'vanilla.html', { value, language, schema });
      await caretIn(page, 'listen', 2);
      await showDocs(page);
      await expect(card(page).locator('.config-editor__hover-link')).toHaveAttribute('href', url);
      await expect(announcement(page)).toContainText(`Documentation: ${url} (Alt+Enter opens it).`);
      await page.keyboard.press('Alt+Enter');
      await expect.poll(() => opened).toEqual([url]);
      expect(await input(page).inputValue()).toBe(value);

      // Ctrl+M: Tab moves the focus, into the card's link first.
      await caretIn(page, 'listen', 2);
      await page.keyboard.press('Control+m');
      await showDocs(page);
      const link = card(page).locator('.config-editor__hover-link');
      await page.keyboard.press('Tab');
      await expect(link).toBeFocused();
      await expect(card(page)).toBeVisible();
      // Back to the text, the card still shown; Escape on the link closes it
      // and the text takes the focus back.
      await page.keyboard.press('Shift+Tab');
      await expect(input(page)).toBeFocused();
      await expect(card(page)).toBeVisible();
      await page.keyboard.press('Tab');
      await expect(link).toBeFocused();
      await page.keyboard.press('Escape');
      await expect(card(page)).toHaveCount(0);
      await expect(input(page)).toBeFocused();
      // Tab past the link: on to the page, the card closed.
      await showDocs(page);
      await page.keyboard.press('Tab');
      await expect(link).toBeFocused();
      await page.keyboard.press('Tab');
      await expect(card(page)).toHaveCount(0);
      expect(await page.evaluate(() => document.activeElement !== document.body)).toBe(true);
      expect(await input(page).inputValue()).toBe(value);
      expect(errors).toEqual([]);
    });
  }

  test("Escape on the problem tip's link gives the focus back to the text", async ({ page }) => {
    const errors = await open(page, 'vanilla.html', {
      value: 'port: eighty\n',
      schema: JSON.stringify({ type: 'object', properties: { port: { type: 'integer', 'x-docs-url': 'https://example.com/port' } } }),
    });
    await expect(page.locator('.config-editor__diagnostic-marker')).toHaveCount(1);
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+m');
    await page.keyboard.press('F8');
    const link = page.locator('.config-editor__problem-tip-link');
    await expect(link).toBeVisible();
    await page.keyboard.press('Tab');
    await expect(link).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(link).toBeHidden();
    await expect(input(page)).toBeFocused();
    expect(errors).toEqual([]);
  });
});

test.describe('M23-A3: in a right-to-left page', () => {
  const rtl = (page) => page.evaluate(() => document.documentElement.setAttribute('dir', 'rtl'));

  test('the editor, its footer and its popups stay left to right; right-to-left values stay text', async ({ page }) => {
    const errors = await open(page, 'vanilla.html', {
      value: 'name: שלום עולם\ndb:\n  host: x\nport: eighty\n',
      schema: JSON.stringify({
        type: 'object',
        properties: {
          name: { type: 'string', description: 'The name.' },
          db: { type: 'object', properties: { host: { type: 'string' }, user: { type: 'string' } } },
          port: { type: 'integer' },
        },
      }),
    });
    await rtl(page);
    expect(await page.evaluate(() => getComputedStyle(document.body).direction)).toBe('rtl');
    const direction = (selector) => page.locator(selector).first().evaluate((element) => getComputedStyle(element).direction);
    for (const selector of ['.config-editor', '.config-editor__input', '.config-editor__highlight', '.config-editor__gutter', '.config-editor__status']) {
      expect(await direction(selector), selector).toBe('ltr');
    }
    // The gutter on the left, `db:` read in order.
    const gutter = await page.locator('.config-editor__gutter').boundingBox();
    const text = await input(page).boundingBox();
    expect(gutter.x).toBeLessThan(text.x);
    const order = await page.locator('.config-editor__highlight .config-editor__code-line').nth(1).evaluate((line) => {
      const node = document.createTreeWalker(line, NodeFilter.SHOW_TEXT).nextNode();
      const box = (at) => {
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + 1);
        return range.getBoundingClientRect().left;
      };
      return node.textContent.startsWith('db') ? box(0) < box(1) : null;
    });
    expect(order).toBe(true);
    await expect(page.locator('.config-editor__highlight .config-editor__code-line').first()).toContainText('שלום עולם');

    // Each popup: the card, the tip, the find widget and the suggestions.
    await caretIn(page, 'name', 2);
    await showDocs(page);
    expect(await direction('.config-editor__hover')).toBe('ltr');
    await page.keyboard.press('Escape');
    await page.keyboard.press('F8');
    await expect(page.locator('.config-editor__problem-tip')).toBeVisible();
    expect(await direction('.config-editor__problem-tip')).toBe('ltr');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+f');
    await expect(page.locator('.config-editor__find')).toBeVisible();
    expect(await direction('.config-editor__find')).toBe('ltr');
    await page.keyboard.press('Escape');
    await caretIn(page, '  host', 3);
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+Space');
    const list = page.locator('.config-editor__completions');
    await expect(list.locator('[role="option"]').first()).toBeVisible();
    expect(await direction('.config-editor__completions')).toBe('ltr');
    // By the caret, near the text's left edge, not across the editor.
    const listBox = await list.boundingBox();
    expect(listBox.x - text.x).toBeLessThan(120);
    expect(errors).toEqual([]);
  });

  test('the documents view keeps its tabs left to right', async ({ page }) => {
    const errors = await open(page, 'documents.html');
    await rtl(page);
    const strip = page.locator('.config-editor-documents');
    expect(await strip.evaluate((element) => getComputedStyle(element).direction)).toBe('ltr');
    const first = await tab(page, 'main').boundingBox();
    const second = await tab(page, 'limits').boundingBox();
    expect(first.x).toBeLessThan(second.x);
    // ArrowRight goes to the tab on the right.
    await tab(page, 'main').focus();
    await page.keyboard.press('ArrowRight');
    await expect(tab(page, 'limits')).toBeFocused();
    expect(errors).toEqual([]);
  });
});

test.describe('M23-A4: forced colors', () => {
  test.use({ forcedColors: 'active' });

  test('the selected tab has a system-color bar, closable or not', async ({ page }) => {
    const errors = await open(page, 'documents.html');
    const highlight = await page.evaluate(() => {
      const probe = document.createElement('div');
      probe.style.forcedColorAdjust = 'none';
      probe.style.backgroundColor = 'Highlight';
      document.body.append(probe);
      const color = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return color;
    });
    const bar = (id) =>
      tab(page, id).evaluate((element) => {
        const style = getComputedStyle(element, '::before');
        return style.content === 'none' ? null : style.backgroundColor;
      });
    for (const [id, other] of [
      ['main', 'limits'],
      ['limits', 'main'],
    ]) {
      await tab(page, id).click();
      await expect(tab(page, id)).toHaveAttribute('aria-selected', 'true');
      expect(await bar(id), id).toBe(highlight);
      expect(await bar(other), other).toBe(null);
    }
    expect(errors).toEqual([]);
  });
});

test.describe('L23-A1, L23-A2: the change preview from the keyboard', () => {
  const options = { value: 'a: 1\nb: 2\nc: 3\n', baseline: 'a: 1\nb: 9\nc: 3\n' };

  test('Enter on a diff marker focuses Revert; the preview is right after the text in the Tab order', async ({ page }) => {
    const errors = await open(page, 'vanilla.html', options);
    // Where Tab goes from the text with no preview (Ctrl+M mode).
    await input(page).click();
    await page.keyboard.press('Control+m');
    await page.keyboard.press('Tab');
    await page.evaluate(() => (window.afterText = document.activeElement));
    expect(await page.evaluate(() => window.afterText !== document.body)).toBe(true);

    const revert = page.locator('.config-editor__diff-peek-revert');
    await page.locator('.config-editor__diff-change-trigger').first().focus();
    await page.keyboard.press('Enter');
    await expect(revert).toBeFocused();
    // Shift+Tab from Revert: back to the text.
    await page.keyboard.press('Shift+Tab');
    await expect(input(page)).toBeFocused();
    await expect(revert).toBeVisible();
    // Tab from the text: Revert, the close button, then what follows the text.
    await page.keyboard.press('Tab');
    await expect(revert).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(page.locator('.config-editor__diff-peek-close')).toBeFocused();
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => document.activeElement === window.afterText)).toBe(true);
    expect(await input(page).inputValue()).toBe(options.value);
    expect(errors).toEqual([]);
  });
});

test.describe('L23-A4: the open suggestion list', () => {
  test('the textarea controls the list only while it is open', async ({ page }) => {
    const errors = await open(page, 'vanilla.html', {
      value: 'port: 1\n',
      schema: JSON.stringify({ type: 'object', properties: { port: {}, host: {} } }),
    });
    await expect(input(page)).toHaveAttribute('role', 'textbox');
    await expect(input(page)).not.toHaveAttribute('aria-controls', /./);
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+Space');
    const list = page.locator('.config-editor__completions');
    await expect(list.locator('[role="option"]').first()).toBeVisible();
    await expect(input(page)).toHaveAttribute('aria-controls', await list.getAttribute('id'));
    await expect(input(page)).toHaveAttribute('aria-activedescendant', /./);
    await page.keyboard.press('Escape');
    await expect(list.locator('[role="option"]')).toHaveCount(0);
    await expect(input(page)).not.toHaveAttribute('aria-controls', /./);
    await expect(input(page)).not.toHaveAttribute('aria-activedescendant', /./);
    expect(errors).toEqual([]);
  });
});
