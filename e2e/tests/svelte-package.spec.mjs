import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { frameworkCases } from './framework-cases.mjs';

// `@pngwasi/config-editor-svelte` through js/examples/svelte.html (its app
// binds the document with `bind:value`, or `value` + `onChange` for the late
// and slow parents): every case a framework component must pass
// (framework-cases.mjs), then what is Svelte's own: `bind:value` both ways,
// callback props, the exported functions, unmounting. The settings,
// documents, memory and parity specs run on svelte.html and
// documents-svelte.html too. Build the packages first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

frameworkCases('svelte.html');

async function open(browserPage) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => ['error', 'warning'].includes(message.type()) && errors.push(message.text()));
  await browserPage.goto(page('svelte.html'));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

/**
 * Mounts a `ConfigEditor` of its own into a new element with `props` and a
 * callback for each name in `events` that records its arguments in
 * `window.events`; `window.mounted` holds the instance (its exports).
 */
async function mountOwn(browserPage, props, events = [], { mounts = true } = {}) {
  await browserPage.evaluate(
    ([props, events]) => {
      const { mount, unmount } = window.Svelte;
      const { ConfigEditor } = window.packageApi;
      if (window.mounted) unmount(window.mounted);
      document.getElementById('own')?.remove();
      const target = document.body.appendChild(document.createElement('div'));
      target.id = 'own';
      window.events = [];
      const callbacks = Object.fromEntries(events.map((name) => [name, (...args) => window.events.push([name, ...args])]));
      window.mounted = mount(ConfigEditor, { target, props: { ...props, ...callbacks } });
    },
    [props, events],
  );
  if (mounts) await browserPage.locator('#own .config-editor__input').waitFor();
}

const input = (browserPage) => browserPage.locator('.config-editor__input').first();
const saved = (browserPage) => browserPage.locator('#saved');

test.describe('svelte.html: Svelte idioms', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('bind:value carries typing to the parent and the parent\'s value to the editor', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('debug: true');
    await expect(saved(browserPage)).toContainText('debug: true');
    await browserPage.locator('#replace').click();
    await expect(input(browserPage)).toHaveValue('listen: 9000\n');
    await input(browserPage).click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('x: 1');
    await expect(saved(browserPage)).toHaveText('listen: 9000\nx: 1');
    // A reset to the value of before the edits still applies.
    await browserPage.locator('#reset').click();
    await expect(input(browserPage)).toHaveValue(/^# Burrow server configuration\n/);
    expect(errors).toEqual([]);
  });

  test('callback props fire with their arguments; without onSave there is nothing to save to', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { value: 'listen: 1\n', fileName: 'own.yaml' }, [
      'onChange', 'onSave', 'onDirtyChange', 'onPathChange', 'onChangeSummary',
    ]);
    await browserPage.locator('#own .config-editor__input').click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('a: 2');
    await browserPage.keyboard.press('Control+s');
    await expect.poll(() => browserPage.evaluate(() => window.events.map(([name]) => name))).toEqual(
      expect.arrayContaining(['onChange', 'onSave', 'onDirtyChange', 'onPathChange', 'onChangeSummary']),
    );
    const events = await browserPage.evaluate(() => window.events);
    expect(events.find(([name]) => name === 'onSave')).toEqual(['onSave', 'listen: 1\na: 2', 'yaml']);
    expect(events.filter(([name]) => name === 'onDirtyChange').map(([, dirty]) => dirty)).toEqual([false, true]);
    expect(events.findLast(([name]) => name === 'onChange')[1]).toBe('listen: 1\na: 2');
    await mountOwn(browserPage, { value: 'listen: 1\n' });
    expect(await browserPage.evaluate(() => window.mounted.save())).toBe(false);
    expect(errors).toEqual([]);
  });

  test('onError takes the errors a bad prop raises, and nothing is logged', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { value: 'listen: 1\n', theme: 'not-a-theme' }, ['onError'], { mounts: false });
    await expect.poll(() => browserPage.evaluate(() => window.events.map(([, error]) => String(error)).join('\n'))).toContain('not-a-theme');
    await expect(browserPage.locator('#own .config-editor__input')).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test('the exported functions are the whole instance API', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { value: 'web:\n  listen: 1\n' });
    const result = await browserPage.evaluate(() => {
      const instance = window.mounted;
      return {
        value: instance.getValue(),
        text: instance.getText(),
        dirty: instance.isDirty(),
        editor: instance.getEditor() !== null,
        path: instance.getCurrentPath()?.pointer ?? null,
        range: instance.rangeOf('/web/listen')?.startLine,
        fold: instance.fold('/web'),
        folded: instance.getFoldedLines(),
        diff: (instance.setValue('web: {}\n'), instance.unifiedDiff(0)),
      };
    });
    expect(result).toEqual({
      value: 'web:\n  listen: 1\n',
      text: 'web:\n  listen: 1\n',
      dirty: false,
      editor: true,
      path: expect.any(String),
      range: 2,
      fold: true,
      folded: [{ startLine: 1, endLine: 2 }],
      diff: expect.stringContaining('+web: {}'),
    });
    expect(errors).toEqual([]);
  });

  test('unmounting destroys the editor and empties its element', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { value: 'listen: 1\n' });
    await browserPage.evaluate(() => window.Svelte.unmount(window.mounted));
    await expect(browserPage.locator('#own')).toBeEmpty();
    expect(errors).toEqual([]);
  });
});
