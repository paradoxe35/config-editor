import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { FRAMEWORK_PAGES } from './helpers.mjs';

// QA round 29 (host API). A text the host gives of its own replaces the
// document, and the texts the editor sent before it belong to the text it
// replaced: going back to one of them (a file picker back on the first
// file) applies, with `echoes: true` (ConfigEditor, ConfigDocuments) and in
// the framework components. Ranges given with a skipped echo are in the
// echoed text: they are mapped to the document, so a lock holds, typing is
// kept and an update never throws. An update's `value` is the host's word,
// the constructor's text included (a reset). A custom language's unknown
// token kind is warned about.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

function listen(page) {
  page.errors = [];
  page.warnings = [];
  page.on('console', (message) => {
    if (message.type() === 'error') page.errors.push(message.text());
    if (message.type() === 'warning') page.warnings.push(message.text());
  });
  page.on('pageerror', (error) => page.errors.push(String(error)));
}

async function open(page) {
  listen(page);
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.module = await import('../config-editor/index.js');
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.mountPoint = () => {
      document.querySelectorAll('[data-host]').forEach((element) => element.remove());
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '240px';
      document.body.prepend(element);
      return element;
    };
  });
}

/** A framework page whose parent the test plays: `window.bind(value,
 * onChange, props)` renders the component with them (Vue's `v-model` props
 * for Vue). */
async function openFramework(page, name) {
  listen(page);
  // `?random`: Svelte's one-way `value` (the test's parent hands it back).
  await page.goto(example(name) + '?random');
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  await page.evaluate((vue) => {
    window.pause = (ms = 400) => new Promise((resolve) => setTimeout(resolve, ms));
    window.bind = (value, onChange, props = {}) =>
      window.setExtraProps(vue ? { modelValue: value, 'onUpdate:modelValue': onChange, ...props } : { value, onChange, ...props });
  }, name.startsWith('vue'));
}

const shownText = (page) => page.evaluate(() => document.querySelector('.config-editor__input').value);

// --- H29-1: a file picker over one editor. -----------------------------------

// How the parent stores each edit of the file shown and renders it back:
// - debounce: stored at once, rendered 300 ms after the last edit (a switch
//   renders at once);
// - random: stored and rendered 0-200 ms late (a fetch per edit), into the
//   file it was made in;
// - fetch: stored 150 ms late, rendered only while that file is shown.
const PARENTS = ['debounce', 'random', 'fetch'];

/** The parent's part, run in the page: `window.files`, `window.current`,
 * `stored(text)` for each edit, `window.switchTo(name)`; `render()` shows
 * `window.files[window.current]`. */
function parentScript(parent) {
  return `
    window.files = { A: 'a: 1\\n', B: 'b: 2\\n' };
    window.current = 'A';
    let timer = null;
    window.stored = (text) => {
      const file = window.current;
      if (${JSON.stringify(parent)} === 'debounce') {
        window.files[file] = text;
        clearTimeout(timer);
        timer = setTimeout(() => window.render(), 300);
        return;
      }
      const random = ${JSON.stringify(parent)} === 'random';
      setTimeout(() => {
        window.files[file] = text;
        if (random || window.current === file) window.render();
      }, random ? Math.random() * 200 : 150);
    };
    window.switchTo = (name) => {
      window.current = name;
      clearTimeout(timer);
      window.render();
    };
  `;
}

/** Types `key` at the end of the first line. */
async function typeOnFirstLine(page, key) {
  await page.locator('.config-editor__input').focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('End');
  await page.keyboard.type(key);
}

/** Types in A, switches to B, types there, back to A, back to B: what the
 * editor showed on each file and what the parent holds. */
async function pickFiles(page, gap) {
  await typeOnFirstLine(page, '9');
  await page.waitForTimeout(gap);
  await page.evaluate(() => window.switchTo('B'));
  await page.waitForTimeout(700);
  const onB = await shownText(page);
  await typeOnFirstLine(page, '7');
  await page.waitForTimeout(gap);
  await page.evaluate(() => window.switchTo('A'));
  await page.waitForTimeout(900);
  const backOnA = await shownText(page);
  await page.evaluate(() => window.switchTo('B'));
  await page.waitForTimeout(900);
  const backOnB = await shownText(page);
  return { onB, backOnA, backOnB, files: await page.evaluate(() => window.files) };
}

const PICKED = { onB: 'b: 2\n', backOnA: 'a: 19\n', backOnB: 'b: 27\n', files: { A: 'a: 19\n', B: 'b: 27\n' } };

test.describe('QA round 29: a file picker over one editor', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const host of ['editor', 'one document']) {
    test(`${host} with echoes: going back to a file applies its text, and nothing reports another file's text`, async ({ page }) => {
      test.slow();
      await open(page);
      const failures = [];
      for (const parent of PARENTS) {
        for (const gap of [50, 600]) {
          await page.evaluate(
            async ([host, script]) => {
              new Function(script)();
              const element = window.mountPoint();
              if (host === 'editor') {
                const editor = new window.module.ConfigEditor(element, {
                  value: window.files.A,
                  echoes: true,
                  onChange: (text) => window.stored(text),
                });
                window.render = () => editor.update({ value: window.files[window.current] });
              } else {
                const list = () => [{ id: 'main', fileName: 'main.yaml', value: window.files[window.current] }];
                const view = new window.module.ConfigDocuments(element, {
                  documents: list(),
                  echoes: true,
                  onChange: (_, text) => window.stored(text),
                });
                window.render = () => view.update({ documents: list() });
              }
              await window.pause(400);
            },
            [host, parentScript(parent)],
          );
          const picked = await pickFiles(page, gap);
          if (JSON.stringify(picked) !== JSON.stringify(PICKED)) failures.push({ parent, gap, ...picked });
        }
      }
      expect(failures).toEqual([]);
      expect(page.errors).toEqual([]);
    });
  }

  for (const name of FRAMEWORK_PAGES) {
    for (const parent of PARENTS) {
      test(`${name}, a ${parent} parent: going back to a file applies its text`, async ({ page }) => {
        const failures = [];
        for (const gap of [50, 600]) {
          await openFramework(page, name);
          await page.evaluate(async (script) => {
            new Function(script)();
            window.render = () => window.bind(window.files[window.current], window.stored);
            window.render();
            await window.pause(300);
          }, parentScript(parent));
          const picked = await pickFiles(page, gap);
          if (JSON.stringify(picked) !== JSON.stringify(PICKED)) failures.push({ gap, ...picked });
        }
        expect(failures).toEqual([]);
        expect(page.errors).toEqual([]);
      });
    }
  }
});

// --- H29-2: ranges given with a skipped echo. --------------------------------

const START = `name: web\nid: srv-42\nnote: ${'x'.repeat(30)}\n`;
const TYPED = `# a long comment line here\nname: web\nid: srv-42\nnote: ${'x'.repeat(10)}\n`;

/** The host's ranges for `text`: `id: srv-42` locked, a note on `note:`'s
 * line (to the text's end, past its last break). */
const RANGES = `
  window.ranges = (text) => {
    const id = text.indexOf('id: ');
    const note = text.lastIndexOf('note: ');
    return {
      readOnlyRanges: id < 0 ? [] : [{ start: id, end: text.indexOf('\\n', id) }],
      hostDiagnostics: note < 0 ? [] : [{ range: { start: note, end: text.length - 1 }, message: 'note checked', severity: 'info' }],
    };
  };
`;

/** Types a comment line at the top and at once `ZZ` after the locked
 * value, then deletes 20 characters back from the note's end, each echo
 * coming back 300 ms late with its ranges. */
async function typeAroundRanges(page) {
  await page.locator('.config-editor__input').focus();
  await page.keyboard.press('Control+Home');
  await page.keyboard.type('# a long comment line here\n', { delay: 15 });
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('End');
  await page.keyboard.type('ZZ', { delay: 15 });
  await page.waitForTimeout(800);
  await page.keyboard.press('Control+End');
  await page.keyboard.press('ArrowLeft');
  for (let index = 0; index < 20; index += 1) {
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(15);
  }
  await page.waitForTimeout(800);
}

test.describe('QA round 29: ranges given with a skipped echo', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const host of ['editor', 'one document']) {
    test(`${host} with echoes: the lock holds, typing is kept, and no update throws`, async ({ page }) => {
      await open(page);
      await page.evaluate(
        async ([host, start, ranges]) => {
          new Function(ranges)();
          const element = window.mountPoint();
          window.held = start;
          if (host === 'editor') {
            const editor = new window.module.ConfigEditor(element, {
              value: start,
              echoes: true,
              ...window.ranges(start),
              onChange: (text) =>
                setTimeout(() => {
                  window.held = text;
                  editor.update({ value: text, ...window.ranges(text) });
                }, 300),
            });
            window.shown = () => editor.value;
          } else {
            const list = () => [{ id: 'a', fileName: 'a.yaml', value: window.held, ...window.ranges(window.held) }];
            const view = new window.module.ConfigDocuments(element, {
              documents: list(),
              echoes: true,
              onChange: (_, text) =>
                setTimeout(() => {
                  window.held = text;
                  view.update({ documents: list() });
                }, 300),
            });
            window.shown = () => view.editor('a').value;
          }
          await window.pause(400);
        },
        [host, START, RANGES],
      );
      await typeAroundRanges(page);
      expect(await page.evaluate(() => ({ text: window.shown(), held: window.held }))).toEqual({ text: TYPED, held: TYPED });
      expect(page.errors).toEqual([]);
    });
  }

  for (const name of FRAMEWORK_PAGES) {
    test(`${name}, a parent 300 ms late: the lock holds, typing is kept, and no update fails`, async ({ page }) => {
      await openFramework(page, name);
      await page.evaluate(
        async ([start, ranges]) => {
          new Function(ranges)();
          window.held = start;
          const onChange = (text) =>
            setTimeout(() => {
              window.held = text;
              window.bind(text, onChange, window.ranges(text));
            }, 300);
          window.bind(start, onChange, window.ranges(start));
          await window.pause(400);
        },
        [START, RANGES],
      );
      await typeAroundRanges(page);
      expect({ text: await shownText(page), held: await page.evaluate(() => window.held) }).toEqual({ text: TYPED, held: TYPED });
      expect(page.errors).toEqual([]);
    });
  }

  test('rangesFor: ranges checked for an older text are checked in it and moved to the document', async ({ page }) => {
    await open(page);
    const result = await page.evaluate(async () => {
      const older = 'name: web\nport: 80\n';
      const editor = new window.module.ConfigEditor(window.mountPoint(), { value: '# added\nname: web\n' });
      const port = older.indexOf('port');
      const thrown = (apply) => {
        try {
          apply();
          return null;
        } catch (error) {
          return String(error.message);
        }
      };
      // Valid for the older text, past the document's end: no throw.
      const valid = thrown(() =>
        editor.update({
          rangesFor: older,
          readOnlyRanges: [{ start: 0, end: 4 }],
          hostDiagnostics: [{ range: { start: port, end: older.length - 1 }, message: 'port checked' }],
        }),
      );
      await window.pause(200);
      return {
        valid,
        past: thrown(() => editor.update({ rangesFor: older, readOnlyRanges: [{ start: 0, end: older.length + 1 }] })),
        notText: thrown(() => editor.update({ rangesFor: 5 })),
      };
    });
    expect(result.valid).toBeNull();
    expect(result.past).toContain("past the text's end (19)");
    expect(result.notText).toContain('rangesFor');
    expect(page.errors).toEqual([]);
    expect(page.warnings).toEqual([]);
  });
});

// --- M29-3: an update's value is the host's word. ----------------------------

test.describe('QA round 29: a reset to the text the host gave last', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('with echoes on their way applies at once, and the editor ends where the host does', async ({ page }) => {
    test.slow();
    await open(page);
    const failures = [];
    for (const kind of ['slow', 'latest', 'random']) {
      for (const drops of [true, false]) {
        await page.evaluate(
          async ([kind, drops]) => {
            const original = 'name: x\n';
            window.store = original;
            // A host that drops the answers to what it sent before a reset
            // (`drops`), or one that takes every answer.
            let version = 0;
            let timer = null;
            let pending = null;
            const element = window.mountPoint();
            const editor = new window.module.ConfigEditor(element, {
              value: original,
              echoes: true,
              onChange: (text) => {
                const sent = version;
                const apply = (text) => {
                  if (drops && sent !== version) return;
                  window.store = text;
                  editor.update({ value: text });
                };
                if (kind === 'slow') setTimeout(() => apply(text), 600);
                if (kind === 'random') setTimeout(() => apply(text), Math.random() * 200);
                if (kind === 'latest') {
                  pending = text;
                  clearTimeout(timer);
                  timer = setTimeout(() => apply(pending), 150);
                }
              },
            });
            await window.pause(150);
            editor.revealRange({ start: 7, end: 7 });
            window.editorUnderTest = editor;
            window.reset = () => {
              version += 1;
              window.store = original;
              editor.update({ value: original });
              return editor.value;
            };
          },
          [kind, drops],
        );
        await page.locator('[data-host] .config-editor__input').focus();
        await page.keyboard.type('abc', { delay: 20 });
        const afterReset = await page.evaluate(() => window.reset());
        await page.waitForTimeout(30);
        await page.evaluate(() => window.editorUnderTest.revealRange({ start: 7, end: 7 }));
        await page.keyboard.type('Q', { delay: 20 });
        await page.waitForTimeout(1500);
        const end = await page.evaluate(() => ({ text: window.editorUnderTest.value, store: window.store }));
        const expected = drops ? { text: 'name: xQ\n', store: 'name: xQ\n' } : { text: end.store, store: end.store };
        if (afterReset !== 'name: x\n' || JSON.stringify(end) !== JSON.stringify(expected)) {
          failures.push({ kind, drops, afterReset, ...end });
        }
      }
    }
    expect(failures).toEqual([]);
    expect(page.errors).toEqual([]);
  });
});

// --- L29-4: a custom language's unknown token kind. --------------------------

test.describe('QA round 29: custom language token kinds', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('an unknown kind from highlight is warned about once, and read as a value', async ({ page }) => {
    await open(page);
    const classes = await page.evaluate(async () => {
      const name = window.module.registerLanguage({
        label: 'Kinds',
        extensions: ['kinds'],
        parse: (text) => Object.fromEntries(text.split('\n').filter(Boolean).map((line) => line.split('='))),
        serialize: (value) => Object.entries(value).map(([key, item]) => `${key}=${item}\n`).join(''),
        highlight: (line) => [{ start: 0, end: line.length, kind: line.startsWith('A') ? 'nonsense' : 'value' }],
      });
      const editor = new window.module.ConfigEditor(window.mountPoint(), { language: name, value: 'A=1\nA=2\nB=3\n' });
      await window.pause(300);
      editor.revealRange({ start: 3, end: 3 });
      await window.pause(100);
      return [...document.querySelectorAll('[data-host] .config-editor__highlight span')]
        .filter((span) => span.children.length === 0 && span.textContent.startsWith('A'))
        .map((span) => span.className);
    });
    await page.locator('[data-host] .config-editor__input').focus();
    await page.keyboard.type('x');
    await page.waitForTimeout(200);
    const warned = page.warnings.filter((text) => text.includes("Kinds's highlight()"));
    expect(warned).toEqual([
      'config-editor: Kinds\'s highlight() gave a token of kind "nonsense", not one of "key", "string", "value", "literal", "comment", "punctuation", "reference" or "interpolation"; read as "value"',
    ]);
    expect(classes.length).toBeGreaterThan(0);
    expect(new Set(classes)).toEqual(new Set(['config-editor__value']));
    expect(page.errors).toEqual([]);
  });
});
