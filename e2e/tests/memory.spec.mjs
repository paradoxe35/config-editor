import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Memory over many cycles of what hosts do all day: mount and destroy
// editors, scroll and fold long documents, open and close documents, and
// redraw underlines and suggestion lists. WebAssembly memory never shrinks,
// so each test measures its growth after a warm-up: what a cycle leaks shows
// up as steady growth, while memory a cycle frees is reused by the next.
// Through the npm package (build it first with `npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const KB = 1024;

/** A 1500-line YAML document, with a nested map every tenth line to fold. */
const longDocument = () =>
  Array.from({ length: 1500 }, (_, index) =>
    index % 10 === 0 ? `group${index}:\n  key${index}: ${index}` : `key${index}: ${index}`,
  ).join('\n');

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  // The page's own editor goes: each test mounts its own.
  await browserPage.evaluate(async (value) => {
    window.longDocument = value;
    window.editor?.destroy();
    window.editor?.free();
    window.editor = null;
  }, longDocument());
  return errors;
}

/** The WebAssembly memory's size, in bytes. */
async function wasmBytes(browserPage) {
  return browserPage.evaluate(async () => {
    if (window.reactApi) return (await window.reactApi.loadConfigEditor()).memory.buffer.byteLength;
    const module = await import('../config-editor/pkg/config_editor.js');
    return (await module.default()).memory.buffer.byteLength;
  });
}

/** Lets pending effects, timers and frames run. */
const settle = (browserPage) =>
  browserPage.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 50)))),
  );

/** Runs `cycle` `warmUp` times, then `cycles` more; returns the growth of
 * the WebAssembly memory over the later ones. */
async function growth(browserPage, { warmUp = 5, cycles, cycle }) {
  for (let round = 0; round < warmUp; round += 1) await cycle(round);
  await settle(browserPage);
  const before = await wasmBytes(browserPage);
  for (let round = warmUp; round < warmUp + cycles; round += 1) await cycle(round);
  await settle(browserPage);
  return (await wasmBytes(browserPage)) - before;
}

test.describe('memory', () => {
  test('editors destroyed and freed give their memory back', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const grown = await growth(browserPage, {
      cycles: 40,
      cycle: () =>
        browserPage.evaluate(async () => {
          const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
          const editor = new ConfigEditor(document.getElementById('editor'), { value: window.longDocument });
          await new Promise((resolve) => setTimeout(resolve, 20));
          editor.destroy();
          editor.free();
        }),
    });
    // Each mount keeps its state's handles (under 1 KB, see `EditorState`).
    expect(grown).toBeLessThanOrEqual(128 * KB);
    expect(errors).toEqual([]);
  });

  test('scrolling and folding a long document keeps memory flat', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
      window.editor = new ConfigEditor(document.getElementById('editor'), { value: window.longDocument });
    });
    await browserPage.locator('.config-editor__input').click();
    await browserPage.mouse.move(300, 300);
    const grown = await growth(browserPage, {
      warmUp: 3,
      cycles: 8,
      cycle: async () => {
        // Rows scroll in and out of view.
        for (const delta of [1500, 1500, 1500, 1500, -1500, -1500, -1500, -1500]) {
          await browserPage.mouse.wheel(0, delta);
          await browserPage.waitForTimeout(30);
        }
        // Fold all, unfold all.
        await browserPage.keyboard.press('Control+k');
        await browserPage.keyboard.press('Control+0');
        await expect(browserPage.locator('.config-editor__fold-chip').first()).toBeVisible();
        await browserPage.keyboard.press('Control+k');
        await browserPage.keyboard.press('Control+j');
        await expect(browserPage.locator('.config-editor__fold-chip')).toHaveCount(0);
      },
    });
    // Leaking, each cycle grew it by about 1.2 MB.
    expect(grown).toBeLessThanOrEqual(256 * KB);
    expect(errors).toEqual([]);
  });

  test('redrawn underlines and suggestion lists leave nothing behind', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
      const schema = await (await fetch('../../schema.json')).text();
      window.editor = new ConfigEditor(document.getElementById('editor'), {
        value: window.longDocument.split('\n').slice(0, 300).join('\n'),
        schema,
      });
      window.hostProblems = Array.from({ length: 300 }, (_, index) => ({
        range: { startLine: index + 1, startColumn: 1, endLine: index + 1, endColumn: 4 },
        message: `problem ${index}`,
      }));
    });
    await browserPage.locator('.config-editor__input').click();
    const grown = await growth(browserPage, {
      cycles: 20,
      cycle: async () => {
        // Every underline is drawn again, and again without the host's.
        await browserPage.evaluate(() => window.editor.setHostDiagnostics(window.hostProblems));
        await expect(browserPage.locator('.config-editor__diagnostic-marker-host').first()).toBeAttached();
        await browserPage.evaluate(() => window.editor.setHostDiagnostics([]));
        await expect(browserPage.locator('.config-editor__diagnostic-marker-host')).toHaveCount(0);
        // A suggestion list opens and closes (the root's keys, at the start).
        await browserPage.keyboard.press('Control+Home');
        await browserPage.keyboard.press('Control+Space');
        await expect(browserPage.locator('.config-editor__completion').first()).toBeVisible();
        await browserPage.keyboard.press('Escape');
        await expect(browserPage.locator('.config-editor__completion')).toHaveCount(0);
      },
    });
    expect(grown).toBeLessThanOrEqual(256 * KB);
    expect(errors).toEqual([]);
  });

  test('documents opened and closed give their memory back', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'documents.html');
    await browserPage.evaluate(() => {
      window.mainFile = { id: 'main', fileName: 'main.yaml', value: 'listen: 7835\n' };
      window.documentsView.setDocuments([window.mainFile]);
    });
    const grown = await growth(browserPage, {
      cycles: 400,
      cycle: async (round) => {
        const id = `file${round}`;
        await browserPage.evaluate(
          async ({ id }) => {
            const view = window.documentsView;
            const pause = () => new Promise((resolve) => setTimeout(resolve, 10));
            view.setDocuments([window.mainFile, { id, fileName: `${id}.yaml`, value: window.longDocument }]);
            await pause();
            view.select(id);
            await pause();
            view.setDocuments([window.mainFile]);
            await pause();
          },
          { id },
        );
      },
    });
    // Each editor keeps its state's handles (under 1 KB: 300 KB here);
    // leaking, each closed document kept about as much again.
    expect(grown).toBeLessThanOrEqual(480 * KB);
    expect(errors).toEqual([]);
  });

  test('the React component remounting gives the old editor back', async ({ page: browserPage }) => {
    await browserPage.goto(page('react.html'));
    await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    await browserPage.locator('.config-editor__input').waitFor();
    const grown = await growth(browserPage, {
      cycles: 40,
      cycle: async (round) => {
        // A new file name mounts a new editor.
        await browserPage.evaluate((name) => window.setExtraProps({ fileName: name }), `file${round}.yaml`);
        await expect(browserPage.locator('.config-editor__toolbar')).toContainText(`file${round}.yaml`);
      },
    });
    expect(grown).toBeLessThanOrEqual(128 * KB);
  });
});
