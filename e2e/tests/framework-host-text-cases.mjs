import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// What every framework component must do with the texts the host's lists
// name, run on each framework's example page (react.html, vue.html,
// svelte.html): `?revisions` keeps `{ text, revision }` as the READMEs show
// (`valueRevision`, `rangesFor`), `?manual` holds each edit's echo until
// the test hands it back (see `js/examples/echo-delay.ts`). A lock stays on
// the text it was given for through late echoes, hide and show, re-mounts
// and resets; late problems land on the text they name; the host's own
// change (`applyChanges`) is rebased over the typing since. Build the
// packages first with `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const url = (name, query) => pathToFileURL(join(js, 'examples', name)).href + query;

const initialText = '# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n';
const locked = 'public_address: tunnel.example.com';
const rangeIn = (text) => ({ start: text.indexOf(locked), end: text.indexOf(locked) + locked.length });

async function open(page, name, query) {
  const errors = [];
  const warnings = [];
  page.on('pageerror', (error) => errors.push(String(error)));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
    if (message.type() === 'warning') warnings.push(message.text());
  });
  await page.goto(url(name, query));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.locator('.config-editor__input').waitFor();
  return { errors, warnings };
}

const input = (page) => page.locator('.config-editor__input');
const saved = (page) => page.locator('#saved');
const lockMarks = (page) => page.locator('.config-editor__locked');
// The page's extra props, merged.
const extra = (page, props) => page.evaluate((props) => window.setExtraProps((old) => ({ ...old, ...props })), props);
const parentState = (page) => page.evaluate(() => window.parentState());
// Types `text` at the top of the document.
async function typeAtTop(page, text) {
  await input(page).evaluate((element) => {
    element.focus();
    element.setSelectionRange(0, 0);
  });
  await page.keyboard.insertText(text);
}
// Hands back the echoes held since `from` (the count held before), in order;
// returns the count held now.
async function releaseFrom(page, from) {
  return page.evaluate((from) => {
    const count = window.heldEchoes();
    window.releaseEchoes([...Array(count).keys()].slice(from));
    return count;
  }, from);
}
// Waits until the parent holds a revision newer than `after`.
async function revisionAfter(page, after) {
  await page.waitForFunction((after) => {
    const revision = window.parentState().revision;
    return revision !== undefined && revision !== null && (after === null || revision > after);
  }, after);
  return parentState(page);
}

/** Registers the host-text component tests for the example page `name`. */
export function hostTextCases(name) {
  test.describe(`${name}: the texts the host's lists name`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    test('a new text of the parent\'s own with the same ranges locks what they name in it', async ({ page }) => {
      const { errors } = await open(page, name, '');
      await extra(page, { readOnlyRanges: [{ start: 0, end: 6 }] });
      await expect(lockMarks(page)).toHaveText(['# Tunn']);
      await page.evaluate(() => window.setParentValue('abcdef: 1\nlisten: 2\n'));
      await expect(input(page)).toHaveValue('abcdef: 1\nlisten: 2\n');
      // As a mount with those ranges marks it: a locked key takes its field.
      await expect(lockMarks(page)).toHaveText(['abcdef', ': 1']);
      expect(errors).toEqual([]);
    });

    for (const how of ['hide and show', 're-mount']) {
      test(`a ${how} while echoes are late keeps the lock on its text, named by the file as loaded and by a revision`, async ({ page }) => {
        const { errors } = await open(page, name, '?manual&revisions');
        const again = async () => {
          if (how === 're-mount') {
            await page.evaluate(() => window.renameFile());
            await expect(page.locator('.config-editor__toolbar')).toContainText('renamed-');
          } else {
            await page.evaluate(() => window.setShown(false));
            await expect(input(page)).toHaveCount(0);
            await page.evaluate(() => window.setShown(true));
            await input(page).waitFor();
          }
        };
        // Worked out once on the file as loaded.
        await extra(page, { readOnlyRanges: [rangeIn(initialText)], rangesFor: initialText });
        await expect(lockMarks(page)).toHaveText([locked]);
        await typeAtTop(page, 'a: 1\n');
        await again();
        await expect(lockMarks(page)).toHaveText([locked]);
        let held = await releaseFrom(page, 0);
        await expect(saved(page)).toHaveText(`a: 1\n${initialText}`);
        await expect(input(page)).toHaveValue(`a: 1\n${initialText}`);
        await expect(lockMarks(page)).toHaveText([locked]);
        // Worked out on the revision the parent holds.
        const state = await revisionAfter(page, null);
        await extra(page, { readOnlyRanges: [rangeIn(state.text)], rangesFor: state.revision });
        await typeAtTop(page, 'b: 2\n');
        await again();
        await expect(lockMarks(page)).toHaveText([locked]);
        held = await releaseFrom(page, held);
        await expect(input(page)).toHaveValue(`b: 2\na: 1\n${initialText}`);
        await expect(lockMarks(page)).toHaveText([locked]);
        await releaseFrom(page, held);
        await expect(saved(page)).toHaveText(`b: 2\na: 1\n${initialText}`);
        expect(errors).toEqual([]);
      });
    }

    // Hidden and shown again as the same component (React's `<Activity>`,
    // Vue's `<KeepAlive>`), a v0.1.0 parent (no `valueRevision`) handing
    // each edit back late: the late answers for the texts the editor sent
    // before it was hidden keep what is typed after. React's component
    // makes a new editor in its element, which takes those texts over (the
    // first answer replaced the typing before); Vue's keeps its editor.
    // Svelte keeps no component across a hide and show (`{#if}` makes a
    // new one, which needs `valueRevision`).
    const sameComponent = { 'react.html': '&activity', 'vue.html': '&keep-alive' }[name];
    test('hidden and shown as the same component while echoes are late, a v0.1.0 parent keeps the typing', async ({ page }) => {
      test.skip(!sameComponent, 'no way to keep a Svelte component across a hide and show');
      const { errors } = await open(page, name, `?manual${sameComponent}`);
      await typeAtTop(page, 'a: 1\n');
      await typeAtTop(page, 'c: 3\n');
      await page.evaluate(() => window.setShown(false));
      await expect(input(page)).toBeHidden();
      await page.evaluate(() => window.setShown(true));
      await expect(input(page)).toBeVisible();
      await expect(input(page)).toHaveValue(`c: 3\na: 1\n${initialText}`);
      await typeAtTop(page, 'b: 2\n');
      // The first text held: the editor before the hide sent it.
      let held = await page.evaluate(() => {
        window.releaseEchoes([0]);
        return window.heldEchoes();
      });
      await page.waitForTimeout(100);
      await expect(input(page)).toHaveValue(`b: 2\nc: 3\na: 1\n${initialText}`);
      held = await releaseFrom(page, 1);
      await page.waitForTimeout(400);
      await releaseFrom(page, held);
      await expect(saved(page)).toHaveText(`b: 2\nc: 3\na: 1\n${initialText}`);
      await expect(input(page)).toHaveValue(`b: 2\nc: 3\na: 1\n${initialText}`);
      expect(errors).toEqual([]);
    });

    test('setValue and the parent\'s own text, with revisions and late echoes: the next typing is kept', async ({ page }) => {
      const { errors } = await open(page, name, '?manual&revisions');
      await page.evaluate(() => window.editorHandle.current.setValue('x: 1\n'));
      await expect(input(page)).toHaveValue('x: 1\n');
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('y');
      let held = await releaseFrom(page, 0);
      await expect(saved(page)).toHaveText('x: 1\ny');
      await expect(input(page)).toHaveValue('x: 1\ny');
      await page.evaluate(() => window.setParentValue('z: 2\n'));
      await expect(input(page)).toHaveValue('z: 2\n');
      await page.keyboard.press('Control+End');
      await page.keyboard.type('w');
      held = await releaseFrom(page, held);
      await page.waitForTimeout(100);
      await releaseFrom(page, held);
      await expect(saved(page)).toHaveText('z: 2\nw');
      await expect(input(page)).toHaveValue('z: 2\nw');
      expect(errors).toEqual([]);
    });

    test('late problems for an older revision land on the text they name', async ({ page }) => {
      const { errors } = await open(page, name, '?revisions');
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('debug: 1');
      const state = await revisionAfter(page, null);
      expect(state.text).toBe(`${initialText}debug: 1`);
      await typeAtTop(page, 'a: 1\nb: 2\n');
      // Line 3 of the revision checked: `public_address`, line 5 now.
      await extra(page, {
        hostDiagnostics: [{ range: { startLine: 3, startColumn: 1, endLine: 3, endColumn: 15 }, severity: 'warning', message: 'checked late', source: 'server' }],
        diagnosticsFor: state.revision,
      });
      await expect(page.locator('.config-editor__diagnostic-marker-host').first()).toHaveAttribute('aria-label', /on line 5: checked late/);
      expect(errors).toEqual([]);
    });

    test('applyChanges on an older revision while echoes are late: rebased, locks follow, Undo keeps it, the parent hears it', async ({ page }) => {
      const { errors } = await open(page, name, '?manual&revisions');
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('debug: 1');
      let held = await releaseFrom(page, 0);
      const state = await revisionAfter(page, null);
      await extra(page, { readOnlyRanges: [rangeIn(state.text)], rangesFor: state.revision });
      await expect(lockMarks(page)).toHaveText([locked]);
      await typeAtTop(page, 'top: 1\n');
      const at = state.text.indexOf('7835');
      const revision = await page.evaluate(
        ([at, revision]) => window.editorHandle.current.applyChanges([{ from: at, to: at + 4, insert: '8080' }], { revision }),
        [at, state.revision],
      );
      expect(typeof revision).toBe('number');
      const changed = state.text.replace('7835', '8080');
      await expect(input(page)).toHaveValue(`top: 1\n${changed}`);
      await expect(lockMarks(page)).toHaveText([locked]);
      await input(page).focus();
      await page.keyboard.press('Control+Z');
      await expect(input(page)).toHaveValue(changed);
      await expect(lockMarks(page)).toHaveText([locked]);
      held = await releaseFrom(page, held);
      await page.waitForTimeout(100);
      await releaseFrom(page, held);
      await expect(saved(page)).toHaveText(changed);
      expect(errors).toEqual([]);
    });

    test('a v0.1.0 parent working its ranges out on each echo keeps the lock on its text', async ({ page }) => {
      const { errors } = await open(page, name, '?manual');
      let held = 0;
      for (const line of ['k1: 1\n', 'k2: 2\n', 'k3: 3\n']) {
        await typeAtTop(page, line);
        // Each echo comes back, then the parent works the ranges out on the
        // text it holds and renders them.
        held = await page.evaluate((from) => {
          const count = window.heldEchoes();
          window.releaseEchoes([...Array(count).keys()].slice(from));
          return count;
        }, held);
        const text = await page.evaluate(() => window.parentState().text);
        await extra(page, { readOnlyRanges: [rangeIn(text)] });
        await expect(lockMarks(page)).toHaveText([locked]);
      }
      await expect(saved(page)).toHaveText(`k3: 3\nk2: 2\nk1: 1\n${initialText}`);
      await expect(lockMarks(page)).toHaveText([locked]);
      const text = await saved(page).textContent();
      await extra(page, { readOnlyRanges: [rangeIn(text)] });
      await page.waitForTimeout(300);
      await expect(lockMarks(page)).toHaveText([locked]);
      expect(errors).toEqual([]);
    });

    test('ranges naming a revision the editor no longer keeps: a warning, the locks unchanged', async ({ page }) => {
      const { errors, warnings } = await open(page, name, '?revisions');
      await input(page).click();
      await page.keyboard.press('Control+End');
      await page.keyboard.type('a');
      const first = await revisionAfter(page, null);
      // More texts sent and handed back since than the editor keeps (64).
      await page.keyboard.type('b'.repeat(70));
      await expect(saved(page)).toHaveText(`${initialText}a${'b'.repeat(70)}`);
      const second = await revisionAfter(page, first.revision);
      await extra(page, { readOnlyRanges: [rangeIn(second.text)], rangesFor: second.revision });
      await expect(lockMarks(page)).toHaveText([locked]);
      await extra(page, { readOnlyRanges: [{ start: 0, end: 6 }], rangesFor: first.revision });
      await expect.poll(() => warnings.some((warning) => warning.includes('out of date'))).toBe(true);
      await expect(lockMarks(page)).toHaveText([locked]);
      expect(errors).toEqual([]);
    });
  });
}
