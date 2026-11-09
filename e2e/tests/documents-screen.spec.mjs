import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES, setupEditor } from './helpers.mjs';

// What a `ConfigDocuments` tells its host of each document's screen:
// `on_edit_as_change` (the language on screen: once when the editor mounts,
// then on each change) and `on_sync_change` (whether the text on screen is
// written back to the host's value), each exactly when the screen changes;
// `DocumentsHandle::edit_as` asking for a language again; a document opened
// from the text a host kept (`edit_as_text`); and a read-only document's
// reason (`read_only_reason`) on its tab and in its editor.
//
// The demo's `?documents` page logs the callbacks on its status line
// (`data-languages`: `id:Language,…`, `data-syncs`: `id:true,…`); the npm
// example pages log `language <id> <name>` and `sync <id> <bool>` lines.

const tab = (page, id) => page.locator(`[role="tab"][data-document-id="${id}"]`);
const panel = (page, id) => page.locator(`[role="tabpanel"][data-document-id="${id}"]`);
const input = (page, id) => panel(page, id).locator('.config-editor__input');
const status = (page) => page.locator('.demo__documents-status');
const button = (page, name) => page.locator(`.demo__${name}`);

/** The typing pause and then some: a callback that would fire late has fired. */
const settle = (page) => page.waitForTimeout(700);

async function expectHeard(page, languages, syncs) {
  await expect(status(page)).toHaveAttribute('data-languages', languages.join(','));
  await expect(status(page)).toHaveAttribute('data-syncs', syncs.join(','));
  await settle(page);
  await expect(status(page)).toHaveAttribute('data-languages', languages.join(','));
  await expect(status(page)).toHaveAttribute('data-syncs', syncs.join(','));
}

/** Types at the end of `id`'s text, the focus back in its editor where it was. */
async function typeAtEnd(page, id, text) {
  await input(page, id).focus();
  await page.keyboard.press('Control+End');
  await page.keyboard.type(text);
}

test.describe('documents tell the host what is on screen', () => {
  setupEditor(test, '?documents');

  test('a mounted editor tells its language and that it is in sync, once', async ({ page }) => {
    await expectHeard(page, ['main:YAML'], ['main:true']);
    await tab(page, 'limits').click();
    await expectHeard(page, ['main:YAML', 'limits:TOML'], ['main:true', 'limits:true']);
    // Shown again, nothing new.
    await tab(page, 'main').click();
    await tab(page, 'limits').click();
    await expectHeard(page, ['main:YAML', 'limits:TOML'], ['main:true', 'limits:true']);
  });

  test('a switch that waits is told when it is made; edits that cannot convert are out of sync', async ({ page }) => {
    // `extra:` is a null, which TOML cannot write: the switch waits.
    await typeAtEnd(page, 'main', 'extra: ');
    await button(page, 'edit-as-toml').click();
    await expect(panel(page, 'main').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'YAML editor');
    await expectHeard(page, ['main:YAML'], ['main:true']);
    await typeAtEnd(page, 'main', '1');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true']);
    await expect(input(page, 'main')).toHaveValue(/extra = 1/);

    // Invalid TOML: out of sync once, however many keys keep it invalid.
    await typeAtEnd(page, 'main', '\nbroken = ');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await page.keyboard.type('x');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await page.keyboard.press('Backspace');
    await page.keyboard.type('2');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false', 'main:true']);
    await expect(page.locator('.demo__documents-values')).toContainText('broken: 2');
  });

  test('undo and redo across a switch are told; the handle asks for the language again', async ({ page }) => {
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true']);
    await input(page, 'main').focus();
    await page.keyboard.press('Control+z');
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML'], ['main:true']);
    await page.keyboard.press('Control+Shift+z');
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML', 'main:TOML'], ['main:true']);
    await page.keyboard.press('Control+z');
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML', 'main:TOML', 'main:YAML'], ['main:true']);
    // The list still says TOML: listing it again changes nothing.
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML', 'main:TOML', 'main:YAML'], ['main:true']);
    await button(page, 'ask-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML', 'main:TOML', 'main:YAML', 'main:TOML'], ['main:true']);
    await expect(panel(page, 'main').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
  });

  test('an empty and a comment-only document are shown as TOML', async ({ page }) => {
    await input(page, 'main').focus();
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Delete');
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true']);
    await expect(input(page, 'main')).toHaveValue('');
    await button(page, 'edit-as-yaml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML'], ['main:true']);
    await input(page, 'main').focus();
    await page.keyboard.type('# only a comment');
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML', 'main:TOML'], ['main:true']);
    // The host's comments stay in its file (the Edit-as notice says so).
    await expect(panel(page, 'main').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
  });

  test("asking for the file's language back waits while the text cannot convert", async ({ page }) => {
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true']);
    await typeAtEnd(page, 'main', '\nbroken = ');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await button(page, 'edit-as-yaml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await expect(panel(page, 'main').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
    await typeAtEnd(page, 'main', '3');
    await expectHeard(page, ['main:YAML', 'main:TOML', 'main:YAML'], ['main:true', 'main:false', 'main:true']);
    await expect(input(page, 'main')).toHaveValue(/broken: 3/);
  });

  test('two documents at once, each its own', async ({ page }) => {
    await button(page, 'edit-as-toml').click();
    await typeAtEnd(page, 'main', '\nbroken = ');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await tab(page, 'limits').click();
    await typeAtEnd(page, 'limits', '\nworse = ');
    // limits is in its own language: always written back.
    await expectHeard(
      page,
      ['main:YAML', 'main:TOML', 'limits:TOML'],
      ['main:true', 'main:false', 'limits:true'],
    );
    await tab(page, 'main').click();
    await typeAtEnd(page, 'main', '4');
    await expectHeard(
      page,
      ['main:YAML', 'main:TOML', 'limits:TOML'],
      ['main:true', 'main:false', 'limits:true', 'main:true'],
    );
  });

  test('paste and undo change the in-sync flag too', async ({ page, context }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await button(page, 'edit-as-toml').click();
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true']);
    await page.evaluate(() => navigator.clipboard.writeText('\nbroken = '));
    await input(page, 'main').focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Control+v');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    await page.keyboard.press('Control+z');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false', 'main:true']);
    // Redone: out of sync again.
    await page.keyboard.press('Control+Shift+z');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false', 'main:true', 'main:false']);
  });

  test('a document opened from the text a host kept shows it, out of sync, and saves it once fixed', async ({ page }) => {
    await button(page, 'edit-as-toml').click();
    await typeAtEnd(page, 'main', '\nbroken = ');
    await expectHeard(page, ['main:YAML', 'main:TOML'], ['main:true', 'main:false']);
    const kept = await input(page, 'main').inputValue();
    await button(page, 'reopen').click();
    await tab(page, 'main-reopened').click();
    await expect(input(page, 'main-reopened')).toHaveValue(kept);
    await expect(panel(page, 'main-reopened').locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
    // main gone, limits took its place and mounted, then the reopened one.
    await expectHeard(
      page,
      ['main:YAML', 'main:TOML', 'limits:TOML', 'main-reopened:TOML'],
      ['main:true', 'main:false', 'limits:true', 'main-reopened:false'],
    );
    // The host's value is as it was until the text converts.
    await expect(page.locator('.demo__documents-values')).not.toContainText('broken');
    await typeAtEnd(page, 'main-reopened', '5');
    await expectHeard(
      page,
      ['main:YAML', 'main:TOML', 'limits:TOML', 'main-reopened:TOML'],
      ['main:true', 'main:false', 'limits:true', 'main-reopened:false', 'main-reopened:true'],
    );
    // In the host's language, its comment kept.
    await expect(page.locator('.demo__documents-values')).toContainText('# Tunnel server configuration');
    await expect(page.locator('.demo__documents-values')).toContainText('broken: 5');
    // Undo starts at the kept text.
    await page.keyboard.press('Control+z');
    await expect(input(page, 'main-reopened')).toHaveValue(kept);
    await page.keyboard.press('Control+z');
    await expect(input(page, 'main-reopened')).toHaveValue(kept);
  });

  test("a read-only document's reason is on its tab, in its notice and in its editor's description", async ({ page }) => {
    const reason = 'Managed by the cluster operator';
    await expect(tab(page, 'clients')).toHaveAttribute('title', `conf.d/clients.json\nRead-only: ${reason}`);
    await expect(tab(page, 'clients')).toHaveAttribute('aria-label', 'conf.d/clients.json, read-only');
    const described = await tab(page, 'clients').getAttribute('aria-describedby');
    await expect(page.locator(`[id="${described}"]`)).toHaveText(reason);
    // An editable tab has no description.
    await expect(tab(page, 'limits')).not.toHaveAttribute('aria-describedby', /./);
    await expect(tab(page, 'limits')).toHaveAttribute('title', 'conf.d/limits.toml');

    await tab(page, 'clients').click();
    await input(page, 'clients').click();
    await page.keyboard.type('x');
    const notice = panel(page, 'clients').locator('.config-editor__read-only-notice');
    await expect(notice).toHaveText(`Cannot edit in a read-only editor: ${reason}`);
    await expect(panel(page, 'clients').locator('.config-editor__announcement')).toHaveText(
      `Cannot edit in a read-only editor: ${reason}`,
    );
    const ids = (await input(page, 'clients').getAttribute('aria-describedby')).split(' ');
    const texts = await Promise.all(ids.map((id) => page.locator(`[id="${id}"]`).textContent()));
    expect(texts).toContain(reason);
  });
});

// The npm packages' example pages, built with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

/** The `language` and `sync` lines of the page's log, in order. */
const screenLog = async (page) =>
  (await page.locator('#log').textContent()).split('\n').filter((line) => /^(language|sync) /.test(line));

async function expectLog(page, lines) {
  await expect.poll(() => screenLog(page)).toEqual(lines);
  await page.waitForTimeout(700);
  expect(await screenLog(page)).toEqual(lines);
}

/** Calls the page's `editAs` (the vanilla view's, else the component's handle). */
const editAs = (page, id, language) =>
  page.evaluate(
    ([id, language]) => (window.documentsView ?? window.documentsHandle.current).editAs(id, language),
    [id, language],
  );

for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
  test.describe(`JS example ${name}: what is on screen`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test.beforeEach(async ({ page }) => {
      page.errors = [];
      page.on('pageerror', (error) => page.errors.push(String(error)));
      page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
      await page.goto(example(name));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.locator('.config-editor__input').waitFor();
    });

    test.afterEach(async ({ page }) => {
      expect(page.errors).toEqual([]);
    });

    test('mount, a switch that waits, undo, asking again and the in-sync flag', async ({ page }) => {
      await expectLog(page, ['language main yaml', 'sync main true']);
      // `extra:` is a null: the switch waits until it is a number.
      await typeAtEnd(page, 'main', 'extra: ');
      await page.evaluate(() => window.updateFile('main', { editAs: 'toml' }));
      await expectLog(page, ['language main yaml', 'sync main true']);
      await typeAtEnd(page, 'main', '1');
      await expectLog(page, ['language main yaml', 'sync main true', 'language main toml']);

      await typeAtEnd(page, 'main', '\nbroken = ');
      await expectLog(page, ['language main yaml', 'sync main true', 'language main toml', 'sync main false']);
      await page.keyboard.press('Backspace');
      await page.keyboard.press('Backspace');
      await page.keyboard.press('Backspace');
      await page.keyboard.type('= 2');
      await expectLog(page, [
        'language main yaml',
        'sync main true',
        'language main toml',
        'sync main false',
        'sync main true',
      ]);
      // Undo across the switch, then ask for TOML again: the prop is unchanged.
      const frame = panel(page, 'main').locator('.config-editor__frame');
      for (let step = 0; step < 10 && (await frame.getAttribute('aria-label')) !== 'YAML editor'; step += 1) {
        await page.keyboard.press('Control+z');
      }
      await expect(frame).toHaveAttribute('aria-label', 'YAML editor');
      await expect(input(page, 'main')).toHaveValue(/extra: 1/);
      const back = await screenLog(page);
      expect(back.at(-1)).toBe('language main yaml');
      expect(await editAs(page, 'main', 'toml')).toBe(true);
      await expectLog(page, [...back, 'language main toml']);
      expect(await editAs(page, 'missing', 'toml')).toBe(false);
    });

    test('a kept text opens out of sync and converts once fixed; a read-only reason shows', async ({ page }) => {
      await page.evaluate(() =>
        window.updateFile('limits', { editAs: 'yaml', editAsText: 'limits:\n  max_pending: [\n' }),
      );
      await tab(page, 'limits').click();
      await expect(input(page, 'limits')).toHaveValue('limits:\n  max_pending: [\n');
      await expectLog(page, ['language main yaml', 'sync main true', 'language limits yaml', 'sync limits false']);
      await input(page, 'limits').focus();
      await page.keyboard.press('Control+a');
      await page.keyboard.type('limits:\n  max_pending: 9');
      await expectLog(page, [
        'language main yaml',
        'sync main true',
        'language limits yaml',
        'sync limits false',
        'sync limits true',
      ]);
      await expect(page.locator('#log')).toContainText('change limits');

      await page.evaluate(() => window.updateFile('clients', { readOnlyReason: 'Managed by the cluster operator' }));
      await expect(tab(page, 'clients')).toHaveAttribute('title', 'conf.d/clients.json\nRead-only: Managed by the cluster operator');
      await tab(page, 'clients').click();
      await input(page, 'clients').click();
      await page.keyboard.type('x');
      await expect(panel(page, 'clients').locator('.config-editor__read-only-notice')).toHaveText(
        'Cannot edit in a read-only editor: Managed by the cluster operator',
      );
    });
  });
}
