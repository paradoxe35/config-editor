import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Hover docs from the keyboard: Ctrl+K Ctrl+I, the focused card, its link, Escape.

const input = (page) => page.locator('.config-editor__input');

const announcement = (page) => page.locator('.config-editor__announcement');

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

const vanilla = (options = {}) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

async function open(page, options) {
  const errors = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await page.goto(vanilla(options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
  return errors;
}

const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];

const pageUrl = (options) =>
  `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

const DOCS = {
  yaml: 'a: 1\nb: 2\nport: 3\n',
  json: '{ "a": 1,\n  "b": 2,\n  "port": 3 }\n',
  toml: 'a = 1\nb = 2\nport = 3\n',
  hcl: 'a = 1\nb = 2\nport = 3\n',
  xml: '<a>1</a>\n<b>2</b>\n<port>3</port>\n',
};

const schema = JSON.stringify({ type: 'object', properties: { port: { type: 'integer', description: 'Port docs' } } });

test.describe('Alt+Enter', () => {
  setupEditor(test);

  test('does nothing without a docs link at the caret', async ({ page }) => {
    const e = editor(page);
    await e.input.click();
    const before = await e.value();
    await page.keyboard.press('Alt+Enter');
    expect(await e.value()).toBe(before);
  });
});

test.describe('hover docs from the keyboard', () => {
  setupEditor(test, '?language=yaml');

  test('Ctrl+K Ctrl+I shows the docs of the key at the caret and says them; Escape closes them', async ({ page }) => {
    const e = editor(page);
    await e.paste('listen: 7835\n');
    await input(page).evaluate((element) => {
      element.focus();
      element.setSelectionRange(2, 2);
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    const description = await card.locator('.config-editor__hover-description').first().textContent();
    await expect(announcement(page)).toContainText(description.trim());
    await expect(announcement(page)).toContainText('listen');
    await page.keyboard.press('Escape');
    await expect(card).toHaveCount(0);
    // The caret did not move, and the text did not change.
    expect(await e.value()).toBe('listen: 7835\n');
  });

  test('the chord still folds and unfolds everything', async ({ page }) => {
    const e = editor(page);
    await e.paste('web:\n  listen: 1\n  hosts: []\n');
    await input(page).click();
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+0');
    await expect(page.locator('.config-editor__fold-chip')).toHaveCount(1);
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+j');
    await expect(page.locator('.config-editor__fold-chip')).toHaveCount(0);
  });
});

test.describe('Ctrl+K chords', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test('Ctrl+K then an unknown key types nothing, as in VS Code', async ({ page }) => {
    const errors = await open(page, { value: 'a: 1\n', schema: '{}' });
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('x');
    await page.keyboard.type('b');
    await expect.poll(() => input(page).inputValue()).toBe('a: 1\nb');
    expect(errors).toEqual([]);
  });

  test('Ctrl+K Ctrl+I says each part once, with one full stop after it', async ({ page }) => {
    const schema = JSON.stringify({
      type: 'object',
      properties: { listen: { type: 'integer', title: 'Port', description: 'Port to listen on.', default: 7835 } },
    });
    const errors = await open(page, { value: 'listen: 80\n', schema });
    await input(page).click();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    await expect(page.locator('.config-editor__hover')).toHaveCount(1);
    await expect(announcement(page)).toContainText('Port to listen on. Default: 7835');
    expect(await announcement(page).textContent()).not.toContain('..');
    expect(errors).toEqual([]);
  });

  test('Ctrl+K Ctrl+I with the caret on the `<` of an XML tag shows its docs', async ({ page }) => {
    const schema = JSON.stringify({
      type: 'object',
      properties: { config: { type: 'object', properties: { listen: { type: 'integer', description: 'Port to listen on' } } } },
    });
    const errors = await open(page, { value: '<config>\n  <listen>80</listen>\n</config>\n', language: 'xml', schema });
    await input(page).click();
    // Line 2, on the `<` of `<listen>`.
    await input(page).evaluate((element) => {
      const at = element.value.indexOf('<listen>');
      element.focus();
      element.setSelectionRange(at, at);
    });
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
    const card = page.locator('.config-editor__hover');
    await expect(card).toHaveCount(1);
    await expect(card.locator('.config-editor__hover-key')).toHaveText('listen');
    expect(errors).toEqual([]);
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const vanilla = (options = {}) =>
    `${pathToFileURL(join(js, 'examples', 'vanilla.html')).href}?options=${encodeURIComponent(JSON.stringify(options))}`;

  const input = (page) => page.locator('.config-editor__input');

  async function open(page, options) {
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
    await page.goto(vanilla(options));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await input(page).waitFor();
    return errors;
  }

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe('the textarea, Ctrl+K Ctrl+I, docs links and the footer', () => {
    test.use({ viewport: { width: 1000, height: 600 } });
    const listenSchema = JSON.stringify({
      type: 'object',
      properties: { config: { type: 'object', properties: { listen: { type: 'integer', description: 'Port to listen on.' } } } },
    });
    const values = {
      yaml: ['config:\n  listen: 80\n', '80'],
      json: ['{\n  "config": {\n    "listen": 80\n  }\n}\n', '80'],
      toml: ['[config]\nlisten = 80\n', '80'],
      hcl: ['config {\n  listen = 80\n}\n', '80'],
      xml: ['<config>\n  <listen>80</listen>\n</config>\n', '80'],
    };
    for (const [language, [value, target]] of Object.entries(values)) {
      test(`${language}: Ctrl+K Ctrl+I on a value shows its field's docs`, async ({ page }) => {
        const errors = await open(page, { value, language, schema: listenSchema });
        await input(page).click();
        await input(page).evaluate((element, target) => {
          const at = element.value.indexOf(target) + 1;
          element.setSelectionRange(at, at);
        }, target);
        await page.keyboard.press('ArrowRight');
        await page.keyboard.press('ArrowLeft');
        await page.keyboard.press('Control+k');
        await page.keyboard.press('Control+i');
        await expect(page.locator('.config-editor__hover')).toContainText('Port to listen on.');
        expect(errors).toEqual([]);
      });
    }
  });
});

// A group of its own (needs the built npm packages).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input');

  const card = (page) => page.locator('.config-editor__hover');

  const announcement = (page) => page.locator('.config-editor__announcement');

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

  test.skip(!built, 'run `npm run build` in js/ first');

  test.describe("the hover card's Documentation link, by keyboard", () => {
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
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');

  const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));

  const example = (name, options) =>
    `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

  const input = (page) => page.locator('.config-editor__input').first();

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

  const chord = async (page) => {
    await page.keyboard.press('Control+k');
    await page.keyboard.press('Control+i');
  };

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  test.describe('the focused hover card', () => {
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
});

test.describe('Escape and the keyboard with a card or tip showing', () => {
  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 900, height: 500 } });
  for (const language of LANGUAGES) {
    test(`${language}: Escape closes the popup with the pointer on what it shows for`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      const value = DOCS[language];
      const line = value.split('\n').findIndex((text) => text.includes('port')) + 1;
      const problem = {
        range: { startLine: line, startColumn: 1, endColumn: value.split('\n')[line - 1].length + 1 },
        severity: 'warning',
        message: 'Checked by the host',
      };
      for (const [options, popup] of [
        [{ schema }, '.config-editor__hover'],
        [{ hostDiagnostics: [problem] }, '.config-editor__problem-tip'],
      ]) {
        await page.goto(pageUrl({ value, language, ...options }));
        await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        const port = await page.evaluate(() => {
          const row = [...document.querySelectorAll('.config-editor__highlight .config-editor__code-line')].find((row) =>
            row.textContent.includes('port'),
          );
          // The middle of `port`'s second letter.
          let left = row.textContent.indexOf('port') + 1;
          const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
          let node;
          while ((node = walker.nextNode()) && left >= node.length) left -= node.length;
          const range = document.createRange();
          range.setStart(node, left);
          range.setEnd(node, left + 1);
          const box = range.getBoundingClientRect();
          return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
        });
        await page.mouse.move(port.x, port.y);
        await expect(page.locator(popup)).toBeVisible();
        expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
        await page.keyboard.press('Escape');
        await expect(page.locator(popup)).toBeHidden();
      }
      expect(errors).toEqual([]);
    });
  }
});

// A group of its own (needs the built npm packages, a 1000×700 window).
test.describe(() => {
  // Escape closes the hover card or the problem
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

  const input = (page) => page.locator('.config-editor__input').first();

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

  test.skip(!built, 'run `npm run build` in js/ first');
  test.use({ viewport: { width: 1000, height: 700 } });

  // With the focus on a documents view's tab or "+", Escape left the
  // popup under the pointer open (it acted only with the focus nowhere).
  test.describe('Escape from a documents tab or the "+" button', () => {
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
          // `port` has a schema: its card lists the problem.
          ['x', '.config-editor__problem-tip-entry'],
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
});
