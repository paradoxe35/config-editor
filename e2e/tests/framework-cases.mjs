import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// What every framework component must do, run on each framework's example
// page (react.html, vue.html, svelte.html: the same controls, ids, query
// flags and `window` hooks): a parent that applies the value late or
// slowly never rolls the document back, a re-mount keeps the document, the
// caret and focus, a failed mount comes back once its prop is fixed, the
// WebAssembly loads from the app's URL however late, and the instance API
// (React's ref, Vue's template ref, Svelte's exports; `window.editorHandle`
// on each page) resets the document. Build the packages first with
// `npm run build` in js/.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const page = (name) => pathToFileURL(join(js, 'examples', name)).href;

async function open(browserPage, name) {
  const errors = [];
  browserPage.on('pageerror', (error) => errors.push(String(error)));
  browserPage.on('console', (message) => message.type() === 'error' && errors.push(message.text()));
  await browserPage.goto(page(name));
  await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await browserPage.locator('.config-editor__input').waitFor();
  return errors;
}

const input = (browserPage) => browserPage.locator('.config-editor__input');
const saved = (browserPage) => browserPage.locator('#saved');

/** Registers the shared component tests for the example page `name`. */
export function frameworkCases(name) {
  test.describe(`${name}: the component`, () => {
    test.skip(!built, 'js packages not built: run `npm run build` in js/');

    // props
    test('a colors prop overrides the theme', async ({ page: browserPage }) => {
      await open(browserPage, name);
      const style = await browserPage.locator('.config-editor').first().getAttribute('style');
      expect(style).toContain('--config-editor-key: #ffcc66');
    });

    test('readOnlyReason says why an edit is refused; onSyncChange tells whether the text is written back', async ({ page: browserPage }) => {
      await open(browserPage, name);
      const extra = (props) => browserPage.evaluate((props) => window.setExtraProps(props), props);
      await extra({ readOnly: true, readOnlyReason: 'Managed by the cluster operator' });
      await input(browserPage).click();
      await browserPage.keyboard.type('x');
      await expect(browserPage.locator('.config-editor__read-only-notice')).toHaveText(
        'Cannot edit in a read-only editor: Managed by the cluster operator',
      );
      await extra({ readOnly: true });
      await input(browserPage).click();
      await browserPage.keyboard.type('x');
      await expect(browserPage.locator('.config-editor__read-only-notice')).toHaveText('Cannot edit in a read-only editor');

      await browserPage.evaluate(() => {
        window.syncs = [];
        window.setExtraProps({ editAs: 'toml', onSyncChange: (inSync) => window.syncs.push(inSync) });
      });
      await expect(browserPage.locator('.config-editor__frame')).toHaveAttribute('aria-label', 'TOML editor');
      await input(browserPage).focus();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('broken = ');
      await expect.poll(() => browserPage.evaluate(() => window.syncs)).toEqual([false]);
      await browserPage.keyboard.type('x');
      await browserPage.keyboard.press('Backspace');
      await browserPage.keyboard.type('1');
      await expect.poll(() => browserPage.evaluate(() => window.syncs)).toEqual([false, true]);
      await browserPage.waitForTimeout(700);
      expect(await browserPage.evaluate(() => window.syncs)).toEqual([false, true]);
      await expect(saved(browserPage)).toContainText('broken: 1');
    });

    test('a setting prop taken away returns to its default, a text taken out of strings too', async ({ page: browserPage }) => {
      // The binding hands the editor's `update` only what changed: `null`
      // for a prop no longer passed, each text of `strings` on its own.
      await open(browserPage, name);
      const mark = () => input(browserPage).evaluate((element) => (element.dataset.kept = 'yes'));
      await mark();
      const toolbar = browserPage.locator('.config-editor__toolbar');
      const footer = browserPage.locator('.config-editor__status');
      const extra = (props) => browserPage.evaluate((props) => window.setExtraProps(props), props);
      await extra({ showFooter: false, strings: { linesOther: '{count} Zeilen', schemaConnected: 'Schema verbunden' } });
      await expect(toolbar).toContainText('Schema verbunden');
      await expect(footer).toHaveCount(0);
      await extra({ strings: { linesOther: '{count} Zeilen' } });
      await expect(toolbar).toContainText('Schema connected');
      await expect(footer).toContainText('Zeilen');
      await extra({});
      await expect(footer).toContainText(/\d+ lines/);
      // Live, not a new editor.
      expect(await input(browserPage).evaluate((element) => element.dataset.kept)).toBe('yes');
    });

    test('onEditAsChange reports the switch', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.locator('#edit-as').selectOption('toml');
      await expect(browserPage.locator('#editing')).toHaveText('toml');
    });

    // parent updates and re-mounts
    test('the parent can revert to an earlier value', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('a');
      await browserPage.keyboard.type('b');
      await expect(saved(browserPage)).toContainText('ab');
      await browserPage.locator('#revert').click();
      await expect(input(browserPage)).toHaveValue(/tunnel\.example\.com\na$/);
    });

    for (const mode of ['', '?uncontrolled']) {
      test(`a re-mount keeps the edited document ${mode || '(controlled)'}`, async ({ page: browserPage }) => {
        await browserPage.goto(page(name) + mode);
        await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await input(browserPage).waitFor();
        await input(browserPage).click();
        await browserPage.keyboard.press('Control+End');
        await browserPage.keyboard.type('limits: {}');
        await browserPage.locator('#rename').click();
        await expect(browserPage.locator('.config-editor__toolbar')).toContainText('renamed-tunnel.yaml');
        await expect(input(browserPage)).toHaveValue(/limits: \{\}/);
        // The diff gutter still compares against the original text.
        await expect(browserPage.locator('.config-editor__diff-marker-added').first()).toBeVisible();
      });
    }

    // late parents and failed mounts
    test('a reset to a value a debounced parent skipped is its echo, the parent kept in step; setValue applies it', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?late');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('x');
      await browserPage.keyboard.press('Backspace');
      await browserPage.keyboard.type('x');
      // The parent applies only the latest value, late.
      await expect(saved(browserPage)).toHaveText(before + 'x');
      // The initial text was sent and never came back: the parent setting
      // it is taken for its echo (see the README), and hears the document.
      await browserPage.locator('#reset').click();
      await browserPage.waitForTimeout(700);
      await expect(input(browserPage)).toHaveValue(before + 'x');
      await expect(saved(browserPage)).toHaveText(before + 'x');
      // setValue always applies, and the parent follows.
      await browserPage.locator('#force-reset').click();
      await expect(input(browserPage)).toHaveValue(before);
      await browserPage.waitForTimeout(700);
      await expect(input(browserPage)).toHaveValue(before);
      await expect(saved(browserPage)).toHaveText(before);
    });

    test('a parent applying each value 0-200 ms late, in any order, loses no keystroke', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?random');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      let typed = before;
      for (const round of ['abcdefghij', 'klmnopqrst']) {
        await browserPage.keyboard.type(round, { delay: 25 });
        await browserPage.keyboard.press('Backspace');
        await browserPage.keyboard.press('Backspace');
        await browserPage.keyboard.type('XY', { delay: 25 });
        typed += round.slice(0, -2) + 'XY';
        await browserPage.waitForTimeout(600);
        await expect(input(browserPage)).toHaveValue(typed);
        await expect(saved(browserPage)).toHaveText(typed);
      }
    });

    test('backspacing a word against a slow parent never rolls the document back', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?slow');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.press('Enter');
      const seen = [];
      await browserPage.keyboard.type('hello world, ok', { delay: 100 });
      for (let index = 0; index < 15; index += 1) {
        await browserPage.keyboard.press('Backspace');
        await browserPage.waitForTimeout(100);
      }
      for (const key of 'done') {
        await browserPage.keyboard.type(key);
        seen.push(await input(browserPage).inputValue());
        await browserPage.waitForTimeout(100);
      }
      // While the echoes come back, keep checking the document never jumps.
      for (let index = 0; index < 30; index += 1) {
        seen.push(await input(browserPage).inputValue());
        await browserPage.waitForTimeout(80);
      }
      const typed = before + '\ndone';
      expect(seen.slice(4).every((value) => value === typed)).toBe(true);
      await expect(saved(browserPage)).toHaveText(typed, { timeout: 5000 });
    });

    test('setValue after the parent has caught up brings the parent along', async ({ page: browserPage }) => {
      await browserPage.goto(page(name));
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('ab');
      await expect(saved(browserPage)).toContainText('ab');
      await browserPage.evaluate(() => window.editorHandle.current.setValue('listen: 1\n'));
      await expect(input(browserPage)).toHaveValue('listen: 1\n');
      await expect(saved(browserPage)).toHaveText('listen: 1\n');
    });

    test('setValue against a slow parent sticks, and the parent ends on it too', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?slow');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('q');
      await browserPage.keyboard.press('Backspace');
      await browserPage.keyboard.type('w');
      await browserPage.evaluate((text) => window.editorHandle.current.setValue(text), before);
      await browserPage.waitForTimeout(2500);
      await expect(input(browserPage)).toHaveValue(before);
      await expect(saved(browserPage)).toHaveText(before, { timeout: 5000 });
    });

    test('setValue resets to a text the user just passed through', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?late');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('k2: 2');
      await expect(saved(browserPage)).toContainText('k2: 2');
      for (let count = 0; count < 5; count += 1) await browserPage.keyboard.press('Backspace');
      await browserPage.keyboard.type('k3: 3');
      // Before the late parent applies it, discard back to the initial text.
      await browserPage.locator('#force-reset').click();
      await expect(input(browserPage)).toHaveValue(before);
      await browserPage.waitForTimeout(600);
      await expect(input(browserPage)).toHaveValue(before);
      await expect(saved(browserPage)).not.toContainText('k3');
    });

    test('more than 50 values in flight to a slow parent never roll the document back', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?slow');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.press('Enter');
      await browserPage.keyboard.type('# ' + 'a'.repeat(80), { delay: 5 });
      const typed = before + '\n# ' + 'a'.repeat(80);
      await expect(saved(browserPage)).toHaveText(typed, { timeout: 10000 });
      expect(await input(browserPage).inputValue()).toBe(typed);
    });

    test('fixing a typo against a slow parent never rolls the document back', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?slow');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.press('Enter');
      await browserPage.keyboard.type('# ab', { delay: 80 });
      await browserPage.keyboard.press('Backspace');
      await browserPage.keyboard.type('bc', { delay: 80 });
      // Keep typing while the echoes come back.
      for (let index = 0; index < 20; index += 1) {
        await browserPage.keyboard.type('d');
        await browserPage.waitForTimeout(100);
      }
      const typed = before + '\n# abc' + 'd'.repeat(20);
      expect(await input(browserPage).inputValue()).toBe(typed);
      await expect(saved(browserPage)).toHaveText(typed, { timeout: 5000 });
      expect(await input(browserPage).inputValue()).toBe(typed);
    });

    test('a value the user only passed through, set by a debounced parent, is its echo; setValue applies it', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?late');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const before = await input(browserPage).inputValue();
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('x');
      await browserPage.keyboard.press('Backspace');
      // The late parent applies the unchanged text: no render, nothing echoed.
      await browserPage.waitForTimeout(600);
      // The parent sets the passed-through text, which never came back: an
      // echo, and the parent hears the document again.
      await browserPage.evaluate((text) => window.setParentValue(text), before + 'x');
      await browserPage.waitForTimeout(700);
      await expect(input(browserPage)).toHaveValue(before);
      await expect(saved(browserPage)).toHaveText(before);
      await browserPage.evaluate((text) => window.editorHandle.current.setValue(text), before + 'x');
      await expect(input(browserPage)).toHaveValue(before + 'x');
      await expect(saved(browserPage)).toHaveText(before + 'x');
    });

    test('fixing a prop that failed a re-mount brings the editor back', async ({ page: browserPage }) => {
      const errors = await open(browserPage, name);
      await browserPage.locator('#bad-theme').click();
      await browserPage.locator('#rename').click();
      await expect(input(browserPage)).toHaveCount(0);
      await browserPage.locator('#theme').selectOption('nord');
      await expect(input(browserPage)).toHaveCount(1);
      await expect(browserPage.locator('.config-editor').first()).toHaveAttribute('data-color-theme', 'nord');
      // The failures went to the console (the example has no onError).
      expect(errors.some((error) => error.includes('not-a-theme') || error.includes('theme'))).toBe(true);
    });

    // round 5
    test('a re-mount while the parent is late keeps the newest document', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?late');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('debug: true');
      // Re-mount before the late parent catches up, then keep typing.
      await browserPage.locator('#rename').click();
      await expect(browserPage.locator('.config-editor__toolbar')).toContainText('renamed-');
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('\nfoo: 1');
      await browserPage.waitForTimeout(1500);
      await expect(input(browserPage)).toHaveValue(/debug: true\nfoo: 1$/);
      await expect(saved(browserPage)).toContainText('debug: true\nfoo: 1');
    });

    // the ref while a re-mount has failed
    test('the instance value is the document, not the initial value', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?uncontrolled');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('debug: true');
      await browserPage.locator('#bad-theme').click();
      await browserPage.locator('#rename').click();
      await expect(input(browserPage)).toHaveCount(0);
      const value = await browserPage.evaluate(() => window.editorHandle.current.value);
      expect(value).toContain('debug: true');
    });

    // round 11
    test('setValue before the editor mounts applies and reaches the parent', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?reset-early');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await expect(input(browserPage)).toHaveValue('from-effect: 1\n');
      await expect(saved(browserPage)).toHaveText('from-effect: 1\n');
      expect(await browserPage.evaluate(() => window.editorHandle.current.value)).toBe('from-effect: 1\n');
    });

    test('a slow parent replacing the value while echoes are in flight loses no keystroke', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?slow');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('abc', { delay: 80 });
      await browserPage.locator('#replace').click();
      await expect(input(browserPage)).toHaveValue('listen: 9000\n');
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('q');
      await browserPage.waitForTimeout(900);
      await browserPage.keyboard.type('rstuvw', { delay: 100 });
      await browserPage.waitForTimeout(3500);
      await expect(input(browserPage)).toHaveValue('listen: 9000\nqrstuvw');
      await expect(saved(browserPage)).toHaveText('listen: 9000\nqrstuvw', { timeout: 5000 });
    });

    // round 12
    test('a reset made before mount survives a late parent', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?reset-early&late');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await expect(input(browserPage)).toHaveValue('from-effect: 1\n');
      await browserPage.waitForTimeout(1500);
      await expect(input(browserPage)).toHaveValue('from-effect: 1\n');
      await expect(saved(browserPage)).toHaveText('from-effect: 1\n');
    });

    test('the editor mounts when the app loads the wasm after the component’s own load failed', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?late-load');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await expect(input(browserPage)).toHaveValue(/listen: 7835/, { timeout: 10000 });
    });

    // round 13
    for (const [query, delay] of [['', 250], ['?late', 500], ['?late', 650]]) {
      test(`${query || '(controlled)'}: typing while editing as JSON every ${delay} ms keeps every letter in order`, async ({ page: browserPage }) => {
        await browserPage.goto(page(name) + query);
        await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
        await browserPage.locator('#edit-as').selectOption('json');
        await expect(input(browserPage)).toHaveValue(/"public_address"/);
        const at = (await input(browserPage).inputValue()).indexOf('tunnel.example.com') + 'tunnel.example.com'.length;
        await input(browserPage).evaluate((element, offset) => {
          element.focus();
          element.setSelectionRange(offset, offset);
        }, at);
        await browserPage.keyboard.type('abcdefgh', { delay });
        await browserPage.waitForTimeout(2000);
        await expect(input(browserPage)).toHaveValue(/tunnel\.example\.comabcdefgh"/);
        await expect(saved(browserPage)).toContainText('tunnel.example.comabcdefgh');
      });
    }

    test('a parent value set while an Edit-as edit waits is applied', async ({ page: browserPage }) => {
      await browserPage.goto(page(name));
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await browserPage.locator('#edit-as').selectOption('json');
      await expect(input(browserPage)).toHaveValue(/"listen"/);
      const at = (await input(browserPage).inputValue()).indexOf('7835') + 4;
      await input(browserPage).evaluate((element, offset) => {
        element.focus();
        element.setSelectionRange(offset, offset);
      }, at);
      await browserPage.keyboard.type('9');
      await browserPage.locator('#replace').click();
      await browserPage.waitForTimeout(1500);
      await expect(saved(browserPage)).toHaveText('listen: 9000\n');
      await expect(input(browserPage)).toHaveValue(/9000/);
      await expect(input(browserPage)).not.toHaveValue(/78359/);
    });

    // round 14
    test('a re-mount prop changed while typing keeps focus, caret and keystrokes', async ({ page: browserPage }) => {
      await browserPage.goto(page(name));
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('xy');
      await browserPage.evaluate(() => window.renameFile());
      await browserPage.waitForTimeout(300);
      await browserPage.keyboard.type('zw');
      await expect(input(browserPage)).toHaveValue(/xyzw$/);
      expect(await browserPage.evaluate(() => document.activeElement?.tagName)).toBe('TEXTAREA');
      await expect(saved(browserPage)).toContainText('xyzw');
    });

    test('a wrong wasm URL followed by the right one while the first loads still mounts', async ({ page: browserPage }) => {
      await browserPage.goto(page(name) + '?double-load');
      await expect(browserPage.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await expect(input(browserPage)).toHaveValue(/listen: 7835/, { timeout: 10000 });
    });

    test("onProblems hears the editor's own problems when they change, and only then", async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.evaluate(() => {
        window.heard = [];
        window.setExtraProps({
          onProblems: (problems) => window.heard.push(problems.map(({ origin, severity, message }) => ({ origin, severity, message }))),
        });
      });
      const heard = () => browserPage.evaluate(() => window.heard);
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.press('Enter');
      // `[` closes itself: `[]` alone in a mapping is no YAML.
      await browserPage.keyboard.type('[');
      await expect.poll(async () => (await heard()).length).toBe(1);
      const [broken] = await heard();
      expect(broken.some((problem) => problem.origin === 'editor' && problem.severity === 'error')).toBe(true);
      await browserPage.keyboard.press('Backspace');
      await expect.poll(async () => (await heard()).length).toBe(2);
      const fixed = (await heard())[1];
      expect(fixed.length).toBeLessThan(broken.length);
      // A comment at the end changes no problem: nothing more is heard.
      await browserPage.keyboard.type('# note');
      await browserPage.waitForTimeout(1200);
      expect((await heard()).length).toBe(2);
    });

    test('unmounted right after an edit, it calls nothing more', async ({ page: browserPage }) => {
      await open(browserPage, name);
      await browserPage.evaluate(() => {
        window.after = [];
        // A callback arriving once the editor has left the page.
        const late = (name) => () => !document.querySelector('.config-editor__input') && window.after.push(name);
        window.setExtraProps({
          onChangeSummary: late('onChangeSummary'),
          onDirtyChange: late('onDirtyChange'),
          onPathChange: late('onPathChange'),
        });
      });
      await input(browserPage).click();
      await browserPage.keyboard.press('Control+End');
      await browserPage.keyboard.type('xy');
      await browserPage.evaluate(() => window.setShown(false));
      await expect(input(browserPage)).toHaveCount(0);
      await browserPage.waitForTimeout(800);
      expect(await browserPage.evaluate(() => window.after)).toEqual([]);
    });
  });
}
