import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setupEditor } from './helpers.mjs';

// Focus is never dropped to BODY when the focused control goes away: a
// footer count emptied or removed by a host text (D13.a11y-2), a change
// preview a host push edits away (D18.a11y-1), a fold chip or chevron a host
// update moves (D15.a11y-1), the Edit-as notice's Dismiss (D12.a11y-2), the
// demo's "Apply changes" (D20.a11y-1), and, in a documents view, tabs the host
// reorders (D9.a11y-1), an unselected tab closed (D16.a11y-2) and the last
// tab closed (D22.a11y-1). One keeper (`focus::keep_focus`) serves them all.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name, options) =>
  `${pathToFileURL(join(js, 'examples', name)).href}${options ? `?options=${encodeURIComponent(JSON.stringify(options))}` : ''}`;

const input = (page) => page.locator('.config-editor__input').first();
const active = (page) =>
  page.evaluate(() => {
    const element = document.activeElement;
    return {
      tag: element?.tagName,
      cls: String(element?.className ?? ''),
      role: element?.getAttribute('role') ?? null,
      id: element?.getAttribute('data-document-id') ?? null,
    };
  });
const onText = (page) => expect.poll(async () => (await active(page)).cls).toContain('config-editor__input');

async function openExample(page, name, options) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(example(name, options));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await input(page).waitFor();
}

const settle = (page) => page.waitForTimeout(700);
/** Script focus, as assistive technology's virtual cursor does. */
const focusOn = (page, selector) =>
  page
    .locator(selector)
    .first()
    .evaluate((element) => element.focus());

const FOLDED = 'a:\n  x: 1\n  y: 2\nb:\n  c: 3\n  d: 4\n';
const MOVED = `# top\n\n${FOLDED}`;
const TUNNEL = '# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n';

test.describe('a control of the editor that goes while it has the focus', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test.beforeEach(async ({ page }) => openExample(page, 'vanilla.html'));

  // D13.a11y-2
  test('the problem status emptied by a host text hands the focus to the text', async ({ page }) => {
    await page.evaluate(() => window.editor.update({ value: 'a: [1\n' }));
    const status = page.locator('.config-editor__diagnostic-status');
    await expect(status).toBeEnabled();
    await focusOn(page, '.config-editor__diagnostic-status');
    expect((await active(page)).cls).toContain('diagnostic-status');
    await page.evaluate((value) => window.editor.update({ value }), TUNNEL);
    await expect(status).toBeDisabled();
    await onText(page);
  });

  // D13.a11y-2
  test('the change count removed by a host text hands the focus to the text', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# note\n');
    const count = page.locator('.config-editor__change-status');
    await expect(count).toHaveText('1 change');
    await focusOn(page, '.config-editor__change-status');
    expect((await active(page)).cls).toContain('change-status');
    await page.evaluate((value) => window.editor.update({ value }), TUNNEL);
    await expect(count).toHaveCount(0);
    await onText(page);
  });

  // D18.a11y-1
  test('a change preview a host push edits away hands the focus to the text', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('x: 1\n');
    await expect(page.locator('.config-editor__change-status')).toHaveText('1 change');
    await page.keyboard.press('Alt+F3');
    const peek = page.locator('.config-editor__diff-peek');
    await expect(peek).toHaveCount(1);
    expect(await peek.evaluate((element) => element.contains(document.activeElement))).toBe(true);
    await page.evaluate((value) => window.editor.update({ value: `${value}x: 2\n` }), TUNNEL);
    await expect(peek).toHaveCount(0);
    await onText(page);
  });

  // D15.a11y-1
  for (const [label, selector, value] of [
    ['a chip a host update moves', '.config-editor__fold-chip', MOVED],
    ['a chip a host update removes', '.config-editor__fold-chip', 'b:\n  c: 3\n  d: 4\n'],
    ['a chevron a host update moves', '.config-editor__fold-toggle', MOVED],
  ]) {
    test(`${label} hands the focus to the text`, async ({ page }) => {
      await page.evaluate((value) => window.editor.update({ value }), FOLDED);
      await input(page).click();
      await page.keyboard.press('Control+Home');
      await page.keyboard.press('Control+Shift+BracketLeft');
      await expect(page.locator('.config-editor__fold-chip')).toHaveCount(1);
      await focusOn(page, selector);
      expect((await active(page)).cls).toContain('fold');
      await page.evaluate((value) => window.editor.update({ value }), value);
      await settle(page);
      await onText(page);
    });
  }

  test('focus that left the editor on purpose is not taken back by a later change', async ({ page }) => {
    await page.evaluate(() => window.editor.update({ value: 'a: [1\n' }));
    await focusOn(page, '.config-editor__diagnostic-status');
    await page.locator('#saved').click();
    await page.evaluate((value) => window.editor.update({ value }), TUNNEL);
    await settle(page);
    expect((await active(page)).tag).toBe('BODY');
  });
});

// D12.a11y-2, D20.a11y-1: the demo.
test.describe('the demo: the Edit-as notice', () => {
  setupEditor(test, '?edit=toml');

  test('Dismiss from the keyboard hands the focus to the text', async ({ page }) => {
    const dismiss = page.locator('.config-editor__edit-as-notice-dismiss');
    await dismiss.evaluate((element) => element.focus());
    await page.keyboard.press('Enter');
    await expect(page.locator('.config-editor__edit-as-notice')).toBeHidden();
    await onText(page);
  });
});

test.describe('the demo: Apply changes', () => {
  setupEditor(test, '?sample=large');

  test('Space on it hands the focus to the text as it disables itself', async ({ page }) => {
    await input(page).click();
    await page.keyboard.press('Control+End');
    await page.keyboard.type('# note');
    const apply = page.getByRole('button', { name: 'Apply changes' });
    await expect(apply).toBeEnabled();
    await apply.evaluate((element) => element.focus());
    await page.keyboard.press('Space');
    await expect(apply).toBeDisabled();
    await onText(page);
  });
});

// D9.a11y-1, D16.a11y-2, D22.a11y-1: tabs.
test.describe('documents: a tab that goes or moves while it has the focus', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  test.beforeEach(async ({ page }) => openExample(page, 'documents.html'));

  const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);

  // D9.a11y-1
  for (const [label, focus, check] of [
    ['the selected tab', '[role="tab"][data-document-id="main"]', (state) => state.id === 'main' && state.role === 'tab'],
    [
      'the text of the shown document',
      '[role="tabpanel"]:not([inert]) .config-editor__input',
      (state) => state.cls.includes('config-editor__input'),
    ],
  ]) {
    test(`a host reorder keeps the focus on ${label}`, async ({ page }) => {
      await tab(page, 'main').click();
      await focusOn(page, focus);
      await page.evaluate(async () => {
        const schema = await (await fetch('../../schema.json')).text();
        const file = (id, fileName, value, extra = {}) => ({ id, fileName, value, schema, ...extra });
        window.documentsView.update({
          documents: [
            file('limits', 'conf.d/limits.toml', '[limits]\nmax_pending = 512\n'),
            file('main', 'tunnel.yaml', 'listen: 7835\npublic_address: tunnel.example.com\n', { closable: false }),
            file('clients', 'conf.d/clients.json', '{\n  "clients": {}\n}\n', { readOnly: true }),
          ],
        });
      });
      await expect.poll(() => page.locator('[role="tab"]').first().getAttribute('data-document-id')).toBe('limits');
      await settle(page);
      expect(check(await active(page))).toBe(true);
    });
  }

  // D16.a11y-2
  test('closing an unselected focused tab hands the focus to the selected tab', async ({ page }) => {
    await tab(page, 'main').click();
    await focusOn(page, '[role="tab"][data-document-id="limits"]');
    await page.keyboard.press('Delete');
    await expect(tab(page, 'limits')).toHaveCount(0);
    await expect.poll(async () => (await active(page)).id).toBe('main');
    expect((await active(page)).role).toBe('tab');
  });

  // D22.a11y-1
  test('closing the last document hands the focus to the note of the empty view', async ({ page }) => {
    await page.evaluate(() => window.updateFile('main', { closable: true }));
    await tab(page, 'main').click();
    await focusOn(page, '[role="tab"][aria-selected="true"]');
    for (let count = 3; count > 0; count -= 1) {
      await page.keyboard.press('Delete');
      await expect(page.locator('[role="tab"]')).toHaveCount(count - 1);
    }
    const note = page.locator('.config-editor-documents__empty');
    await expect(note).toBeVisible();
    await expect.poll(async () => (await active(page)).cls).toContain('config-editor-documents__empty');
  });
});
