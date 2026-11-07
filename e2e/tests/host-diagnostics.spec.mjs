import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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
const setHost = (browserPage, list) => browserPage.evaluate((problems) => window.editor.setHostDiagnostics(problems), list);

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
    docsUrl: 'about:blank#ports',
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
      { text: '7835', classes: ['docs', 'host', 'warning'], message: 'port in use', detail: 'burrow · E_PORT · Open docs ↗' },
      { text: 'tunnel.example.com', classes: ['host', 'info'], message: 'resolves to 10.0.0.1', detail: 'host' },
      { text: 'insecure_plaintext', classes: ['hint', 'host'], message: 'consider TLS', detail: 'host' },
    ]);
    await expect(footer(browserPage)).toHaveText('1 error, 1 warning, 1 info, 1 hint · the server restarted');
    // The message shows who reported it, under the message.
    const tip = await markers(browserPage).first().evaluate((element) => getComputedStyle(element, '::after').content);
    expect(tip).toContain('port in use');
    expect(tip).toContain('burrow · E_PORT · Open docs ↗');
    // F8 walks them in order.
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+Home');
    const caretLine = () => input(browserPage).evaluate((element) => element.value.slice(0, element.selectionStart).split('\n').length);
    for (const line of [2, 3, 4, 2]) {
      await browserPage.keyboard.press('F8');
      await expect.poll(caretLine).toBe(line);
    }
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
    // In TOML the paths still find their fields; the range, given in the
    // YAML text, shows unplaced.
    await browserPage.locator('#edit-as').selectOption('toml');
    await expect(input(browserPage)).toHaveValue(/listen = 7835/);
    await expect(markers(browserPage)).toHaveCount(2);
    expect((await shown(browserPage)).map((marker) => marker.text)).toEqual(['7835', '"tunnel.example.com"']);
    await expect(footer(browserPage)).toContainText('consider TLS');
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
    await marker.hover();
    const box = await marker.boundingBox();
    // Near the top of the editor, the message opens below the line.
    const popup = browserPage.waitForEvent('popup');
    await browserPage.mouse.move(box.x + 4, box.y + box.height + 10);
    await browserPage.mouse.click(box.x + 4, box.y + box.height + 10);
    await popup;
  });

  test('bad host diagnostics throw, saying which and why', async ({ page: browserPage }) => {
    await open(browserPage, 'vanilla.html');
    await expect(setHost(browserPage, [{ path: '/listen' }])).rejects.toThrow(/hostDiagnostics\[0\]: message must be a string/);
    await expect(setHost(browserPage, [{ message: 'x', severity: 'fatal' }])).rejects.toThrow(/unknown severity "fatal"/);
    await expect(setHost(browserPage, [{ message: 'x', range: { line: 1 } }])).rejects.toThrow(/range must be/);
    await expect(setHost(browserPage, {})).rejects.toThrow(/must be an array/);
  });
});

test.describe('react.html', () => {
  test('the hostDiagnostics prop updates the markers', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'react.html');
    await browserPage.locator('#host-problems').click();
    await expect(markers(browserPage)).toHaveCount(1);
    await expect(markers(browserPage)).toHaveAttribute('data-detail', 'server');
    await expect(markers(browserPage)).toHaveClass(/diagnostic-marker-warning/);
    await browserPage.locator('#host-problems').click();
    await expect(markers(browserPage)).toHaveCount(0);
    expect(errors).toEqual([]);
  });
});
