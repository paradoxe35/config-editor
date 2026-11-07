import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// An editor destroyed while work it deferred still waits: the typing
// pause, the refusal tip's timer, the caret note, the hover card, input
// not applied yet, a documents view's next frame. That work runs after the
// editor's state is gone and must skip it (see
// crates/config-editor-leptos/src/reactive/later.rs), on the vanilla page
// and in each framework component. Build the packages first with
// `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

/** Longer than any timer the editor sets (the protect tip hides after 2.5 s). */
const TIMERS_MS = 3000;

const TEXT = '# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n';

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  if (name === 'vanilla.html') {
    // An editor of the test's own, with the same document and lock as the
    // framework pages.
    await browserPage.evaluate(async (value) => {
      window.editor?.destroy();
      window.editor?.free();
      const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
      const schema = await (await fetch('../../schema.json')).text();
      window.editor = new ConfigEditor(document.getElementById('editor'), {
        value,
        schema,
        readOnlyPaths: ['/public_address'],
      });
      window.unmountEditor = () => {
        window.editor.destroy();
        window.editor.free();
      };
    }, TEXT);
  } else {
    await browserPage.locator('#lock-address').check();
    await browserPage.evaluate(() => {
      window.unmountEditor = () => window.setShown(false);
    });
  }
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');

/** The middle of the first token on `line` reading `text`. */
async function tokenAt(browserPage, line, text) {
  const token = browserPage
    .locator(`.config-editor__code-line[data-line="${line}"] span`, { hasText: new RegExp(`^${text}$`) })
    .first();
  const box = await token.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** What starts each kind of deferred work, right before the unmount. */
const pending = {
  async typingPause(p) {
    await input(p).click();
    await p.keyboard.press('Control+End');
    await p.keyboard.type('a');
  },
  async protectTip(p) {
    const at = await tokenAt(p, 3, 'tunnel.example.com');
    await p.mouse.click(at.x, at.y);
    await p.keyboard.type('z');
    await expect(p.locator('.config-editor__protect-tip')).toBeVisible();
  },
  async caretNote(p) {
    await input(p).click();
    await p.keyboard.press('Control+Home');
    await p.keyboard.press('ArrowDown');
  },
  async hoverCard(p) {
    const at = await tokenAt(p, 2, 'listen');
    await p.mouse.move(at.x, at.y, { steps: 3 });
  },
};

const pages = ['vanilla.html', ...FRAMEWORK_PAGES];

for (const name of pages) {
  test.describe(`${name}: unmounted while work waits`, () => {
    for (const [work, start] of Object.entries(pending)) {
      test(`${work} pending at unmount`, async ({ page: browserPage }) => {
        const errors = await open(browserPage, name);
        await start(browserPage);
        await browserPage.evaluate(() => window.unmountEditor());
        await expect(input(browserPage)).toHaveCount(0);
        await browserPage.waitForTimeout(TIMERS_MS);
        expect(errors).toEqual([]);
      });
    }

    test('input not applied yet at unmount', async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      await input(browserPage).click();
      // The editor applies input at the end of the task: the unmount comes
      // first.
      await browserPage.evaluate(() => {
        const textarea = document.querySelector('.config-editor__input');
        textarea.value += 'x';
        textarea.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: 'x' }));
        window.unmountEditor();
      });
      await expect(input(browserPage)).toHaveCount(0);
      await browserPage.waitForTimeout(TIMERS_MS);
      expect(errors).toEqual([]);
    });
  });
}

test('documents view destroyed while its next frame waits', async ({ page: browserPage }) => {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page('documents.html'));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.evaluate((value) => {
    const view = window.documentsView;
    view.update({ documents: [
      { id: 'a', fileName: 'a.yaml', value },
      { id: 'b', fileName: 'b.yaml', value },
    ] });
  }, TEXT);
  await expect(browserPage.locator('.config-editor__input')).toHaveCount(1);
  // A switch (the tab scrolled into view and focused next frame) and a
  // close (its entry freed next frame), then the view goes in the same task.
  await browserPage.evaluate((value) => {
    const view = window.documentsView;
    view.select('b');
    view.update({ documents: [{ id: 'b', fileName: 'b.yaml', value }] });
    view.destroy();
  }, TEXT);
  await expect(browserPage.locator('.config-editor-documents__tab')).toHaveCount(0);
  await browserPage.waitForTimeout(500);
  expect(errors).toEqual([]);
});
