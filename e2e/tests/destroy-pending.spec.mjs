import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// `destroy()` (then `free()`, as the framework bindings unmount) during a
// typing pause: the edit still waiting to convert reaches `onChange` first,
// nothing traps, and nothing runs afterwards. For a `ConfigDocuments`
// document and a `ConfigEditor`, in a converted (Edit-as) view and the
// host's own language, called directly, from a callback, and from the
// binding's unmount (QA D8.memory-1: a documents view destroyed inside the
// pause disposed the documents' state before unmounting their editors, so
// the editor's last report read a disposed value and trapped, losing the
// edit and stranding the view).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const START = 'listen: 7835\nname: web\n';
const TYPED = 'listen: 78359\nname: web\n';

const CONTAINERS = ['documents', 'editor'];
const VIEWS = [
  { name: 'converted', editAs: 'json' },
  { name: 'native', editAs: null },
];
// How the instance goes: `destroy(); free()` from code, the same from the
// callback the keystroke fires (`onDirtyChange`), from `onSave` while the
// instance's own `save()`/`saveAll()` runs, and the binding's unmount.
const WAYS = ['direct', 'in callback', 'in save', 'binding'];

async function open(page) {
  page.errors = [];
  page.on('console', (message) => message.type() === 'error' && page.errors.push(message.text()));
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.module = await import('../config-editor/index.js');
    window.binding = await import('../config-editor/binding.js');
    window.editor?.destroy();
    document.body.replaceChildren();
  });
}

/** Mounts `container` with START shown as `editAs`, recording what reaches
 * the host; `way` decides how `window.qa.end()` destroys it. */
async function mount(page, container, editAs, way) {
  await page.evaluate(
    async ({ container, editAs, way, start }) => {
      const host = document.createElement('div');
      host.id = 'qa';
      host.style.cssText = 'position:fixed;inset:0;height:100vh;';
      document.body.append(host);
      const qa = { heard: [], thrown: [], ended: false, instance: null, unmount: null };
      window.qa = qa;
      const guard = (step) => {
        try {
          step();
        } catch (error) {
          qa.thrown.push(String(error));
        }
      };
      qa.end = () => {
        if (qa.ended) return;
        qa.ended = true;
        if (qa.unmount) {
          guard(qa.unmount);
          return;
        }
        guard(() => qa.instance.destroy());
        guard(() => qa.instance.free());
      };
      const onDirtyChange = (...args) => {
        const dirty = args[args.length - 1];
        if (dirty && way === 'in callback') qa.end();
      };
      const onSave = () => {
        if (way === 'in save') qa.end();
      };
      const documents = container === 'documents';
      const onChange = documents ? (_, text) => qa.heard.push(text) : (text) => qa.heard.push(text);
      const options = documents
        ? { documents: [{ id: 'main', fileName: 'main.yaml', value: start, editAs }], onChange, onDirtyChange, onSave }
        : { language: 'yaml', value: start, editAs, onChange, onDirtyChange, onSave };
      Object.assign(options, { showFooter: false, showToolbar: false });
      if (way === 'binding') {
        const Binding = documents ? window.binding.DocumentsBinding : window.binding.EditorBinding;
        if (!documents) {
          options.defaultValue = options.value;
          delete options.value;
        }
        const binding = new Binding('qa', options);
        binding.attach(host);
        while (!binding.instance) await new Promise((resolve) => setTimeout(resolve, 10));
        qa.instance = binding.instance;
        qa.unmount = () => binding.detach();
      } else {
        qa.instance = documents
          ? new window.module.ConfigDocuments(host, options)
          : new window.module.ConfigEditor(host, options);
      }
      qa.go = () => {
        if (way !== 'in save') return qa.end();
        if (documents) qa.instance.saveAll();
        else qa.instance.save();
      };
    },
    { container, editAs, way, start: START },
  );
}

for (const container of CONTAINERS) {
  for (const view of VIEWS) {
    for (const way of WAYS) {
      test(`${container}, ${view.name} view, destroyed ${way} in the typing pause: the edit reaches onChange`, async ({
        page,
      }) => {
        test.skip(!built, 'run `npm run build` in js/ first');
        await open(page);
        await mount(page, container, view.editAs, way);
        const input = page.locator('#qa .config-editor__input');
        await expect(input).toBeVisible();
        if (view.editAs) await expect.poll(() => input.inputValue()).toMatch(/^\{/);
        await input.evaluate((element) => {
          const at = element.value.indexOf('7835') + 4;
          element.focus();
          element.setSelectionRange(at, at);
        });
        // Typed, then gone within the pause (250 ms) before its report.
        await page.keyboard.type('9');
        await page.evaluate(() => window.qa.go());
        await page.waitForTimeout(500);
        const after = await page.evaluate(() => ({
          heard: window.qa.heard,
          thrown: window.qa.thrown,
          inputs: document.querySelectorAll('#qa .config-editor__input').length,
        }));
        expect(after.thrown).toEqual([]);
        expect(page.errors).toEqual([]);
        expect(after.heard.at(-1)).toBe(TYPED);
        expect(after.inputs).toBe(0);
      });
    }
  }
}
