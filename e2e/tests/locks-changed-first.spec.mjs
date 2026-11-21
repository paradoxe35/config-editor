import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Locks and secrets with real keys on the vanilla page: a path the host
// locks after the user changed it takes every way back to the host's text
// (Backspace, undo, typing it again) and saves again once it reads so; a
// key holding a host range's field renamed a key at a time holds it once,
// under the host's path; a comment after a locked block's header is free;
// and a placeholder cut and pasted over another lands whole.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

async function mount(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.qa = new module.ConfigEditor(host, { language: 'yaml', ...options, onSave: () => {} });
  }, options);
  await input(page).waitFor();
}

async function caretAfter(page, marker) {
  await input(page).evaluate((element, marker) => {
    const at = element.value.indexOf(marker) + marker.length;
    element.focus();
    element.setSelectionRange(at, at);
  }, marker);
}

async function select(page, marker) {
  await input(page).evaluate((element, marker) => {
    const at = element.value.indexOf(marker);
    element.focus();
    element.setSelectionRange(at, at + marker.length);
  }, marker);
}

const text = (page) => page.evaluate(() => window.qa.text);
const held = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll('#qa .config-editor__diagnostic-marker')]
      .map((marker) => marker.getAttribute('data-message') ?? '')
      .filter((message) => message.startsWith('Not saved')),
  );

const VALUE = 'listen: 7835\npublic_address: tunnel.example.com\n';

test.describe('locks after a change, renames, comments and cut placeholders', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  const ways = {
    Backspace: async (page) => {
      await caretAfter(page, 'example.comX');
      await page.keyboard.press('Backspace');
    },
    undo: async (page) => {
      await input(page).focus();
      await page.keyboard.press('Control+z');
    },
    'typing it again': async (page) => {
      await select(page, 'example.comX');
      await page.keyboard.type('example.com');
    },
  };
  for (const [way, back] of Object.entries(ways)) {
    test(`a path locked after it was changed goes back by ${way} and saves`, async ({ page }) => {
      await mount(page, { value: VALUE });
      await caretAfter(page, 'tunnel.example.com');
      await page.keyboard.type('X');
      await page.evaluate(() => window.qa.update({ readOnlyPaths: ['/public_address'] }));
      await expect.poll(() => held(page)).toHaveLength(1);
      expect(await page.evaluate(() => window.qa.save())).toBe(false);
      await back(page);
      await expect.poll(() => text(page)).toBe(VALUE);
      await expect.poll(() => held(page)).toEqual([]);
      expect(await page.evaluate(() => window.qa.save())).toBe(true);
      // Locked again.
      await caretAfter(page, 'tunnel.example.com');
      await page.keyboard.type('Y');
      expect(await text(page)).toBe(VALUE);
    });
  }

  test('a key holding a host range renamed a key at a time holds it once, by the host path', async ({ page }) => {
    const value = 'services:\n  web:\n    image: nginx\n    ports:\n      - "80:80"\n';
    await mount(page, {
      value,
      readOnlyRanges: [{ startLine: 5, startColumn: 1, endLine: 6, endColumn: 1 }],
    });
    await caretAfter(page, '  web');
    await page.keyboard.type('app');
    await expect
      .poll(() => held(page))
      .toEqual([
        'Not saved: a key holding the read-only field /services/web/ports/0 was renamed; give the key its old name back to save',
      ]);
  });

  test('a comment after a locked block header is typed freely', async ({ page }) => {
    const value = 'listen: 7835\nwebsocket:\n  path: /ws\n  ping: 30\n';
    await mount(page, { value, readOnlyPaths: ['/listen', '/websocket'] });
    await caretAfter(page, 'websocket:');
    await page.keyboard.type(' # c');
    await expect.poll(() => text(page)).toBe(value.replace('websocket:', 'websocket: # c'));
  });

  test('a placeholder cut and pasted over another is the whole token, flagged', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    const value = 'tls:\n  key: «secret 3f2a»\n  pin: «secret 0b1c»\n';
    const schema = JSON.stringify({
      properties: {
        tls: {
          properties: { key: { type: 'string', writeOnly: true }, pin: { type: 'string', writeOnly: true } },
        },
      },
    });
    await mount(page, { value, schema });
    await select(page, '«secret 0b1c»');
    await page.keyboard.press('Control+x');
    await select(page, '«secret 3f2a»');
    await page.keyboard.press('Control+v');
    await expect.poll(() => text(page)).toBe('tls:\n  key: «secret 0b1c»\n  pin: \n');
  });
});
