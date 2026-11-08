import { expect, test } from '@playwright/test';
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, extname, join, normalize, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pointAt, pointerOn } from './helpers.mjs';

// The editor under a strict Content-Security-Policy, as a host's admin UI
// serves it: no inline styles or scripts (`style-src 'self'`), the wasm
// compiled with `'wasm-unsafe-eval'`. The pages (e2e/csp) run the editor
// with `styled: false` and link the preset as a file; any refused style or
// script (a `securitypolicyviolation` event, or Chrome's console error)
// fails the test while it types, shows problems, hovers, suggests, finds,
// goes to a line, folds, previews and reverts a change, shows the read-only
// and secret tips, edits as another language, switches tabs (with the "+"
// and close buttons) and sets colors.

export const CSP = "default-src 'self'; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'";

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const dist = join(root, 'dist');
const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
};

// The trunk build's hashed files, under fixed names.
function distFile(pattern) {
  const name = existsSync(dist) && readdirSync(dist).find((file) => pattern.test(file));
  return name ? join(dist, name) : null;
}
const LEPTOS = {
  '/leptos/demo.js': () => distFile(/^config-editor-demo-.*\.js$/),
  '/leptos/demo_bg.wasm': () => distFile(/^config-editor-demo-.*_bg\.wasm$/),
  '/leptos/demo.css': () => distFile(/\.css$/),
};

// Serves the repository with the CSP header on every response.
function serve() {
  const server = createServer((request, response) => {
    const path = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const file = LEPTOS[path]?.() ?? normalize(join(root, path));
    if (!file || !file.startsWith(root + sep) || !existsSync(file) || !statSync(file).isFile()) {
      response.writeHead(404, { 'Content-Security-Policy': CSP }).end();
      return;
    }
    response.writeHead(200, {
      'Content-Type': TYPES[extname(file)] ?? 'application/octet-stream',
      'Content-Security-Policy': CSP,
      'Cache-Control': 'no-store',
    });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

let server;
let origin;
test.beforeAll(async () => {
  server = await serve();
  origin = `http://127.0.0.1:${server.address().port}`;
});
test.afterAll(() => new Promise((resolve) => server.close(resolve)));

// Opens `path` and records every CSP violation and page error on the page.
async function open(page, path) {
  page.refused = [];
  await page.addInitScript(() => {
    window.refusedByCsp = [];
    document.addEventListener('securitypolicyviolation', (event) =>
      window.refusedByCsp.push(`${event.violatedDirective}: ${event.blockedURI} ${event.sample}`),
    );
  });
  page.on('pageerror', (error) => page.refused.push(String(error)));
  page.on('console', (message) => {
    if (message.type() === 'error') page.refused.push(message.text());
  });
  await page.goto(`${origin}${path}`);
  await page.locator('.config-editor__input').first().waitFor();
}

async function expectNothingRefused(page) {
  const logged = [...page.refused];
  const probe = logged.findIndex((line) => /inline style/i.test(line));
  if (page.probed && probe >= 0) logged.splice(probe, 1);
  const refused = [...logged, ...(await page.evaluate(() => window.refusedByCsp))];
  expect(refused, 'nothing refused by the Content-Security-Policy').toEqual([]);
}

// The page really runs under the policy: a style attribute set as text is
// refused, and the preset came from the stylesheet file.
async function expectPolicyAndPreset(page) {
  expect(await page.evaluate(() => document.querySelectorAll('.config-editor style').length), 'no injected <style>').toBe(0);
  expect(await page.locator('.config-editor__input-wrap').first().evaluate((element) => getComputedStyle(element).position)).toBe(
    'relative',
  );
  const refusedBefore = await page.evaluate(() => {
    const probe = document.createElement('div');
    probe.setAttribute('style', 'color: red');
    document.body.append(probe);
    const applied = getComputedStyle(probe).color === 'rgb(255, 0, 0)';
    probe.remove();
    return applied;
  });
  expect(refusedBefore, 'a text style attribute is not applied').toBe(false);
  await expect.poll(() => page.evaluate(() => window.refusedByCsp.length)).toBeGreaterThan(0);
  await page.evaluate(() => (window.refusedByCsp.length = 0));
  // Chrome also logs that refusal; it is the only one allowed.
  page.probed = true;
}

async function replaceText(page, text) {
  const input = page.locator('.config-editor__input');
  await input.click();
  await page.keyboard.press('Control+A');
  await page.keyboard.press('Delete');
  await input.evaluate((element, value) => {
    element.setRangeText(value, 0, element.value.length, 'end');
    element.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertFromPaste', data: value }));
  }, text);
  await page.waitForTimeout(150);
}

async function caretAtEnd(page, line) {
  await page.locator('.config-editor__input').evaluate((element, target) => {
    const lines = element.value.split('\n');
    const offset = lines.slice(0, target).reduce((sum, text) => sum + text.length + 1, 0) + lines[target].length;
    element.focus();
    element.setSelectionRange(offset, offset);
  }, line);
}

// Typing, schema problems, hover, suggestions, find/replace, go to line,
// folding, the change preview and revert: the same on every page.
async function exerciseEditor(page) {
  const input = page.locator('.config-editor__input');
  await replaceText(page, 'listen: 7835\ninsecure_plaintext: false\nclients:\n  acme:\n    secret: c2VjcmV0\n');
  // Typing makes a schema problem (a string for a boolean); hovering its
  // marker shows the tip.
  await caretAtEnd(page, 1);
  await page.keyboard.type(' nope');
  const marker = page.locator('.config-editor__diagnostic-marker').first();
  await expect(marker).toBeVisible();
  await pointerOn(page, marker);
  await expect(page.locator('.config-editor__problem-tip')).toBeVisible();
  await page.mouse.move(0, 0);
  // Hover docs on a key.
  const key = await pointAt(page, 0, 2);
  await page.mouse.move(key.x, key.y, { steps: 3 });
  await expect(page.locator('.config-editor__hover')).toBeVisible();
  await page.mouse.move(0, 0);
  // Suggestions on an empty line.
  await caretAtEnd(page, 4);
  await page.keyboard.press('Enter');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Control+Space');
  await expect(page.locator('.config-editor__completions')).not.toHaveClass(/hidden/);
  await expect(page.locator('.config-editor__completion').first()).toBeVisible();
  await page.keyboard.press('Escape');
  // Find and replace, then go to line.
  await page.keyboard.press('Control+H');
  const find = page.locator('.config-editor__find');
  await expect(find).toBeVisible();
  await page.keyboard.type('acme');
  await expect(page.locator('.config-editor__search-match').first()).toBeVisible();
  await page.keyboard.press('Escape');
  await input.focus();
  await page.keyboard.press('Control+G');
  await expect(page.locator('.config-editor__goto')).toBeVisible();
  await page.keyboard.type('2');
  await page.keyboard.press('Enter');
  await expect(page.locator('.config-editor__goto')).toBeHidden();
  // Fold a block, then open it from its chip.
  await page.locator('.config-editor__fold-toggle[data-line="3"]').click();
  const chip = page.locator('.config-editor__fold-chip');
  await expect(chip).toBeVisible();
  await chip.click();
  await expect(chip).toHaveCount(0);
  // The change preview, and Revert.
  const changed = page.locator('.config-editor__diff-marker-modified, .config-editor__diff-marker-added').first();
  await changed.click();
  const peek = page.locator('.config-editor__diff-peek');
  await expect(peek).toBeVisible();
  await peek.getByRole('button', { name: /Revert this change/ }).click();
  await expect(peek).toHaveCount(0);
}

// The read-only tip (an edit refused) and a secret's tip (hovered).
async function exerciseTips(page, { locked, secret }) {
  const tip = page.locator('.config-editor__protect-tip');
  const lines = (await page.locator('.config-editor__input').inputValue()).split('\n');
  const lockedLine = lines.findIndex((line) => line.includes(locked));
  const at = await pointAt(page, lockedLine, lines[lockedLine].indexOf(locked) + 2);
  await page.mouse.click(at.x, at.y);
  await page.keyboard.type('x');
  await expect(tip).toBeVisible();
  const secretLine = lines.findIndex((line) => line.includes(secret));
  await page.mouse.move(0, 0);
  const over = await pointAt(page, secretLine, lines[secretLine].indexOf(secret) + 4);
  await page.mouse.move(over.x, over.y, { steps: 4 });
  await expect(tip).toBeVisible();
}

async function expectColors(page, selector) {
  const key = await page
    .locator(selector)
    .first()
    .evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-key').trim());
  expect(key).toBe('#ffcc66');
}

// Every theme from the one preset file: each gives the editor its own
// colors; custom colors stay over a switch.
async function exerciseThemes(page, names, setTheme, colors) {
  const input = page.locator('.config-editor__input');
  const shown = () =>
    input.evaluate((element) => {
      const style = getComputedStyle(element);
      const value = (name) => style.getPropertyValue(name).trim();
      return `${value('--config-editor-bg')} ${value('--config-editor-key')} ${value('--config-editor-string')}`;
    });
  expect(names.length).toBe(11);
  const seen = new Set();
  for (const name of names) {
    await setTheme(name);
    await expect(page.locator('.config-editor[data-color-theme]:has(.config-editor__input)')).toHaveAttribute('data-color-theme', name);
    const colorsShown = await shown();
    expect(colorsShown.split(' ').every(Boolean), `${name} sets its colors`).toBe(true);
    seen.add(colorsShown);
  }
  expect(seen.size, 'every theme looks its own').toBe(names.length);
  await colors();
  for (const name of names.slice(0, 3)) {
    await setTheme(name);
    expect(await input.evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-key').trim())).toBe('#ffcc66');
  }
}

async function exerciseDocuments(page) {
  await expectColors(page, '[role="tablist"]');
  const tabs = page.locator('[role="tab"]');
  await tabs.nth(1).click();
  await expect(tabs.nth(1)).toHaveAttribute('aria-selected', 'true');
  await page.locator('[role="tabpanel"]:visible .config-editor__input').click();
  await page.keyboard.type('x');
  await tabs.nth(0).click();
  await expect(tabs.nth(0)).toHaveAttribute('aria-selected', 'true');
  // The "+" and a close button, hovered and focused.
  const add = page.locator('.config-editor-documents__add');
  await expect(add).toBeVisible();
  await add.hover();
  await add.focus();
  const close = page.locator('.config-editor-documents__close').first();
  await close.hover();
  await expect(close).toBeVisible();
  await add.click();
}

test.describe('under a strict CSP: the Leptos demo', () => {
  test.skip(!existsSync(dist), 'needs the trunk build (dist/)');

  test('editing, problems, popups, folding and the change preview with custom colors', async ({ page }) => {
    await open(page, '/e2e/csp/leptos.html?styled=0&colors=custom');
    await expectPolicyAndPreset(page);
    await expectColors(page, '.config-editor__input');
    await exerciseEditor(page);
    await expectNothingRefused(page);
  });

  test('every theme from the one preset file, with and without custom colors', async ({ page }) => {
    await open(page, '/e2e/csp/leptos.html?styled=0');
    const select = page.locator('.demo__theme-select');
    const names = await select.locator('option').evaluateAll((options) => options.map((option) => option.value));
    await exerciseThemes(
      page,
      names,
      (name) => select.selectOption(name),
      () => open(page, '/e2e/csp/leptos.html?styled=0&colors=custom'),
    );
    await expectNothingRefused(page);
  });

  test('the read-only and secret tips', async ({ page }) => {
    await open(page, '/e2e/csp/leptos.html?styled=0&schema=protected');
    await exerciseTips(page, { locked: 'srv-42', secret: '«secret 3f2a»' });
    await expectNothingRefused(page);
  });

  test('edit as another language', async ({ page }) => {
    await open(page, '/e2e/csp/leptos.html?styled=0&edit=json');
    await expect.poll(() => page.locator('.config-editor__input').inputValue()).toMatch(/^\{/);
    await page.locator('.config-editor__input').click();
    await page.keyboard.type(' ');
    await expectNothingRefused(page);
  });

  test('documents as tabs', async ({ page }) => {
    await open(page, '/e2e/csp/leptos.html?documents&styled=0&colors=custom');
    await exerciseDocuments(page);
    await expectNothingRefused(page);
  });
});

const PACKAGE_BUILT = existsSync(join(root, 'js/config-editor/pkg/config_editor_bg.wasm'));

test.describe('under a strict CSP: the JS package (vanilla)', () => {
  test.skip(!PACKAGE_BUILT, 'needs the npm package build (make packages)');

  test('host and schema problems, tips, editing, edit as, colors set and removed', async ({ page }) => {
    await open(page, '/e2e/csp/vanilla.html');
    await expectPolicyAndPreset(page);
    await expectColors(page, '.config-editor__input');
    await expect(page.locator('.config-editor__diagnostic-marker-host')).toHaveCount(1);
    await exerciseTips(page, { locked: 'srv-42', secret: '«secret 3f2a»' });
    // The locked field holds against the user's edits, not the host's.
    await page.evaluate(() => window.editor.update({ value: '' }));
    await exerciseEditor(page);
    await page.evaluate(() => window.editor.update({ editAs: 'json' }));
    await expect.poll(() => page.locator('.config-editor__input').inputValue()).toMatch(/^\{/);
    // A color no longer given goes.
    await page.evaluate(() => window.editor.update({ colors: { key: '#ffcc66' } }));
    const root = page.locator('.config-editor:has(.config-editor__input)');
    await expect.poll(() => root.evaluate((element) => element.style.getPropertyValue('--config-editor-bg'))).toBe('');
    expect(await root.evaluate((element) => element.style.getPropertyValue('--config-editor-key'))).toBe('#ffcc66');
    await expectNothingRefused(page);
  });

  test('every theme from the one preset file, with and without custom colors', async ({ page }) => {
    await open(page, '/e2e/csp/vanilla.html');
    await page.evaluate(() => window.editor.update({ colors: {} }));
    await exerciseThemes(
      page,
      await page.evaluate(() => window.themeNames),
      (name) => page.evaluate((theme) => window.editor.update({ theme }), name),
      () => page.evaluate(() => window.editor.update({ colors: { key: '#ffcc66' } })),
    );
    await expectNothingRefused(page);
  });

  test('documents as tabs', async ({ page }) => {
    await open(page, '/e2e/csp/vanilla.html?view=documents');
    await expectPolicyAndPreset(page);
    await exerciseDocuments(page);
    await expectNothingRefused(page);
  });
});

for (const framework of ['react', 'vue', 'svelte']) {
  test.describe(`under a strict CSP: the ${framework} binding`, () => {
    const built = existsSync(join(root, `js/examples/dist/csp-${framework}.js`));
    test.skip(!PACKAGE_BUILT || !built, 'needs the npm packages and their examples (make packages)');
    const path = (query) => `/e2e/csp/framework.html?framework=${framework}&${query}`;

    test('host and schema problems, tips and colors', async ({ page }) => {
      await open(page, path(''));
      await expectPolicyAndPreset(page);
      await expectColors(page, '.config-editor__input');
      await expect(page.locator('.config-editor__diagnostic-marker-host')).toHaveCount(1);
      await exerciseTips(page, { locked: 'srv-42', secret: '«secret 3f2a»' });
      await expectNothingRefused(page);
    });

    test('editing, popups, folding and the change preview', async ({ page }) => {
      await open(page, path('plain'));
      await exerciseEditor(page);
      await expectNothingRefused(page);
    });

    test('edit as another language', async ({ page }) => {
      await open(page, path('edit=json'));
      await expect.poll(() => page.locator('.config-editor__input').inputValue()).toMatch(/^\{/);
      await expectNothingRefused(page);
    });

    test('documents as tabs', async ({ page }) => {
      await open(page, path('view=documents'));
      await expectPolicyAndPreset(page);
      await exerciseDocuments(page);
      await expectNothingRefused(page);
    });
  });
}
