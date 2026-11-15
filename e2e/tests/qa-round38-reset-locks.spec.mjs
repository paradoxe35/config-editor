import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 38: the read-only ranges a host gave, through its own new texts
// (`replaceValue`, a new `value` given alone). Each reset shows the locks
// carried afresh from the ranges as given and the text they were given for:
// - H38-1: two resets in one task left the lock on the wrong line (the
//   second moved the editor's ranges, still in the first text, as if they
//   were in the first reset's), and the locked value was edited and saved.
// - H38-2: a reset wrote its carried ranges over the given ones, so a later
//   `update({ value })` without ranges read them as given for it.
// - H38-3: a reset to a newer version of the file locked every changed line
//   after the lock, and a lock whose line moved further down was dropped.
// Every reset kind, for a `ConfigEditor` and a document of a
// `ConfigDocuments` (shown, hidden, never shown), both range forms, with
// and without echoes: the locked line is the one holding the locked text
// (else the line aligned with it), it cannot change, and nothing else is
// locked.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

const T = 'name: a\nsecret: keep\nlisten: 1\n';
const ABOVE = `z: 0\n${T}`;

/** The reset kinds: what the host does after mounting with the lock on
 * `secret: keep` (line 2 of `T`), and the line locked when the final text
 * holds no `secret: keep` line. `give(value, ranges)` is a host's update
 * (for a document, its list); `reset(text)` is `replaceValue`. */
const KINDS = {
  'the same text': async ({ reset }) => reset(T),
  'a line inserted above': async ({ reset }) => reset(ABOVE),
  'a line inserted below': async ({ reset }) => reset('name: a\nsecret: keep\nnew: 1\nlisten: 1\n'),
  'the locked line changed': async ({ reset }) => reset('name: a\nsecret: other\nlisten: 1\n'),
  'the locked line moved later': async ({ reset }) => reset('name: a\nlisten: 1\nsecret: keep\n'),
  'the locked line moved earlier': async ({ reset }) => reset('secret: keep\nname: a\nlisten: 1\n'),
  'unrelated text': async ({ reset }) => reset('host: h\nport: 2\nmode: m\n'),
  'the locked line written twice': async ({ reset }) => reset('secret: keep\nname: a\nsecret: keep\nlisten: 1\n'),
  'two resets in one task': async ({ reset }) => {
    reset(ABOVE);
    reset(T);
  },
  'two resets in one task, both moving the lock': async ({ reset }) => {
    reset(ABOVE);
    reset(`x: 1\ny: 2\n${T}`);
  },
  'a reset, then the first text with the ranges again': async ({ reset, give, wait }) => {
    reset(ABOVE);
    await wait();
    give(T, true);
  },
  'a reset, then the reset text with the ranges again': async ({ reset, give, wait }) => {
    reset(ABOVE);
    await wait();
    give(ABOVE, true);
  },
  'a reset, then the first text alone': async ({ reset, give, wait }) => {
    reset(ABOVE);
    await wait();
    give(T, false);
  },
  'a reset, then another text alone in the same task': async ({ reset, give }) => {
    reset(ABOVE);
    give(`x: 1\ny: 2\n${T}`, false);
  },
};

/** The kinds a document's list cannot give (a list without ranges clears them). */
const RANGES_ALONE = ['a reset, then the first text alone', 'a reset, then another text alone in the same task'];

const FALLBACK_LINE = 2;

/** The kinds whose host gives its ranges again for its new text: they lock
 * the places they name there (QA 38, FW38-1: a host re-sending its static
 * ranges after a reset means them for the text it now holds). */
const GIVEN_FOR_NEW_TEXT = { 'a reset, then the reset text with the ranges again': 2 };

/** The kinds whose text holds the locked line more than once: the lock
 * stays on the one the line diff keeps. */
const KEPT_LINE = { 'the locked line written twice': 3 };

/** Mounts on the vanilla page a `ConfigEditor` (`where` 'editor') or a
 * `ConfigDocuments` whose document `b` holds `T` with its lock, `shown`,
 * `hidden` (shown, then another selected) or `never` shown. */
async function mount(page, { where, form, echoes }) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(example('documents.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(
    async ({ where, form, echoes, T }) => {
      window.documentsView.destroy();
      const module = await import('../config-editor/index.js');
      const element = document.createElement('div');
      element.id = 'probe';
      element.style.height = '420px';
      document.body.prepend(element);
      const lock =
        form === 'line'
          ? [{ startLine: 2, startColumn: 1, endLine: 2, endColumn: 13 }]
          : [{ start: T.indexOf('secret'), end: T.indexOf('secret') + 'secret: keep'.length }];
      window.saved = [];
      window.T = T;
      window.ABOVE = `z: 0\n${T}`;
      const onChange = () => {};
      if (where === 'editor') {
        window.view = new module.ConfigEditor(element, {
          language: 'yaml',
          value: T,
          readOnlyRanges: lock,
          echoes,
          onChange,
          onSave: (text) => window.saved.push(text),
        });
        window.reset = (text) => window.view.replaceValue(text);
        window.give = (value, ranges) => window.view.update({ value, ...(ranges ? { readOnlyRanges: lock } : {}) });
        window.target = () => window.view;
      } else {
        const list = (value) => [
          { id: 'a', fileName: 'a.yaml', value: 'a: 1\n' },
          { id: 'b', fileName: 'b.yaml', value, readOnlyRanges: lock },
        ];
        window.view = new module.ConfigDocuments(element, {
          documents: list(T),
          activeId: where === 'never' ? 'a' : 'b',
          echoes,
          onChange,
          onSave: (id, text) => window.saved.push(text),
        });
        if (where === 'hidden') {
          await new Promise((resolve) => setTimeout(resolve, 60));
          window.view.select('a');
        }
        window.reset = (text) => window.view.replaceValue('b', text);
        window.give = (value) => window.view.update({ documents: list(value) });
        window.show = () => window.view.select('b');
        window.target = () => window.view.editor('b');
      }
      await new Promise((resolve) => setTimeout(resolve, 60));
    },
    { where, form, echoes, T },
  );
}

/** The lines (one-based) a user cannot change: on each, the caret two
 * characters before its end (inside a value, not at a lock's edge) and an
 * `X` typed with the keyboard. */
async function lockedLines(page, where = 'editor') {
  const input = page.locator(
    where === 'editor' ? '#probe textarea.config-editor__input' : '#probe .config-editor-documents__panel:visible textarea.config-editor__input',
  );
  await page.evaluate(() => window.show?.());
  await page.waitForTimeout(100);
  const text = () => page.evaluate(() => window.target().value);
  const lines = (await text()).split('\n');
  const locked = [];
  for (let at = 0; at < lines.length; at += 1) {
    const before = await text();
    await input.focus();
    await page.keyboard.press('Control+Home');
    for (let step = 0; step < at; step += 1) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('End');
    if (lines[at].length >= 2) for (const _ of [0, 1]) await page.keyboard.press('ArrowLeft');
    await page.keyboard.type('X');
    await page.waitForTimeout(40);
    if ((await text()) === before) locked.push(at + 1);
    else {
      await page.keyboard.press('Backspace');
      await page.waitForTimeout(20);
    }
  }
  return { locked, text: await text() };
}

const WHERE = ['editor', 'shown', 'hidden', 'never'];

for (const where of WHERE) {
  const name = where === 'editor' ? 'ConfigEditor' : `ConfigDocuments, a document ${where === 'never' ? 'never shown' : where}`;
  for (const echoes of [false, true]) {
    test.describe(`${name}${echoes ? ', echoes' : ''}: a host's new text carries the lock`, () => {
      for (const [kind, act] of Object.entries(KINDS)) {
        if (where !== 'editor' && RANGES_ALONE.includes(kind)) continue;
        for (const form of ['line', 'index']) {
          test(`${kind} (${form} ranges)`, async ({ page }) => {
            await mount(page, { where, form, echoes });
            await page.evaluate(`window.act = ${act.toString()}`);
            await page.evaluate(async () => {
              const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
              await window.act({ reset: window.reset, give: window.give, wait });
            });
            await page.waitForTimeout(150);
            const { locked, text } = await lockedLines(page, where);
            const holding = text.split('\n').indexOf('secret: keep');
            // (The index form's stretch there starts on line 2 and ends inside
            // line 3's key.)
            if (kind in GIVEN_FOR_NEW_TEXT && form === 'index') expect(locked, text).toContain(GIVEN_FOR_NEW_TEXT[kind]);
            else if (kind in GIVEN_FOR_NEW_TEXT) expect(locked, text).toEqual([GIVEN_FOR_NEW_TEXT[kind]]);
            else expect(locked, text).toEqual([KEPT_LINE[kind] ?? (holding >= 0 ? holding + 1 : FALLBACK_LINE)]);
            // What a save sends keeps the locked value.
            await page.evaluate(() => window.target().save());
            const saved = await page.evaluate(() => window.saved);
            if (holding >= 0 && !(kind in GIVEN_FOR_NEW_TEXT)) for (const each of saved) expect(each.split('\n')).toContain('secret: keep');
            expect(page.errors).toEqual([]);
          });
        }
      }
    });
  }
}

// H38-3 in each language: a reset to a newer version of the file, every
// line changed, the header (or version) locked: only its line stays locked.
const VERSIONS = {
  yaml: ['# managed by ops v1\nname: a\nport: 1\n', '# managed by ops v2\nname: b\nport: 2\nmode: m\n', 1],
  json: ['{\n  "version": 1,\n  "name": "a"\n}\n', '{\n  "version": 2,\n  "name": "b",\n  "port": 3\n}\n', 2],
  toml: ['version = 1\nname = "a"\n', 'version = 2\nname = "b"\nport = 3\n', 1],
};

for (const [language, [first, next, line]] of Object.entries(VERSIONS)) {
  for (const resend of [false, true]) {
    test(`${language}: a reset to a newer version locks only the aligned line${resend ? ', the ranges given again' : ''}`, async ({ page }) => {
      page.errors = [];
      page.on('pageerror', (error) => page.errors.push(String(error)));
      await page.goto(example('documents.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(
        async ({ language, first, next, line, resend }) => {
          window.documentsView.destroy();
          const module = await import('../config-editor/index.js');
          const element = document.createElement('div');
          element.id = 'probe';
          element.style.height = '420px';
          document.body.prepend(element);
          const width = first.split('\n')[line - 1].length;
          const lock = [{ startLine: line, startColumn: 1, endLine: line, endColumn: width + 1 }];
          window.view = new module.ConfigEditor(element, { language, value: first, readOnlyRanges: lock, onChange: () => {} });
          window.target = () => window.view;
          await new Promise((resolve) => setTimeout(resolve, 50));
          window.view.replaceValue(next);
          if (resend) window.view.update({ readOnlyRanges: lock });
          await new Promise((resolve) => setTimeout(resolve, 50));
        },
        { language, first, next, line, resend },
      );
      const { locked } = await lockedLines(page);
      expect(locked).toEqual([line]);
      expect(page.errors).toEqual([]);
    });
  }
}
