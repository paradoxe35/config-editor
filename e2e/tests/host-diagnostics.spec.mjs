import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES, pointerOn } from './helpers.mjs';

// Host diagnostics (`hostDiagnostics`, `setHostDiagnostics`): a server's
// problems shown with the editor's own, through the npm packages' example
// pages. Build them first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');
const markers = (browserPage) => browserPage.locator('.config-editor__diagnostic-marker');
const footer = (browserPage) => browserPage.locator('.config-editor__diagnostic-status');
const setHost = (browserPage, list) => browserPage.evaluate((problems) => window.editor.update({ hostDiagnostics: problems }), list);

// The vanilla page's document:
//   1 # Burrow server configuration
//   2 listen: 7835
//   3 public_address: tunnel.example.com
//   4 insecure_plaintext: false
const PROBLEMS = [
  {
    path: '/listen',
    severity: 'warning',
    message: 'port in use',
    code: 'E_PORT',
    source: 'burrow',
    docsUrl: 'https://example.com/docs/ports',
  },
  { path: ['public_address'], severity: 'info', message: 'resolves to 10.0.0.1' },
  { range: { startLine: 4, startColumn: 1, endColumn: 19 }, severity: 'hint', message: 'consider TLS' },
  { message: 'the server restarted' },
];

// What each marker underlines, with its classes and messages.
const shown = (browserPage) =>
  markers(browserPage).evaluateAll((elements) =>
    elements.map((element) => ({
      text: element.textContent,
      classes: [...element.classList]
        .filter((name) => /-(warning|info|hint|host|docs)$/.test(name))
        .map((name) => name.replace('config-editor__diagnostic-marker-', ''))
        .sort(),
      message: element.dataset.message,
      detail: element.dataset.detail ?? null,
    })),
  );

test.describe('vanilla.html', () => {
  test('host problems are marked by path and range, counted, and walked with F8', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await setHost(browserPage, PROBLEMS);
    await expect(markers(browserPage)).toHaveCount(3);
    expect(await shown(browserPage)).toEqual([
      { text: '7835', classes: ['docs', 'host', 'warning'], message: 'port in use', detail: 'burrow · E_PORT · Open docs ↗ (Alt+Enter)' },
      { text: 'tunnel.example.com', classes: ['host', 'info'], message: 'resolves to 10.0.0.1', detail: 'host' },
      { text: 'insecure_plaintext', classes: ['hint', 'host'], message: 'consider TLS', detail: 'host' },
    ]);
    await expect(footer(browserPage)).toHaveText('1 error, 1 warning, 1 info, 1 hint · the server restarted');
    // The message shows who reported it, under the message.
    await pointerOn(browserPage, markers(browserPage).first());
    const tip = browserPage.locator('.config-editor__problem-tip');
    await expect(tip).toContainText('port in use');
    await expect(tip).toContainText('burrow · E_PORT · Open docs ↗ (Alt+Enter)');
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+Home');
    const caretLine = () => input(browserPage).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
    // F8 walks them in order, then the whole document's (the caret stays).
    for (const line of [2, 3, 4, 4, 2]) {
      await browserPage.keyboard.press('F8');
      await expect.poll(caretLine).toBe(line);
    }
    // Back: the whole document's, then the last placed one.
    await browserPage.keyboard.press('Shift+F8');
    await expect(browserPage.locator('.config-editor__announcement')).toContainText('the server restarted');
    await browserPage.keyboard.press('Shift+F8');
    await expect.poll(caretLine).toBe(4);
    expect(errors).toEqual([]);
  });

  test('a new list replaces the old one without re-rendering the lines', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    // Tag every rendered line: a re-render would replace the elements.
    await browserPage.locator('.config-editor__code-line').evaluateAll((lines) => lines.forEach((line) => (line.dataset.tag = 'kept')));
    await setHost(browserPage, PROBLEMS);
    await expect(markers(browserPage)).toHaveCount(3);
    await setHost(browserPage, [{ path: '/insecure_plaintext', message: 'must be true' }]);
    await expect(markers(browserPage)).toHaveCount(1);
    await expect(markers(browserPage)).toHaveText('false');
    await expect(footer(browserPage)).toHaveText(/^1 error ·/);
    const untagged = await browserPage
      .locator('.config-editor__code-line')
      .evaluateAll((lines) => lines.filter((line) => line.dataset.tag !== 'kept').length);
    expect(untagged).toBe(0);
    await setHost(browserPage, []);
    await expect(markers(browserPage)).toHaveCount(0);
    await expect(footer(browserPage)).toHaveText('No errors');
  });

  test('a path follows its field through edits and edit as', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await setHost(browserPage, PROBLEMS.slice(0, 3));
    await expect(markers(browserPage)).toHaveCount(3);
    // A line inserted above moves the marker with its field.
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+Home');
    await browserPage.keyboard.press('End');
    await browserPage.keyboard.press('Enter');
    await browserPage.keyboard.type('# added');
    await browserPage.keyboard.press('Escape');
    await expect
      .poll(() => markers(browserPage).first().evaluate((element) => element.style.top))
      .toContain('2 * var(--config-editor-line-height)');
    await expect(markers(browserPage).first()).toHaveText('7835');
    // In TOML the paths still find their fields, and the range, given in the
    // YAML text (line 4: `public_address` since the line added), marks the
    // key it starts on there.
    await browserPage.locator('#edit-as').selectOption('toml');
    await expect(input(browserPage)).toHaveValue(/listen = 7835/);
    await expect(markers(browserPage)).toHaveCount(3);
    expect((await shown(browserPage)).map((marker) => marker.text)).toEqual(['7835', '"tunnel.example.com"', 'public_address']);
    await expect(footer(browserPage)).not.toContainText('consider TLS');
  });

  test('the host problem replaces the editor’s own on the same value', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await input(browserPage).evaluate((element) => {
      const at = element.value.indexOf('7835');
      element.focus();
      element.setSelectionRange(at, at + 4);
    });
    await browserPage.keyboard.type('true');
    await expect(markers(browserPage)).toHaveCount(1);
    await expect(markers(browserPage)).not.toHaveAttribute('data-detail');
    await setHost(browserPage, [{ path: '/listen', message: 'listen must be a port number' }]);
    await expect(markers(browserPage)).toHaveCount(1);
    await expect(markers(browserPage)).toHaveAttribute('data-message', 'listen must be a port number');
    await expect(footer(browserPage)).toHaveText(/^1 error ·/);
  });

  test('a click on the message of a problem with docs opens them', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await setHost(browserPage, PROBLEMS.slice(0, 1));
    const marker = markers(browserPage).first();
    await pointerOn(browserPage, marker);
    const box = await marker.boundingBox();
    // Near the top of the editor, the message opens below the line; the
    // pointer moving onto it keeps it open.
    const message = browserPage.locator('.config-editor__problem-tip-message');
    const tip = await message.boundingBox();
    expect(tip.y).toBeGreaterThan(box.y + box.height);
    const popup = browserPage.waitForEvent('popup');
    await browserPage.mouse.move(box.x + 4, tip.y + tip.height / 2, { steps: 4 });
    await browserPage.mouse.click(tip.x + 10, tip.y + tip.height / 2);
    await popup;
  });

  test('the keyboard reaches a problem’s code and docs: the caret’s note and F8 say them, Alt+Enter opens the docs', async ({
    page: browserPage,
  }) => {
    await open(browserPage, 'vanilla.html');
    await setHost(browserPage, PROBLEMS);
    await expect(markers(browserPage)).toHaveCount(3);
    // The caret on `7835` (line 2), from the keyboard.
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+Home');
    await browserPage.keyboard.press('ArrowDown');
    await browserPage.keyboard.press('End');
    const note = browserPage.locator(`#${await input(browserPage).getAttribute('aria-describedby')}`);
    await expect(note).toContainText('port in use');
    await expect(note).toContainText('E_PORT');
    await expect(note).toContainText('https://example.com/docs/ports');
    await browserPage.keyboard.press('Control+Home');
    await browserPage.keyboard.press('F8');
    await expect(browserPage.locator('.config-editor__announcement')).toContainText('E_PORT');
    const popup = browserPage.waitForEvent('popup');
    await browserPage.keyboard.press('Alt+Enter');
    // The docs page opens (offline it shows an error page for that address).
    const opened = await popup;
    await expect.poll(() => opened.url()).not.toBe('about:blank');
    // Nothing was typed.
    expect(await input(browserPage).inputValue()).toContain('listen: 7835\n');
  });

  test('bad host diagnostics throw, saying which and why', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await expect(setHost(browserPage, [{ path: '/listen' }])).rejects.toThrow(/hostDiagnostics\[0\]: message must be a string/);
    await expect(setHost(browserPage, [{ message: 'x', severity: 'fatal' }])).rejects.toThrow(/unknown severity "fatal"/);
    await expect(setHost(browserPage, [{ message: 'x', range: { line: 1 } }])).rejects.toThrow(/range must be/);
    await expect(setHost(browserPage, {})).rejects.toThrow(/must be an array/);
    // Round 16-18 leftovers: each mistake throws, saying why.
    await expect(setHost(browserPage, [{ message: 'x', docsUrl: 'http://' }])).rejects.toThrow(/docsUrl must be an http\(s\) URL with a host/);
    await expect(setHost(browserPage, [{ message: 'x', docsUrl: 'https:///docs' }])).rejects.toThrow(/docsUrl must be/);
    await expect(setHost(browserPage, [{ message: 'x', severity: 'ERROR' }])).rejects.toThrow(/unknown severity "ERROR" \(expected "error", "warning", "info" or "hint", lower-case\)/);
    await expect(setHost(browserPage, [{ message: '' }])).rejects.toThrow(/hostDiagnostics\[0\]: message must not be empty/);
    await expect(setHost(browserPage, [{ message: 'x', range: { start: 2 } }])).rejects.toThrow(/range must be \{ start, end \}/);
    await expect(setHost(browserPage, [{ message: 'x', range: { start: 0, end: 3, startLine: 2, startColumn: 1 } }])).rejects.toThrow(/different stretches/);
    // Both styles for one stretch, as rangeOf returns them, are fine.
    await setHost(browserPage, [{ message: 'x', range: { start: 0, end: 1, startLine: 1, startColumn: 1, endLine: 1, endColumn: 2 } }]);
    await expect(markers(browserPage)).toHaveCount(1);
    await setHost(browserPage, [{ message: 'x', docsUrl: 'https://example.com/docs' }]);
  });

  test('bad secrets options throw; a throwing isPlaceholder is logged', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const result = await browserPage.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.append(host);
      let prefixes = null;
      try {
        new ConfigEditor(host, { language: 'yaml', value: 'a: 1\n', secrets: { referencePrefixes: [''] } });
      } catch (error) {
        prefixes = error.message;
      }
      const editor = new ConfigEditor(host, {
        language: 'yaml',
        value: 'pw: «secret 3f2a»\n',
        schema: JSON.stringify({ type: 'object', properties: { pw: { type: 'string', writeOnly: true } } }),
        secrets: { isPlaceholder: () => { throw new Error('boom'); } },
      });
      await new Promise((resolve) => setTimeout(resolve, 300));
      editor.destroy();
      return { prefixes };
    });
    expect(result.prefixes).toMatch(/secrets\.referencePrefixes must be an array of non-empty strings/);
    expect(errors.some((error) => /config-editor: a callback threw/.test(error))).toBe(true);
  });
});

for (const name of FRAMEWORK_PAGES) test.describe(name, () => {
  test('the hostDiagnostics prop updates the markers', async ({ page: browserPage }) => {
    const errors = await open(browserPage, name);
    await browserPage.locator('#host-problems').click();
    await expect(markers(browserPage)).toHaveCount(1);
    await expect(markers(browserPage)).toHaveAttribute('data-detail', 'server');
    await expect(markers(browserPage)).toHaveClass(/diagnostic-marker-warning/);
    await browserPage.locator('#host-problems').click();
    await expect(markers(browserPage)).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});
