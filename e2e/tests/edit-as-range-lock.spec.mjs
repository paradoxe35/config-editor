import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A host's readOnlyRanges lock in an Edit-as view follows its field (QA 33):
// with a key added above the locked field, the next edit anywhere read the
// range at its first positions in the new host text, so the lock went to
// the field above and the locked value took typing and was saved changed.
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const vanilla = pathToFileURL(join(js, 'examples', 'vanilla.html')).href;

/** Host language, view, host text, the line the new key goes after, the new key, a free value's end, the locked value's end. */
const cases = [
  ['yaml', 'json', 'name: a\nport: 80\nhost: h\n', '"name": "a",', '"extra": 1,', '"name": "a', '"host": "h'],
  ['json', 'yaml', '{\n  "name": "a",\n  "port": 80,\n  "host": "h"\n}\n', 'name: a', 'extra: 1', 'name: a', 'host: h'],
  ['toml', 'yaml', 'name = "a"\nport = 80\nhost = "h"\n', 'name: a', 'extra: 1', 'name: a', 'host: h'],
  ['hcl', 'json', 'name = "a"\nport = 80\nhost = "h"\n', '"name": "a",', '"extra": 1,', '"name": "a', '"host": "h'],
  ['xml', 'yaml', '<c>\n  <name>a</name>\n  <port>80</port>\n  <host>h</host>\n</c>\n', 'name: a', 'extra: b', 'name: a', 'host: h'],
];

const input = (page) => page.locator('#locked .config-editor__input');

async function caretAfter(page, marker) {
  await input(page).evaluate((area, marker) => {
    const at = area.value.indexOf(marker);
    if (at < 0) throw new Error(`not found: ${marker}`);
    area.focus();
    area.setSelectionRange(at + marker.length, at + marker.length);
  }, marker);
}

test.describe('Edit-as range lock', () => {
  test.skip(!built, 'run `npm run build` in js/ first');

  for (const [host, view, text, after, added, free, locked] of cases) {
    test(`${host} host edited as ${view}: the range keeps locking its field after a key added above and a second edit`, async ({ page }) => {
      await page.goto(vanilla);
      await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
      await page.evaluate(
        async ({ host, view, text }) => {
          const { ConfigEditor } = await import(new URL('../config-editor/index.js', location.href).href);
          const { rangeOf } = await import(new URL('../config-editor/tools.js', location.href).href);
          const element = document.createElement('div');
          element.id = 'locked';
          element.style.height = '400px';
          document.body.prepend(element);
          const range = rangeOf(host, text, host === 'xml' ? '/c/host' : '/host');
          window.locked = new ConfigEditor(element, { value: text, language: host, readOnlyRanges: [range], editAs: view });
        },
        { host, view, text },
      );
      await caretAfter(page, after);
      await page.keyboard.press('Enter');
      await page.keyboard.type(added);
      await page.waitForTimeout(800);
      // The second edit: a key typed at a free value and taken back.
      await caretAfter(page, free);
      await page.keyboard.type('9');
      await page.keyboard.press('Backspace');
      await page.waitForTimeout(800);
      await caretAfter(page, locked);
      await page.keyboard.type('X');
      await caretAfter(page, free);
      await page.keyboard.type('Z');
      await page.waitForTimeout(800);
      const saved = await page.evaluate(() => {
        window.locked.save();
        return window.locked.value;
      });
      expect(saved).not.toContain('hX');
      expect(saved).toContain('aZ');
      expect(saved).toMatch(/extra/);
    });
  }
});
