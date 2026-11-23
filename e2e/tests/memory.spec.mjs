import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

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
    if (window.coreApi) return (await window.coreApi.loadConfigEditor()).memory.buffer.byteLength;
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
async function growth(browserPage, options) {
  return (await growths(browserPage, options)).wasm;
}

/** `growth`, with the live heap bytes the later cycles left behind too
 * (`heap`: null unless the package is built with `alloc-stats`). */
async function growths(browserPage, { warmUp = 5, cycles, cycle }) {
  for (let round = 0; round < warmUp; round += 1) await cycle(round);
  await settle(browserPage);
  const before = { wasm: await wasmBytes(browserPage), heap: await heap(browserPage) };
  // A timer still pending holds its closure (the typing pause's holds the
  // text): the heap is read once they have all run.
  if (before.heap) {
    await outlastTimers(browserPage);
    before.heap = await heap(browserPage);
  }
  for (let round = warmUp; round < warmUp + cycles; round += 1) await cycle(round);
  await settle(browserPage);
  const wasm = await wasmBytes(browserPage);
  if (before.heap) await outlastTimers(browserPage);
  const after = { wasm, heap: await heap(browserPage) };
  if (!before.heap) return { wasm: after.wasm - before.wasm, heap: null };
  // The sizes whose live allocations changed: [size, change].
  const sizes = after.heap.sizes
    .map((count, size) => [size, count - before.heap.sizes[size]])
    .filter(([, change]) => change !== 0);
  return { wasm: after.wasm - before.wasm, heap: { bytes: after.heap.bytes - before.heap.bytes, sizes } };
}

/** The heap kept nothing over the cycles `growths` measured, in a package
 * built with `alloc-stats` (`npm run build:alloc-stats` in
 * js/config-editor). With `MEMORY_HEAP=1` (CI's heap job) such a package is
 * required: a leak too small to grow the WebAssembly memory still fails. */
function expectNothingKept(heapGrowth) {
  if (process.env.MEMORY_HEAP) {
    expect(heapGrowth, 'MEMORY_HEAP=1 needs a package built with alloc-stats').not.toBeNull();
  }
  if (heapGrowth === null) return;
  const kept = `live allocations kept, [size, count]: ${JSON.stringify(heapGrowth.sizes)}`;
  expect(heapGrowth.bytes, kept).toBeLessThanOrEqual(0);
}

/** Live heap bytes and live allocations per size, in a package built
 * with `alloc-stats` (else null). */
const heap = (browserPage) =>
  browserPage.evaluate(async () => {
    // A framework page's bundle has its own copy of the module, whose
    // statistics it does not export.
    if (window.coreApi) return null;
    const module = await import('../config-editor/pkg/config_editor.js');
    return module.__allocStats ? { bytes: module.__allocStats()[0], sizes: Array.from(module.__allocSizes()) } : null;
  });

test.describe('memory', () => {
  test('editors destroyed and freed give their memory back', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const grown = await growths(browserPage, {
      // The allocator settles into its pages over the first few dozen mounts.
      warmUp: 40,
      cycles: 200,
      cycle: () =>
        browserPage.evaluate(async () => {
          const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
          const editor = new ConfigEditor(document.getElementById('editor'), { value: window.longDocument });
          await new Promise((resolve) => setTimeout(resolve, 20));
          editor.destroy();
          editor.free();
        }),
    });
    // Nothing stays per mount: an editor's state lives in its owner's
    // arena (a leaked state, 960 B a mount, grew it by 192 KB here).
    expect(grown.wasm).toBeLessThanOrEqual(64 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });

  test('a long document typed in, then destroyed, is kept once, and freed, not at all', async ({
    page: browserPage,
  }) => {
    const errors = await open(browserPage, 'vanilla.html');
    // 25,000 lines (about 400 KB): a copy kept is plain in the heap.
    const value = Array.from({ length: 25000 }, (_, index) => `key${index}: value${index}`).join('\n');
    const textBytes = value.length + 3;
    const cycle = async () => {
      await browserPage.evaluate(async (value) => {
        const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
        window.editor = new ConfigEditor(document.getElementById('editor'), { value });
      }, value);
      await browserPage.locator('.config-editor__input').click();
      await browserPage.keyboard.press('Control+End');
      for (const key of 'abc') await browserPage.keyboard.press(key);
      await outlastTimers(browserPage);
      await browserPage.evaluate(() => window.editor.destroy());
      await outlastTimers(browserPage);
    };
    const free = () =>
      browserPage.evaluate(() => {
        window.editor.free();
        window.editor = null;
      });
    // The first mount fills what stays for the page (the font's widths).
    await cycle();
    await free();
    const before = await heap(browserPage);
    test.skip(!before && !process.env.MEMORY_HEAP, 'needs a package built with alloc-stats');
    expect(before, 'MEMORY_HEAP=1 needs a package built with alloc-stats').not.toBeNull();
    for (let round = 0; round < 3; round += 1) {
      await cycle();
      // Destroyed, the instance still answers `value`: one copy of the
      // text, not the two (the view's last text and the answer) it kept.
      const kept = (await heap(browserPage)).bytes - before.bytes;
      expect(kept, `round ${round}`).toBeGreaterThanOrEqual(textBytes);
      expect(kept, `round ${round}`).toBeLessThan(textBytes + 4 * KB);
      expect(await browserPage.evaluate(() => window.editor.value.endsWith('abc'))).toBe(true);
      await free();
      expect((await heap(browserPage)).bytes - before.bytes, `round ${round}, freed`).toBeLessThanOrEqual(0);
    }
    expect(errors).toEqual([]);
  });

  test('an open editor keeps one copy of its text, and typed in, one of its baseline', async ({
    page: browserPage,
  }) => {
    const errors = await open(browserPage, 'vanilla.html');
    // About 400 KB: each copy is one allocation of at least its size.
    const head = 'head of the copied text';
    const value = `${head}: 0\n${Array.from({ length: 25000 }, (_, index) => `key${index}: value${index}`).join('\n')}`;
    const watching = await browserPage.evaluate(async (size) => {
      const module = await import('../config-editor/pkg/config_editor.js');
      if (!module.__allocCopies) return false;
      module.__allocWatch(size);
      return true;
    }, value.length);
    test.skip(!watching && !process.env.MEMORY_HEAP, 'needs a package built with alloc-stats');
    expect(watching, 'MEMORY_HEAP=1 needs a package built with alloc-stats').toBe(true);
    const copies = () =>
      browserPage.evaluate(async (head) => {
        const module = await import('../config-editor/pkg/config_editor.js');
        return module.__allocCopies(head);
      }, head);
    const counts = {};
    // A host that hears nothing, one that hears each text, and one that
    // hands each text back.
    for (const host of ['silent', 'onChange', 'echoes']) {
      await browserPage.evaluate(
        async ({ value, host }) => {
          const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
          const options = { value };
          if (host === 'onChange') options.onChange = (text) => (window.heard = text);
          if (host === 'echoes') {
            options.echoes = true;
            options.onChange = (text, revision) =>
              setTimeout(() => window.editor.update({ value: text, valueRevision: revision }), 0);
          }
          window.editor = new ConfigEditor(document.getElementById('editor'), options);
        },
        { value, host },
      );
      await outlastTimers(browserPage);
      const mounted = await copies();
      await browserPage.locator('.config-editor__input').click();
      await browserPage.keyboard.press('Control+End');
      for (const key of 'abc') await browserPage.keyboard.press(key);
      await outlastTimers(browserPage);
      counts[host] = { mounted, typed: await copies() };
      await browserPage.evaluate(() => {
        window.editor.destroy();
        window.editor.free();
        window.editor = null;
      });
      await outlastTimers(browserPage);
    }
    // Mounted: the document, which the host's text, the host log, the
    // value the binding answers with, the textarea's value, the baseline,
    // the locks and the parses share. Typed in: that text again, and the
    // baseline, the text mounted with (the diff compares with it). 0.1
    // kept 16 copies mounted and 19 typed in (about 6.4 and 7.6 MB here).
    const once = { mounted: 1, typed: 2 };
    expect(counts).toEqual({ silent: once, onChange: once, echoes: once });
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
    const grown = await growths(browserPage, {
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
    expect(grown.wasm).toBeLessThanOrEqual(256 * KB);
    expectNothingKept(grown.heap);
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
    const grown = await growths(browserPage, {
      cycles: 20,
      cycle: async () => {
        // Every underline is drawn again, and again without the host's.
        await browserPage.evaluate(() => window.editor.update({ hostDiagnostics: window.hostProblems }));
        await expect(browserPage.locator('.config-editor__diagnostic-marker-host').first()).toBeAttached();
        await browserPage.evaluate(() => window.editor.update({ hostDiagnostics: [] }));
        await expect(browserPage.locator('.config-editor__diagnostic-marker-host')).toHaveCount(0);
        // A suggestion list opens and closes (the root's keys, at the start).
        await browserPage.keyboard.press('Control+Home');
        await browserPage.keyboard.press('Control+Space');
        await expect(browserPage.locator('.config-editor__completion').first()).toBeVisible();
        await browserPage.keyboard.press('Escape');
        await expect(browserPage.locator('.config-editor__completion')).toHaveCount(0);
      },
    });
    expect(grown.wasm).toBeLessThanOrEqual(256 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });

  test('settings switched off and on leave nothing behind', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    await browserPage.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
      const schema = await (await fetch('../../schema.json')).text();
      window.editor = new ConfigEditor(document.getElementById('editor'), {
        value: window.longDocument.split('\n').slice(0, 300).join('\n'),
        schema,
        settings: { showToolbar: true, showFooter: true },
      });
    });
    const parts = ['showToolbar', 'showFooter', 'showDiagnostics', 'showDiff', 'search', 'goToLine', 'folding', 'highlighting'];
    const grown = await growths(browserPage, {
      cycles: 150,
      cycle: async () => {
        for (const on of [false, true]) {
          await browserPage.evaluate(
            ({ parts, on }) =>
              window.editor.update({
                ...Object.fromEntries(parts.map((part) => [part, on])),
              }),
            { parts, on },
          );
          await expect(browserPage.locator('.config-editor__toolbar')).toHaveCount(on ? 1 : 0);
        }
      },
    });
    // Rebuilt at each switch, the parts grew it by about 3.5 KB a cycle.
    expect(grown.wasm).toBeLessThanOrEqual(192 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });

  test('documents opened and closed give their memory back', async ({ page: browserPage }) => {
    // 400 cycles: a long test, not a speed claim; a slower machine needs the room.
    test.slow();
    const errors = await open(browserPage, 'documents.html');
    await browserPage.evaluate(() => {
      window.mainFile = { id: 'main', fileName: 'main.yaml', value: 'listen: 7835\n' };
      window.documentsView.update({ documents: [window.mainFile] });
    });
    const grown = await growths(browserPage, {
      cycles: 400,
      cycle: async (round) => {
        const id = `file${round}`;
        await browserPage.evaluate(
          async ({ id }) => {
            const view = window.documentsView;
            const pause = () => new Promise((resolve) => setTimeout(resolve, 10));
            view.update({ documents: [window.mainFile, { id, fileName: `${id}.yaml`, value: window.longDocument }] });
            await pause();
            view.select(id);
            await pause();
            view.update({ documents: [window.mainFile] });
            await pause();
          },
          { id },
        );
      },
    });
    // Nothing stays per closed document (a leaked editor state, 960 B a
    // document, grew it by 375 KB here); the allocator's spread over 400
    // long documents reaches two pages at times. The heap (with
    // `alloc-stats`) keeps exactly nothing.
    expect(grown.wasm).toBeLessThanOrEqual(128 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });

  test('registering a language again and again takes no more memory', async ({ page: browserPage }) => {
    const errors = await open(browserPage, 'vanilla.html');
    const grown = await growths(browserPage, {
      warmUp: 2,
      cycles: 5,
      cycle: () =>
        browserPage.evaluate(async () => {
          const { registerLanguage } = await import('../config-editor/pkg/config_editor.js');
          // A new definition each time, as a component registering its
          // language on every mount makes it: fresh functions, the same
          // label and syntax.
          for (let index = 0; index < 1000; index += 1) {
            registerLanguage({
              label: 'Repeated',
              extensions: ['repeated'],
              parse: (text) => Object.fromEntries(text.split('\n').filter(Boolean).map((line) => line.split(' = '))),
              serialize: (value) => Object.entries(value).map(([key, item]) => `${key} = ${item}\n`).join(''),
              comments: { lineComment: '#', blockComment: ['/*', '*/'] },
              brackets: [['(', ')']],
              separators: ['='],
              writtenSeparator: ' = ',
              listBrackets: ['(', ')'],
              itemSeparator: ';',
              itemMarker: '*',
              literalQuotes: ["'"],
            });
          }
        }),
    });
    // Each registration leaked its language and texts, about 400 B (5000
    // grew memory by 2 MB).
    expect(grown.wasm).toBeLessThanOrEqual(64 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });

  for (const name of FRAMEWORK_PAGES) {
    test(`the ${name} component remounting gives the old editor back`, async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      const grown = await growth(browserPage, {
        cycles: 40,
        cycle: async (round) => {
          // A new file name mounts a new editor.
          await browserPage.evaluate((name) => window.setExtraProps({ fileName: name }), `file${round}.yaml`);
          await expect(browserPage.locator('.config-editor__toolbar')).toContainText(`file${round}.yaml`);
        },
      });
      expect(grown).toBeLessThanOrEqual(128 * KB);
      expect(errors).toEqual([]);
    });

    test(`the ${name} component unmounted and mounted again gives its editor back`, async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      const grown = await growth(browserPage, {
        cycles: 40,
        cycle: async () => {
          await browserPage.evaluate(() => window.setShown(false));
          await expect(browserPage.locator('.config-editor__input')).toHaveCount(0);
          await browserPage.evaluate(() => window.setShown(true));
          await expect(browserPage.locator('.config-editor__input')).toHaveCount(1);
        },
      });
      expect(grown).toBeLessThanOrEqual(128 * KB);
      expect(errors).toEqual([]);
    });
  }
});

// --- Soak: every interactive surface, many times over. -------------------
//
// A class guard: a view that comes and goes and subscribes to state that
// stays leaks a little each time it is shown (see
// crates/config-editor-leptos/src/reactive). Each surface below is cycled
// after a warm-up, and memory must stay flat. With `MEMORY_SOAK_REPORT=1`
// (or a list of surfaces) and a package built with the `alloc-stats`
// feature, each surface is also measured on its own (live heap bytes per
// cycle, and the allocation sizes that grew) and logged.

const SOAK_HEAD = [
  '# soak',
  'listen: 7835',
  'public_address: tunnel.example.com',
  'insecure_plaintext: false',
  'clients:',
  '  acme:',
  '    secret: «secret 3f2a»',
  'default_client: acme',
];
const soakDocument = () =>
  [
    ...SOAK_HEAD,
    ...Array.from({ length: 200 }, (_, index) =>
      index % 10 === 0 ? `group${index}:\n  key${index}: ${index}` : `key${index}: ${index}`,
    ),
  ].join('\n') + '\n';

/** The middle of the first highlighted token on `line` reading `text`. */
async function tokenAt(browserPage, line, text) {
  const token = browserPage
    .locator(`.config-editor__code-line[data-line="${line}"] span`, { hasText: new RegExp(`^${text}$`) })
    .first();
  const box = await token.boundingBox();
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

async function hoverAt(browserPage, line, text) {
  const at = await tokenAt(browserPage, line, text);
  await browserPage.mouse.move(at.x, at.y, { steps: 3 });
}

/** Moves the caret to the start of `line` with go to line. */
async function goToLine(browserPage, line) {
  await browserPage.keyboard.press('Control+g');
  await browserPage.keyboard.type(String(line));
  await browserPage.keyboard.press('Enter');
}

const input = (browserPage) => browserPage.locator('.config-editor__input');

/** Puts the caret `into` characters into the first `text` of the editor. */
const caretIn = (browserPage, text, into = 1) =>
  input(browserPage).evaluate(
    (element, [text, into]) => {
      const at = element.value.indexOf(text) + into;
      element.setSelectionRange(at, at);
    },
    [text, into],
  );

/** The caret's line (one-based) and the selected text. */
const caret = (browserPage) =>
  input(browserPage).evaluate((element) => ({
    line: element.value.slice(0, element.selectionStart).split('\n').length,
    selected: element.value.slice(element.selectionStart, element.selectionEnd),
  }));

/** Undoes the `steps` a surface made: the document is as it started, and
 * one more undo changes nothing (the history is as it was too: steps left
 * behind would pile up over the cycles, and a later step can bring the
 * text back before them, as a line moved down and up again does). */
async function undoBack(browserPage, steps) {
  await input(browserPage).focus();
  for (let undo = 0; undo < steps; undo += 1) await browserPage.keyboard.press('Control+z');
  const started = () => browserPage.evaluate(() => window.editor.value === window.soakDocument);
  await expect.poll(started).toBe(true);
  await browserPage.keyboard.press('Control+z');
  await settle(browserPage);
  expect(await started()).toBe(true);
}
/** The pointer onto empty text area (right of the short first line). */
async function away(browserPage) {
  const box = await input(browserPage).boundingBox();
  await browserPage.mouse.move(box.x + box.width - 30, box.y + 12, { steps: 3 });
}

/** Each surface: one cycle shows it and puts it away, leaving the document
 * and the undo history as they were. */
const surfaces = {
  async hover(p) {
    await goToLine(p, 1);
    await hoverAt(p, 2, 'listen');
    await expect(p.locator('.config-editor__hover')).toBeVisible();
    await hoverAt(p, 4, 'insecure_plaintext');
    await expect(p.locator('.config-editor__hover')).toBeVisible();
    await away(p);
    await expect(p.locator('.config-editor__hover')).toHaveCount(0);
  },
  async find(p) {
    await p.keyboard.press('Control+f');
    await p.keyboard.type('key1');
    await expect(p.locator('.config-editor__search-match').first()).toBeAttached();
    await p.keyboard.press('Enter');
    await p.keyboard.press('Escape');
    await expect(p.locator('.config-editor__find')).toBeHidden();
    await goToLine(p, 1);
  },
  async goToLine(p) {
    await goToLine(p, 120);
    await goToLine(p, 1);
  },
  async folding(p) {
    await p.keyboard.press('Control+k');
    await p.keyboard.press('Control+0');
    await expect(p.locator('.config-editor__fold-chip').first()).toBeVisible();
    await p.locator('.config-editor__fold-chip').first().click();
    await p.keyboard.press('Control+k');
    await p.keyboard.press('Control+j');
    await expect(p.locator('.config-editor__fold-chip')).toHaveCount(0);
    await p.locator('.config-editor__gutter').hover();
    await p.locator('.config-editor__fold-toggle').first().click();
    await expect(p.locator('.config-editor__fold-chip')).toHaveCount(1);
    await p.locator('.config-editor__fold-toggle').first().click();
    await expect(p.locator('.config-editor__fold-chip')).toHaveCount(0);
    await input(p).focus();
  },
  async completions(p) {
    // On the empty last line: a root key, accepted, then its value list.
    await p.keyboard.press('Control+End');
    await p.keyboard.press('Control+Space');
    await expect(p.locator('.config-editor__completion').first()).toBeVisible();
    await p.keyboard.press('ArrowDown');
    await p.keyboard.press('Enter');
    await p.keyboard.press('Escape');
    await expect(p.locator('.config-editor__completion')).toHaveCount(0);
    for (let undo = 0; undo < 5; undo += 1) {
      if (await p.evaluate(() => window.editor.value === window.soakDocument)) break;
      await p.keyboard.press('Control+z');
    }
    expect(await p.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
    await goToLine(p, 1);
  },
  async problems(p) {
    await p.keyboard.press('F8');
    await expect(p.locator('.config-editor__announcement')).toContainText(' of ');
    await p.keyboard.press('F8');
    await p.keyboard.press('Escape');
    await goToLine(p, 1);
  },
  async protectTip(p) {
    await goToLine(p, 1);
    // public_address is read-only: the edit is refused with a tip.
    const at = await tokenAt(p, 3, 'tunnel.example.com');
    await p.mouse.click(at.x, at.y);
    await p.keyboard.type('z');
    await expect(p.locator('.config-editor__protect-tip')).toBeVisible();
    await goToLine(p, 1);
    await away(p);
    await expect(p.locator('.config-editor__protect-tip')).toHaveCount(0, { timeout: 5000 });
  },
  async secretHint(p) {
    await goToLine(p, 1);
    const chip = await p.locator('.config-editor__code-line .config-editor__secret').first().boundingBox();
    await p.mouse.move(chip.x + chip.width / 2, chip.y + chip.height / 2, { steps: 3 });
    await expect(p.locator('.config-editor__protect-tip')).toBeVisible();
    await away(p);
    await expect(p.locator('.config-editor__protect-tip')).toHaveCount(0, { timeout: 5000 });
  },
  async baselineNotice(p) {
    await goToLine(p, 2);
    await p.keyboard.press('End');
    await p.keyboard.type('9');
    await p.evaluate(() => window.editor.update({ baseline: window.soakDocument.replace('7835', '7836') }));
    await expect(p.locator('.config-editor__notice')).toBeVisible();
    await p.locator('.config-editor__notice-keep').click();
    await expect(p.locator('.config-editor__notice')).toBeHidden();
    await input(p).focus();
    await p.keyboard.press('Control+z');
    await p.evaluate(() => window.editor.update({ baseline: window.soakDocument }));
    await expect.poll(() => p.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
  },
  async settings(p) {
    for (const on of [false, true]) {
      await p.evaluate(
        (on) =>
          window.editor.update({
            ...Object.fromEntries(
              Object.entries(window.editor.settings)
                .filter(([, value]) => typeof value === 'boolean')
                .map(([key]) => [key, on]),
            ),
          }),
        on,
      );
      await expect(p.locator('.config-editor__toolbar')).toHaveCount(on ? 1 : 0);
    }
    await input(p).focus();
  },
  async tabFocus(p) {
    await p.keyboard.press('Control+m');
    await expect(p.locator('.config-editor__tab-focus-status')).toHaveCount(1);
    await p.keyboard.press('Control+m');
    await expect(p.locator('.config-editor__tab-focus-status')).toHaveCount(0);
  },
  async hostDiagnostics(p) {
    await p.evaluate(() => window.editor.update({ hostDiagnostics: window.hostProblems }));
    await expect(p.locator('.config-editor__diagnostic-marker-host').first()).toBeAttached();
    await p.evaluate(() => window.editor.update({ hostDiagnostics: [] }));
    await expect(p.locator('.config-editor__diagnostic-marker-host')).toHaveCount(0);
  },
  async keyNames(p) {
    // F12 on a use goes to the client's key, selected.
    await caretIn(p, 'default_client: acme', 18);
    await p.keyboard.press('F12');
    await expect.poll(() => caret(p)).toEqual({ line: 6, selected: 'acme' });
    // Shift+F12 on the key lists its use; the keyboard moves and goes.
    await p.keyboard.press('Shift+F12');
    await expect(p.locator('.config-editor__uses [role="option"]')).toHaveCount(1);
    await p.keyboard.press('ArrowDown');
    await p.keyboard.press('Enter');
    await expect(p.locator('.config-editor__uses')).toHaveCount(0);
    await expect.poll(() => caret(p)).toEqual({ line: 8, selected: 'acme' });
    await p.keyboard.press('Shift+F12');
    await expect(p.locator('.config-editor__uses [role="option"]')).toHaveCount(1);
    await p.keyboard.press('Escape');
    await expect(p.locator('.config-editor__uses')).toHaveCount(0);
    // The hover card's links: Uses, then Definition.
    const actions = p.locator('.config-editor__hover-action');
    await hoverAt(p, 8, 'acme');
    await expect(actions).toHaveText(['Definition', 'Uses']);
    await actions.nth(1).click();
    await expect(p.locator('.config-editor__uses [role="option"]')).toHaveCount(1);
    await p.keyboard.press('Escape');
    await away(p);
    await expect(p.locator('.config-editor__hover')).toHaveCount(0);
    await hoverAt(p, 8, 'acme');
    await expect(actions).toHaveText(['Definition', 'Uses']);
    await actions.nth(0).click();
    await expect.poll(() => caret(p)).toEqual({ line: 6, selected: 'acme' });
    await away(p);
    await expect(p.locator('.config-editor__hover')).toHaveCount(0);
    await input(p).focus();
    await goToLine(p, 1);
  },
  async backForward(p) {
    await goToLine(p, 1);
    await goToLine(p, 120);
    await p.keyboard.press('Alt+ArrowLeft');
    await expect.poll(async () => (await caret(p)).line).toBe(1);
    await p.keyboard.press('Alt+ArrowRight');
    await expect.poll(async () => (await caret(p)).line).toBe(120);
    expect(await p.evaluate(() => window.editor.goBack())).toBe(true);
    await expect.poll(async () => (await caret(p)).line).toBe(1);
    expect(await p.evaluate(() => window.editor.goForward())).toBe(true);
    await expect.poll(async () => (await caret(p)).line).toBe(120);
    await goToLine(p, 1);
  },
  async caretNotices(p) {
    const notice = p.locator('.config-editor__caret-notice');
    await caretIn(p, 'listen', 2);
    await p.keyboard.press('F12');
    await expect(notice).toHaveText("No definition for 'listen'");
    await p.keyboard.press('Shift+F12');
    await expect(notice).toHaveText("No uses of 'listen'");
    // Ctrl+Space where nothing fits.
    await p.keyboard.press('Control+End');
    await p.keyboard.type('bindzzq');
    await p.keyboard.press('Escape');
    await p.keyboard.press('Control+Space');
    await expect(notice).toHaveText('No suggestions');
    await expect(p.locator('.config-editor__completion')).toHaveCount(0);
    await undoBack(p, 1);
    await goToLine(p, 1);
  },
  async lockTip(p) {
    // The pointer resting on a locked value shows why it is locked.
    await goToLine(p, 1);
    await hoverAt(p, 3, 'tunnel.example.com');
    await expect(p.locator('.config-editor__protect-tip')).toBeVisible();
    await away(p);
    await expect(p.locator('.config-editor__protect-tip')).toHaveCount(0, { timeout: 5000 });
  },
  async editAs(p) {
    await p.evaluate(() => window.editor.update({ editAs: 'json' }));
    await expect.poll(() => p.evaluate(() => window.editor.text.startsWith('{'))).toBe(true);
    await input(p).focus();
    await p.keyboard.press('Control+s');
    await expect.poll(() => p.evaluate(() => window.saved.length)).toBeGreaterThan(0);
    await p.evaluate(() => window.editor.update({ editAs: 'yaml' }));
    await expect.poll(() => p.evaluate(() => window.editor.text === window.soakDocument)).toBe(true);
    // A switch is an undo step: both are undone, leaving the history as it
    // was (what is undone goes with the next edit).
    await input(p).focus();
    await p.keyboard.press('Control+z');
    await expect.poll(() => p.evaluate(() => window.editor.text.startsWith('{'))).toBe(true);
    await p.keyboard.press('Control+z');
    await expect.poll(() => p.evaluate(() => window.editor.text === window.soakDocument)).toBe(true);
    await p.evaluate(() => (window.saved.length = 0));
    await goToLine(p, 1);
  },
  async hostChanges(p) {
    const swap = (from, to) =>
      p.evaluate(
        ([from, to]) => {
          const at = window.editor.text.indexOf(from);
          const revision = window.editor.applyChanges([{ from: at, to: at + from.length, insert: to }]);
          return revision ?? null;
        },
        [from, to],
      );
    // A plain value, a locked one and a secret placeholder, changed and back.
    for (const [from, to] of [
      ['7835', '8080'],
      ['tunnel.example.com', 'edge.example.com'],
      ['«secret 3f2a»', '«secret 4b1c»'],
    ]) {
      expect(await swap(from, to)).toEqual(expect.any(Number));
      expect(await swap(to, from)).toEqual(expect.any(Number));
    }
    // A new whole value over them, and the old one back.
    await p.evaluate(() =>
      window.editor.update({
        value: window.soakDocument.replace('tunnel.example.com', 'edge.example.com').replace('3f2a', '4b1c'),
      }),
    );
    await expect.poll(() => p.evaluate(() => window.editor.text.includes('edge.example.com'))).toBe(true);
    await p.evaluate(() => window.editor.update({ value: window.soakDocument }));
    await expect.poll(() => p.evaluate(() => window.editor.text === window.soakDocument)).toBe(true);
    // While the save is held (the locked field written twice), a host
    // change waits for it.
    await goToLine(p, 3);
    await p.keyboard.press('Shift+Alt+ArrowDown');
    await expect.poll(() => p.evaluate(() => window.editor.text.split('public_address').length)).toBe(3);
    expect(await swap('# soak', '# soaked')).toBeNull();
    await p.keyboard.press('Control+z');
    await expect.poll(() => p.evaluate(() => window.editor.value.startsWith('# soaked\n'))).toBe(true);
    expect(await swap('# soaked', '# soak')).toEqual(expect.any(Number));
    await expect.poll(() => p.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
    await goToLine(p, 1);
  },
  async changes(p) {
    await goToLine(p, 2);
    await p.keyboard.press('End');
    await p.keyboard.type('9');
    await p.keyboard.press('Control+Home');
    await p.keyboard.press('Alt+F5');
    await expect.poll(async () => (await caret(p)).line).toBe(2);
    await p.keyboard.press('Shift+Alt+F5');
    await p.keyboard.press('Control+Home');
    // The preview from the keyboard, then from the gutter marker.
    const peek = p.locator('.config-editor__diff-peek');
    await p.keyboard.press('Alt+F3');
    await expect(peek).toHaveCount(1);
    await p.keyboard.press('Escape');
    await expect(peek).toHaveCount(0);
    await p.locator('.config-editor__diff-marker-modified').click();
    await expect(peek).toHaveCount(1);
    await p.keyboard.press('Escape');
    await expect(peek).toHaveCount(0);
    await undoBack(p, 1);
    await goToLine(p, 1);
  },
  async replace(p) {
    await goToLine(p, 1);
    await p.keyboard.press('Control+h');
    await p.keyboard.type('key1');
    await expect(p.locator('.config-editor__search-match').first()).toBeAttached();
    const field = p.locator('.config-editor__replace-input');
    await field.fill('kei1');
    await field.press('Enter');
    await field.press('Control+Alt+Enter');
    await expect.poll(() => p.evaluate(() => window.editor.text.includes('key1'))).toBe(false);
    await p.keyboard.press('Escape');
    await expect(p.locator('.config-editor__find')).toBeHidden();
    await undoBack(p, 2);
    await goToLine(p, 1);
  },
  async lineCommands(p) {
    await goToLine(p, 12);
    await p.keyboard.press('Alt+ArrowDown');
    await p.keyboard.press('Alt+ArrowUp');
    await p.keyboard.press('Shift+Alt+ArrowDown');
    await p.keyboard.press('Control+Shift+k');
    await p.keyboard.press('Control+Shift+k');
    // Its copy and the line below it deleted.
    await expect.poll(() => p.evaluate(() => window.editor.text.includes('\nkey3: 3\n'))).toBe(false);
    await undoBack(p, 5);
    await goToLine(p, 1);
  },
  async dragMove(p) {
    // Line 12 (`key2: 2`) dragged below line 15.
    await goToLine(p, 12);
    await p.keyboard.press('Shift+ArrowDown');
    const from = await tokenAt(p, 12, 'key2');
    const to = await tokenAt(p, 16, 'key6');
    await p.mouse.move(from.x, from.y);
    await p.mouse.down();
    await p.mouse.move(from.x + 4, from.y, { steps: 3 });
    await p.mouse.move(to.x, to.y, { steps: 12 });
    await p.mouse.up();
    // The drop lands at the end of the task.
    await p.waitForTimeout(300);
    await expect.poll(() => p.evaluate(() => window.editor.text === window.soakDocument)).toBe(false);
    await undoBack(p, 1);
    await away(p);
    await goToLine(p, 1);
  },
};

/** The sample schema, with an `$id` and a field naming a client
 * (`default_client`), as `window.soakSchema`. */
const installSoakSchema = (browserPage) =>
  browserPage.evaluate(async () => {
    const schema = await (await fetch('../../schema.json')).json();
    schema.$id = 'https://example.com/soak';
    schema.properties.default_client = { type: 'string', 'x-config-editor-keys': '/clients' };
    window.soakSchema = JSON.stringify(schema);
  });

async function soakEditor(browserPage) {
  const errors = await open(browserPage, 'vanilla.html');
  await installSoakSchema(browserPage);
  await browserPage.evaluate(async (value) => {
    const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
    window.soakDocument = value;
    window.saved = [];
    window.editor = new ConfigEditor(document.getElementById('editor'), {
      value,
      schema: window.soakSchema,
      onSave: (text) => window.saved.push(text),
      settings: { showToolbar: true, showFooter: true },
      readOnlyPaths: ['/public_address'],
      secrets: { hint: 'Keeps the stored secret' },
    });
    window.hostProblems = Array.from({ length: 100 }, (_, index) => ({
      range: { startLine: index + 8, startColumn: 1, endLine: index + 8, endColumn: 3 },
      message: `problem ${index}`,
    }));
  }, soakDocument());
  await input(browserPage).click();
  await goToLine(browserPage, 1);
  return errors;
}

/** Logs what `cycle` leaves behind on its own, per cycle. */
async function report(browserPage, name, cycle) {
  try {
    await measure(browserPage, name, cycle);
  } catch (error) {
    console.log(`soak ${name}: failed (${String(error).split('\n')[0]})`);
  }
}

// Longer than any timer the editor sets (the protect tip hides after
// 2.5 s): a timer still pending holds its closure, which is not a leak.
const TIMERS_MS = 3000;

/** Waits out the editor's timers, then settles. */
const outlastTimers = async (browserPage) => {
  await browserPage.waitForTimeout(TIMERS_MS);
  await settle(browserPage);
};

async function measure(browserPage, name, cycle) {
  for (let round = 0; round < 3; round += 1) await cycle(browserPage);
  await outlastTimers(browserPage);
  const before = await heap(browserPage);
  for (let round = 0; round < 10; round += 1) await cycle(browserPage);
  await outlastTimers(browserPage);
  const after = await heap(browserPage);
  if (!before) {
    console.log(`soak ${name}: build the package with alloc-stats to measure`);
    return;
  }
  // The sizes whose live count grew by a cycle or more: where to look.
  const grown = after.sizes
    .map((count, size) => [size, count - before.sizes[size]])
    .filter(([, delta]) => delta >= 10)
    .map(([size, delta]) => `${size}B×${delta / 10}`)
    .slice(0, 8);
  console.log(`soak ${name}: ${Math.round((after.bytes - before.bytes) / 10)} B a cycle ${grown.join(' ')}`);
}

test.describe('memory soak', () => {
  test('every surface of an editor, cycled, keeps memory flat', async ({ page: browserPage }) => {
    test.setTimeout(600_000);
    const errors = await soakEditor(browserPage);
    if (process.env.MEMORY_SOAK_REPORT) {
      // `MEMORY_SOAK_REPORT=find,folding` measures only those.
      const only = process.env.MEMORY_SOAK_REPORT.split(',');
      for (const [name, cycle] of Object.entries(surfaces)) {
        if (only.includes('1') || only.includes(name)) await report(browserPage, name, cycle);
      }
    }
    const grown = await growths(browserPage, {
      warmUp: 3,
      cycles: 20,
      cycle: async () => {
        for (const cycle of Object.values(surfaces)) await cycle(browserPage);
      },
    });
    expect(grown.wasm).toBeLessThanOrEqual(256 * KB);
    expectNothingKept(grown.heap);
    expect(await browserPage.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
    expect(errors).toEqual([]);
  });

  test('documents switched, closed and reopened keep memory flat', async ({ page: browserPage }) => {
    test.setTimeout(300_000);
    const errors = await open(browserPage, 'documents.html');
    await installSoakSchema(browserPage);
    // `a` names a client only `b` defines.
    await browserPage.evaluate((value) => {
      const values = {
        a: value.replace('default_client: acme', 'default_client: bravo'),
        b: value.replace('  acme:', '  bravo:'),
        c: value,
      };
      window.files = ['a', 'b', 'c'].map((id) => ({
        id,
        fileName: `${id}.yaml`,
        value: values[id],
        schema: window.soakSchema,
      }));
      window.documentsView.update({ documents: window.files });
    }, soakDocument());
    const tab = (id) => browserPage.locator(`.config-editor-documents__tab[data-document-id="${id}"]`);
    const shownInput = browserPage.locator(
      '.config-editor-documents__panel:not(.config-editor-documents__panel-hidden) .config-editor__input',
    );
    const cycle = async () => {
      // Switch through every document (each editor stays mounted).
      for (const id of ['b', 'c', 'a']) {
        await tab(id).click();
        await expect(tab(id)).toHaveAttribute('aria-selected', 'true');
      }
      // Close one and reopen it: a new editor each time.
      await browserPage.evaluate(() =>
        window.documentsView.update({ documents: window.files.filter((file) => file.id !== 'c') }),
      );
      await expect(tab('c')).toHaveCount(0);
      await browserPage.evaluate(() => window.documentsView.update({ documents: window.files }));
      await tab('c').click();
      await expect(shownInput).toBeVisible();
      await tab('a').click();
      // Go to definition into `b`, its uses back into `a`, and back and
      // forward across them.
      await expect(tab('a')).toHaveAttribute('aria-selected', 'true');
      await shownInput.evaluate((element) => {
        const at = element.value.indexOf('default_client: bravo') + 18;
        element.focus();
        element.setSelectionRange(at, at);
      });
      await browserPage.keyboard.press('F12');
      await expect(tab('b')).toHaveAttribute('aria-selected', 'true');
      const selected = () =>
        shownInput.evaluate((element) => element.value.slice(element.selectionStart, element.selectionEnd));
      await expect.poll(selected).toBe('bravo');
      await browserPage.keyboard.press('Shift+F12');
      const uses = browserPage.locator('.config-editor__uses [role="option"]');
      await expect(uses).toHaveCount(1);
      await browserPage.keyboard.press('Enter');
      await expect(tab('a')).toHaveAttribute('aria-selected', 'true');
      await browserPage.keyboard.press('Alt+ArrowLeft');
      await expect(tab('b')).toHaveAttribute('aria-selected', 'true');
      await browserPage.keyboard.press('Alt+ArrowRight');
      await expect(tab('a')).toHaveAttribute('aria-selected', 'true');
    };
    if (process.env.MEMORY_SOAK_REPORT) await report(browserPage, 'documents', cycle);
    // Each editor's DOM ids carry its number, one more per reopened
    // document: the warm-up takes it to two digits, so the ids of the
    // editor shown last don't grow by one over the measured cycles.
    const grown = await growths(browserPage, { warmUp: 10, cycles: 20, cycle });
    expect(grown.wasm).toBeLessThanOrEqual(256 * KB);
    expectNothingKept(grown.heap);
    expect(errors).toEqual([]);
  });
});
