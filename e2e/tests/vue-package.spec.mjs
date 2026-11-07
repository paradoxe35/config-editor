import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { frameworkCases } from './framework-cases.mjs';

// `@pngwasi/config-editor-vue` through js/examples/vue.html (its app is a
// template: `v-model`, `@save`, `show-footer`): every case a framework
// component must pass (framework-cases.mjs), then what is Vue's own: props
// cast the Vue way, listeners as props, the exposed instance, unmounting.
// The settings, documents, memory and parity specs run on vue.html and
// documents-vue.html too. Build the packages first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

frameworkCases('vue.html');

async function open(browserPage) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => ['error', 'warning'].includes(message.type()) && errors.push(message.text()));
  await browserPage.goto(page('vue.html'));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

/**
 * Mounts a `ConfigEditor` of its own into a new element, with `props` and a
 * listener for each name in `events` that records its arguments in
 * `window.events`; `window.mounted` holds the app and the template ref.
 */
async function mountOwn(browserPage, props, events = []) {
  await browserPage.evaluate(
    ([props, events]) => {
      const { createApp, h, ref } = window.Vue;
      const { ConfigEditor } = window.packageApi;
      window.mounted?.app.unmount();
      document.getElementById('own')?.remove();
      const element = document.body.appendChild(document.createElement('div'));
      element.id = 'own';
      window.events = [];
      const editor = ref(null);
      const listeners = Object.fromEntries(
        events.map((name) => [name, (...args) => window.events.push([name, ...args])]),
      );
      const app = createApp({ render: () => h(ConfigEditor, { ref: editor, ...props, ...listeners }) });
      app.mount(element);
      window.mounted = { app, editor };
    },
    [props, events],
  );
  await browserPage.locator('#own .config-editor__input').waitFor();
}

test.describe('vue.html: Vue idioms', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('a boolean shorthand reads as true, an absent setting keeps its default', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    // `show-footer` with no value is `""`, cast to true; `folding` is absent
    // and stays on (Vue would cast an absent Boolean prop to false).
    await mountOwn(browserPage, { modelValue: 'web:\n  listen: 1\n', showFooter: '' });
    await expect(browserPage.locator('#own .config-editor__status')).toHaveCount(1);
    await expect(browserPage.locator('#own .config-editor__gutter-folding')).not.toHaveCount(0);
    expect(await browserPage.evaluate(() => window.mounted.editor.value.editor.settings.folding)).toBe(true);
    expect(errors).toEqual([]);
  });

  test('listeners are props: @save takes Ctrl+S, the others fire with their arguments', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { modelValue: 'listen: 1\n', fileName: 'own.yaml' }, [
      'onUpdate:modelValue', 'onSave', 'onDirtyChange', 'onPathChange', 'onChangeSummary', 'onEditAsChange',
    ]);
    const own = browserPage.locator('#own .config-editor__input');
    await own.click();
    await browserPage.keyboard.press('Control+End');
    await browserPage.keyboard.type('a: 2');
    await browserPage.keyboard.press('Control+s');
    await expect.poll(() => browserPage.evaluate(() => window.events.map(([name]) => name))).toEqual(
      expect.arrayContaining(['onUpdate:modelValue', 'onSave', 'onDirtyChange', 'onPathChange', 'onChangeSummary']),
    );
    const events = await browserPage.evaluate(() => window.events);
    expect(events.find(([name]) => name === 'onSave')).toEqual(['onSave', 'listen: 1\na: 2', 'yaml']);
    expect(events.filter(([name]) => name === 'onDirtyChange').map(([, dirty]) => dirty)).toEqual([false, true]);
    expect(events.findLast(([name]) => name === 'onUpdate:modelValue')[1]).toBe('listen: 1\na: 2');
    // Without a save listener the instance has nothing to save to.
    await mountOwn(browserPage, { modelValue: 'listen: 1\n' });
    expect(await browserPage.evaluate(() => window.mounted.editor.value.save())).toBe(false);
    expect(errors).toEqual([]);
  });

  test('@error takes the errors a bad prop raises, and nothing is logged', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { modelValue: 'listen: 1\n' }, ['onError']);
    await browserPage.evaluate(async () => {
      window.mounted.app.unmount();
      const { createApp, h, ref } = window.Vue;
      const { ConfigEditor } = window.packageApi;
      const theme = ref('nord');
      const app = createApp({
        render: () => h(ConfigEditor, { modelValue: 'listen: 1\n', theme: theme.value, onError: (error) => window.events.push(String(error)) }),
      });
      app.mount(document.getElementById('own'));
      window.theme = theme;
    });
    await browserPage.locator('#own .config-editor__input').waitFor();
    await browserPage.evaluate(() => (window.theme.value = 'not-a-theme'));
    await expect.poll(() => browserPage.evaluate(() => window.events.join('\n'))).toContain('not-a-theme');
    expect(errors).toEqual([]);
  });

  test('the template ref is the whole instance API', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    const members = await browserPage.evaluate(() => {
      const handle = window.editorHandle.current;
      return {
        value: handle.value,
        text: handle.text.length > 0,
        dirty: handle.isDirty,
        folded: handle.foldedLines,
        editor: handle.editor !== null,
        functions: ['setValue', 'unifiedDiff', 'rangeOf', 'pathAt', 'reveal', 'fold', 'unfold', 'foldAll', 'unfoldAll', 'focus', 'save']
          .filter((name) => typeof handle[name] !== 'function'),
      };
    });
    expect(members).toEqual({ value: expect.stringContaining('listen: 7835'), text: true, dirty: false, folded: [], editor: true, functions: [] });
    expect(errors).toEqual([]);
  });

  test('unmounting destroys the editor and empties its element', async ({ page: browserPage }) => {
    const errors = await open(browserPage);
    await mountOwn(browserPage, { modelValue: 'listen: 1\n' });
    await browserPage.evaluate(() => window.mounted.app.unmount());
    await expect(browserPage.locator('#own')).toBeEmpty();
    expect(errors).toEqual([]);
  });
});
