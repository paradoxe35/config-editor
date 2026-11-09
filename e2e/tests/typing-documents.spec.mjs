import { expect, test } from '@playwright/test';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Typing in a large document among many open ones, as a host keeps them:
// the 2,000-line sample of every language in a `ConfigDocuments` of ten
// tabs with the schema, the host storing each edit and handing its whole
// list back to `update` (`onChange` → `update({ documents })`).
//
// Each keystroke is timed from its `keydown` to just after the next frame
// is drawn (a timer set in that frame's callback): the editor's own work,
// the host's, and drawing. What one keystroke reads is counted elsewhere
// (`keystroke-work.spec.mjs`, `editor/keystroke_work_tests.rs`); this is the
// ceiling against a pass over the whole document coming back into it.
// The browser draws frames at 60 Hz here, so a keystroke shows in the next
// frame (about 17 ms) when its work fits in one, as it does now; the
// ceiling is three frames.
const LANGUAGES = ['yaml', 'json', 'toml', 'hcl', 'xml'];
const TABS = 10;
const KEYSTROKES = 60;
const MEDIAN_CEILING_MS = 50;

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const sample = (language) =>
  readFileSync(join(root, 'crates', 'config-editor-leptos', 'tests', 'fixtures', 'samples', `tunnel.large.${language}`), 'utf8');
const schema = readFileSync(join(root, 'schema.json'), 'utf8');

/** The UTF-16 offset of the end of the first value past the middle of `text`. */
function valueEnd(text) {
  const lines = text.split('\n');
  let offset = 0;
  for (const [index, line] of lines.entries()) {
    if (index >= lines.length / 2) {
      const value = /^(.*[A-Za-z0-9])["<]?/.exec(line)?.[1];
      if (value && /[:=>]/.test(value) && !/^\s*(#|\/\/|<!--)/.test(line)) return offset + value.length;
    }
    offset += line.length + 1;
  }
  throw new Error('no value to type into');
}

const numbers = {};

test.describe('typing among ten large documents', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');
  test.describe.configure({ timeout: 120_000 });

  test.afterAll(() => {
    console.log(`keystroke, median and 90th percentile (ms):\n${JSON.stringify(numbers, null, 1)}`);
  });

  for (const language of LANGUAGES) {
    test(`${language}: a keystroke is drawn within the ceiling`, async ({ page }) => {
      const errors = [];
      page.on('pageerror', (error) => errors.push(String(error)));
      await page.goto(example('vanilla.html'));
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      const text = sample(language);
      await page.evaluate(
        async ({ language, text, schema, tabs }) => {
          const module = await import('../config-editor/index.js');
          const host = document.createElement('div');
          host.id = 'tabs';
          host.style.height = '600px';
          document.body.prepend(host);
          let files = Array.from({ length: tabs }, (_, n) => ({
            id: `d${n}`,
            fileName: `conf${n}.${language}`,
            value: text,
            schema,
          }));
          const view = new module.ConfigDocuments(host, {
            documents: files,
            onChange: (id, value) => {
              files = files.map((file) => (file.id === id ? { ...file, value } : file));
              view.update({ documents: files });
            },
          });
          window.typed = [];
          addEventListener(
            'keydown',
            () => {
              const start = performance.now();
              requestAnimationFrame(() => setTimeout(() => window.typed.push(performance.now() - start), 0));
            },
            true,
          );
        },
        { language, text, schema, tabs: TABS },
      );
      const input = page.locator('#tabs .config-editor__input:visible');
      await input.click();
      const at = valueEnd(text);
      await input.evaluate((area, at) => area.setSelectionRange(at, at), at);
      // Past the first keystrokes' one-time work (the suggestions' first
      // reading of the schema, the validation pause).
      await page.keyboard.type('ab');
      await page.waitForTimeout(800);
      await page.evaluate(() => (window.typed = []));
      for (let index = 0; index < KEYSTROKES; index += 1) {
        await page.keyboard.press(String.fromCharCode(97 + (index % 26)));
        await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => setTimeout(done, 30))));
      }
      const times = (await page.evaluate(() => window.typed)).sort((a, b) => a - b);
      expect(times).toHaveLength(KEYSTROKES);
      const median = times[Math.floor(times.length / 2)];
      const p90 = times[Math.floor(times.length * 0.9)];
      numbers[language] = { median: Math.round(median * 10) / 10, p90: Math.round(p90 * 10) / 10 };
      expect(errors).toEqual([]);
      expect(median, `${language}: median keystroke`).toBeLessThan(MEDIAN_CEILING_MS);
    });
  }
});
