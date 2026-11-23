import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Locks named by the text they were worked out on (`rangesFor`: a revision
// `onChange` gave, or a text), on a vanilla `ConfigEditor` and a
// `ConfigDocuments` whose host hands every text back late: the lock lands
// on the characters it was given for in whatever the user typed since,
// and stays there as the late echoes come in. The host's own change on an
// older revision (`applyChanges`) is rebased over the typing.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

const TEXT = 'name: a\nport: 80\nhost: h\n';
const LOCKED = 'host: h';

async function open(page) {
  const errors = [];
  page.warnings = [];
  page.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  page.on('console', (message) => message.type() === 'warning' && page.warnings.push(message.text()));
  page.on('pageerror', (error) => errors.push(String(error)));
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async () => {
    window.editor?.destroy();
    window.module = await import('../config-editor/index.js');
  });
  return errors;
}

// Mounts `kind` ('editor' | 'documents') on TEXT, its host holding each
// text with its revision until `flushEchoes()` hands them back, newest
// first. `host()` is the host's `{ text, revision }`; `lock(range,
// rangesFor)` gives a lock; `change(changes, revision)` its own change.
async function mount(page, kind) {
  await page.evaluate(
    ([kind, text]) => {
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '300px';
      document.body.prepend(element);
      const held = [];
      window.flushEchoes = () => held.splice(0).reverse().forEach((deliver) => deliver());
      let host = { text, revision: undefined };
      window.host = () => host;
      // The newest revision wins: a late, older one is ignored.
      const keep = (value, revision) => {
        if (host.revision !== undefined && revision < host.revision) return false;
        host = { text: value, revision };
        return true;
      };
      if (kind === 'documents') {
        let files = [{ id: 'm', fileName: 'm.yaml', value: text }];
        const set = (change) => {
          files = files.map((file) => ({ ...file, ...change }));
          view.update({ documents: files });
        };
        const view = new module.ConfigDocuments(element, {
          documents: files,
          echoes: true,
          onChange: (id, value, revision) =>
            held.push(() => keep(value, revision) && set({ value, valueRevision: revision })),
        });
        window.lock = (range, rangesFor) => set({ readOnlyRanges: [range], rangesFor });
        window.problems = (list, diagnosticsFor) => set({ hostDiagnostics: list, diagnosticsFor });
        window.change = (changes, revision) => view.applyChanges('m', changes, { revision });
      } else {
        const view = new module.ConfigEditor(element, {
          language: 'yaml',
          value: text,
          echoes: true,
          onChange: (value, revision) =>
            held.push(() => keep(value, revision) && view.update({ value, valueRevision: revision })),
        });
        window.lock = (range, rangesFor) => view.update({ readOnlyRanges: [range], rangesFor });
        window.problems = (list, diagnosticsFor) => view.update({ hostDiagnostics: list, diagnosticsFor });
        window.change = (changes, revision) => view.applyChanges(changes, { revision });
      }
    },
    [kind, TEXT],
  );
}

const input = (page) => page.locator('[data-host] .config-editor__input');
const marks = (page) => page.locator('[data-host] .config-editor__locked');
async function typeAt(page, offset, text) {
  await input(page).evaluate((element, offset) => {
    element.focus();
    element.setSelectionRange(offset, offset);
  }, offset);
  await page.keyboard.insertText(text);
}

test.describe('locks named by their text', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const kind of ['editor', 'documents']) {
    test(`${kind}: a lock for the host's revision lands on its field in the text typed since, through late echoes`, async ({ page }) => {
      const errors = await open(page);
      await mount(page, kind);
      await typeAt(page, 0, 'a: 1\n');
      await page.evaluate(() => window.flushEchoes());
      const host = await page.evaluate(() => window.host());
      expect(host.text).toBe(`a: 1\n${TEXT}`);
      // Typed above the field while the host works the lock out on its text.
      await typeAt(page, 0, 'b: 2\nc: 3\n');
      const start = host.text.indexOf(LOCKED);
      await page.evaluate(([range, revision]) => window.lock(range, revision), [{ start, end: start + LOCKED.length }, host.revision]);
      await expect(marks(page)).toHaveText([LOCKED]);
      await page.evaluate(() => window.flushEchoes());
      await expect(input(page)).toHaveValue(`b: 2\nc: 3\na: 1\n${TEXT}`);
      await expect(marks(page)).toHaveText([LOCKED]);
      // The same numbers named by the file as loaded: the same field.
      const loaded = TEXT.indexOf(LOCKED);
      await page.evaluate(([range, text]) => window.lock(range, text), [{ start: loaded, end: loaded + LOCKED.length }, TEXT]);
      await expect(marks(page)).toHaveText([LOCKED]);
      expect(errors).toEqual([]);
    });

    test(`${kind}: applyChanges on the host's older revision is rebased over the typing, the lock follows, Undo keeps it`, async ({ page }) => {
      const errors = await open(page);
      await mount(page, kind);
      await typeAt(page, 0, 'a: 1\n');
      await page.evaluate(() => window.flushEchoes());
      const host = await page.evaluate(() => window.host());
      const start = host.text.indexOf(LOCKED);
      await page.evaluate(([range, revision]) => window.lock(range, revision), [{ start, end: start + LOCKED.length }, host.revision]);
      await typeAt(page, 0, 'top: 0\n');
      const at = host.text.indexOf('80');
      const revision = await page.evaluate(([at, revision]) => window.change([{ from: at, to: at + 2, insert: '8080' }], revision), [at, host.revision]);
      expect(typeof revision).toBe('number');
      const changed = host.text.replace('port: 80', 'port: 8080');
      await expect(input(page)).toHaveValue(`top: 0\n${changed}`);
      await expect(marks(page)).toHaveText([LOCKED]);
      await input(page).focus();
      await page.keyboard.press('Control+Z');
      await expect(input(page)).toHaveValue(changed);
      await page.evaluate(() => window.flushEchoes());
      await expect(input(page)).toHaveValue(changed);
      await expect.poll(() => page.evaluate(() => window.host().text)).toBe(changed);
      await expect(marks(page)).toHaveText([LOCKED]);
      expect(errors).toEqual([]);
    });
  }

  for (const kind of ['editor', 'documents']) {
    // A host whose store has not heard the typing yet (its echo is late)
    // names the text it made its change on: the change lands there, rebased
    // over the typing. A text the editor cannot name throws.
    test(`${kind}: applyChanges named by the host's own text while its echo is late lands on that text`, async ({ page }) => {
      const errors = await open(page);
      await mount(page, kind);
      await typeAt(page, 0, 'a: 1\n');
      const at = TEXT.indexOf('80');
      await page.evaluate(([at, text]) => window.change([{ from: at, to: at + 2, insert: '8080' }], text), [at, TEXT]);
      await expect(input(page)).toHaveValue(`a: 1\n${TEXT.replace('port: 80', 'port: 8080')}`);
      const refused = await page.evaluate(() => {
        try {
          window.change([{ from: 0, to: 0, insert: 'x' }], 'not: a text it knows\n');
          return 'no throw';
        } catch (error) {
          return String(error.message);
        }
      });
      expect(refused).toContain('a text the editor does not know');
      expect(errors).toEqual([]);
    });

    test(`${kind}: diagnostics naming a revision no editor sent are refused as ranges are`, async ({ page }) => {
      const errors = await open(page);
      await mount(page, kind);
      const message = await page.evaluate(() => {
        try {
          window.problems([{ range: { start: 0, end: 4 }, message: 'checked' }], 999999);
          return 'no throw';
        } catch (error) {
          return String(error.message);
        }
      });
      expect(message).toContain('diagnosticsFor names revision 999999, which no editor sent');
      await expect(page.locator('[data-host] .config-editor__diagnostic-marker-host')).toHaveCount(0);
      expect(errors).toEqual([]);
    });

    test(`${kind}: a list out of date is said once per update`, async ({ page }) => {
      const errors = await open(page);
      await mount(page, kind);
      await typeAt(page, 0, 'a: 1\n');
      await page.evaluate(() => window.flushEchoes());
      await page.evaluate(() => window.lock({ start: 0, end: 4 }, 0));
      await page.waitForTimeout(100);
      expect(page.warnings.filter((warning) => warning.includes('out of date'))).toHaveLength(1);
      expect(errors).toEqual([]);
    });
  }

  // A document left out of the list and listed again has no editor until
  // it is shown again: the view's and the document editor's applyChanges
  // both say so, and neither drops the change silently.
  test("applyChanges on a document listed again before it is shown says it has no editor yet, as its editor's does", async ({ page }) => {
    const errors = await open(page);
    const messages = await page.evaluate(async (text) => {
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '300px';
      document.body.prepend(element);
      const files = [{ id: 'a', fileName: 'a.yaml', value: text }, { id: 'b', fileName: 'b.yaml', value: text }];
      const view = new module.ConfigDocuments(element, { documents: files, echoes: true });
      await new Promise((resolve) => setTimeout(resolve, 100));
      view.update({ documents: files.slice(1) });
      view.update({ documents: files });
      await new Promise((resolve) => setTimeout(resolve, 100));
      const tried = (call) => {
        try {
          return `returned ${call()}`;
        } catch (error) {
          return String(error.message);
        }
      };
      const change = [{ from: 0, to: 0, insert: 'x' }];
      return [tried(() => view.applyChanges('a', change)), tried(() => view.editor('a').applyChanges(change))];
    }, TEXT);
    expect(messages).toEqual([
      'applyChanges: the document "a" has no editor yet (it has not been shown since it was listed): list its new value instead',
      'applyChanges: the document "a" has no editor yet (it has not been shown since it was listed): list its new value instead',
    ]);
    expect(errors).toEqual([]);
  });

  test("a document editor's applyChanges is its value at once, as the view's", async ({ page }) => {
    const errors = await open(page);
    const values = await page.evaluate(async (text) => {
      const element = document.createElement('div');
      element.dataset.host = '';
      element.style.height = '300px';
      document.body.prepend(element);
      const view = new module.ConfigDocuments(element, { documents: [{ id: 'm', fileName: 'm.yaml', value: text }], echoes: true });
      await new Promise((resolve) => setTimeout(resolve, 100));
      const editor = view.editor('m');
      editor.applyChanges([{ from: 0, to: 0, insert: 'top: 0\n' }]);
      const first = editor.value;
      view.applyChanges('m', [{ from: 0, to: 0, insert: 'up: 1\n' }]);
      return [first, view.editor('m').value];
    }, TEXT);
    expect(values).toEqual([`top: 0\n${TEXT}`, `up: 1\ntop: 0\n${TEXT}`]);
    expect(errors).toEqual([]);
  });
});
