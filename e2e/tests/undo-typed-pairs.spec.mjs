import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// A pair the editor types itself (a quote or bracket opened, a selection
// wrapped in one, an empty pair deleted) is one undo step with the typing
// around it: one undo never leaves half-typed text such as `a: ""one""`,
// which the next save would have sent (round 8's `D8.editing-2`).

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

async function mount(page, language, value) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', {
    timeout: 20000,
  });
  await page.evaluate(
    async ({ language, value }) => {
      const module = await import('../config-editor/index.js');
      window.editor?.destroy();
      const host = document.createElement('div');
      host.id = 'qa';
      document.body.prepend(host);
      window.qa = new module.ConfigEditor(host, { language, value });
    },
    { language, value },
  );
  await input(page).waitFor();
}

async function select(page, marker) {
  await input(page).evaluate((element, marker) => {
    const at = element.value.indexOf(marker);
    element.focus();
    element.setSelectionRange(at, at + marker.length);
  }, marker);
}

const value = (page) => page.evaluate(() => window.qa.value);

// (language, text, selected, typed over it)
const RETYPED = [
  ['yaml', 'a: "one"\n', '"one"', '"two"'],
  ['yaml', 'a: [1, 2]\n', '[1, 2]', '[3]'],
  ['json', '{"a": "one"}\n', '"one"', '"two"'],
  ['json', '{"a": [1, 2]}\n', '[1, 2]', '[3]'],
  ['toml', 'a = "one"\n', '"one"', '"two"'],
  ['toml', 'a = [1, 2]\n', '[1, 2]', '[3]'],
  ['hcl', 'a = "one"\n', '"one"', '"two"'],
  ['hcl', 'a = [1, 2]\n', '[1, 2]', '[3]'],
  ['xml', '<r k="v"/>\n', '"v"', '"w"'],
];

test.describe('one undo after typing over a selection', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [language, text, selected, typed] of RETYPED) {
    test(`${language}: ${selected} retyped as ${typed} comes back whole`, async ({ page }) => {
      await mount(page, language, text);
      await select(page, selected);
      await page.keyboard.type(typed);
      const after = await value(page);
      expect(after).toBe(text.replace(selected, typed));
      await page.keyboard.press('Control+z');
      await expect.poll(() => value(page)).toBe(text);
      // And redo gives the retyped text back, whole.
      await page.keyboard.press('Control+y');
      await expect.poll(() => value(page)).toBe(after);
    });
  }

  test('a wrap then a pause then typing is still one step', async ({ page }) => {
    await mount(page, 'yaml', 'a: "one"\n');
    await select(page, '"one"');
    await page.keyboard.type('"');
    await page.waitForTimeout(1200);
    await page.keyboard.type('two"');
    expect(await value(page)).toBe('a: "two"\n');
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe('a: "one"\n');
  });

  test('a typed pair and what is typed in it are one step', async ({ page }) => {
    await mount(page, 'yaml', 'a: 1\nb: \n');
    await input(page).evaluate((element) => {
      element.focus();
      element.setSelectionRange(8, 8);
    });
    await page.keyboard.type('"x"');
    expect(await value(page)).toBe('a: 1\nb: "x"\n');
    await page.keyboard.press('Control+z');
    await expect.poll(() => value(page)).toBe('a: 1\nb: \n');
  });
});
