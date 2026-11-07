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
    const grown = await growth(browserPage, {
      cycles: 150,
      cycle: async () => {
        for (const on of [false, true]) {
          await browserPage.evaluate(
            ({ parts, on }) =>
              window.editor.setSettings({
                ...window.editor.settings,
                ...Object.fromEntries(parts.map((part) => [part, on])),
              }),
            { parts, on },
          );
          await expect(browserPage.locator('.config-editor__toolbar')).toHaveCount(on ? 1 : 0);
        }
      },
    });
    // Rebuilt at each switch, the parts grew it by about 3.5 KB a cycle.
    expect(grown).toBeLessThanOrEqual(192 * KB);
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
    await p.evaluate(() => window.editor.setBaseline(window.soakDocument.replace('7835', '7836')));
    await expect(p.locator('.config-editor__notice')).toBeVisible();
    await p.locator('.config-editor__notice-keep').click();
    await expect(p.locator('.config-editor__notice')).toBeHidden();
    await input(p).focus();
    await p.keyboard.press('Control+z');
    await p.evaluate(() => window.editor.setBaseline(window.soakDocument));
    await expect.poll(() => p.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
  },
  async settings(p) {
    for (const on of [false, true]) {
      await p.evaluate(
        (on) =>
          window.editor.setSettings({
            ...window.editor.settings,
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
    await p.evaluate(() => window.editor.setHostDiagnostics(window.hostProblems));
    await expect(p.locator('.config-editor__diagnostic-marker-host').first()).toBeAttached();
    await p.evaluate(() => window.editor.setHostDiagnostics([]));
    await expect(p.locator('.config-editor__diagnostic-marker-host')).toHaveCount(0);
  },
};

async function soakEditor(browserPage) {
  const errors = await open(browserPage, 'vanilla.html');
  await browserPage.evaluate(async (value) => {
    const { ConfigEditor } = await import('../config-editor/pkg/config_editor.js');
    const schema = await (await fetch('../../schema.json')).text();
    window.soakDocument = value;
    window.editor = new ConfigEditor(document.getElementById('editor'), {
      value,
      schema,
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

/** Live heap bytes and live allocations per size, in a package built
 * with `alloc-stats` (else null). */
const heap = (browserPage) =>
  browserPage.evaluate(async () => {
    const module = await import('../config-editor/pkg/config_editor.js');
    return module.__allocStats ? { bytes: module.__allocStats()[0], sizes: Array.from(module.__allocSizes()) } : null;
  });

/** Logs what `cycle` leaves behind on its own, per cycle. */
async function report(browserPage, name, cycle) {
  try {
    await measure(browserPage, name, cycle);
  } catch (error) {
    console.log(`soak ${name}: failed (${String(error).split('\n')[0]})`);
  }
}

async function measure(browserPage, name, cycle) {
  for (let round = 0; round < 3; round += 1) await cycle(browserPage);
  await settle(browserPage);
  const before = await heap(browserPage);
  for (let round = 0; round < 10; round += 1) await cycle(browserPage);
  await settle(browserPage);
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
    const grown = await growth(browserPage, {
      warmUp: 3,
      cycles: 20,
      cycle: async () => {
        for (const cycle of Object.values(surfaces)) await cycle(browserPage);
      },
    });
    expect(grown).toBeLessThanOrEqual(256 * KB);
    expect(await browserPage.evaluate(() => window.editor.value === window.soakDocument)).toBe(true);
    expect(errors).toEqual([]);
  });

  test('documents switched, closed and reopened keep memory flat', async ({ page: browserPage }) => {
    test.setTimeout(300_000);
    const errors = await open(browserPage, 'documents.html');
    await browserPage.evaluate((value) => {
      window.files = ['a', 'b', 'c'].map((id) => ({ id, fileName: `${id}.yaml`, value }));
      window.documentsView.setDocuments(window.files);
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
        window.documentsView.setDocuments(window.files.filter((file) => file.id !== 'c')),
      );
      await expect(tab('c')).toHaveCount(0);
      await browserPage.evaluate(() => window.documentsView.setDocuments(window.files));
      await tab('c').click();
      await expect(shownInput).toBeVisible();
      await tab('a').click();
    };
    if (process.env.MEMORY_SOAK_REPORT) await report(browserPage, 'documents', cycle);
    const grown = await growth(browserPage, { warmUp: 3, cycles: 20, cycle });
    // Each reopened editor keeps its state's handles (under 1 KB).
    expect(grown).toBeLessThanOrEqual(256 * KB);
    expect(errors).toEqual([]);
  });
});
