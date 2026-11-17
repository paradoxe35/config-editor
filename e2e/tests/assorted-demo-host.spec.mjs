import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { editor, setupEditor } from './helpers.mjs';

// Assorted gaps: each replayed in the demo with real
// keys, failing before its fix.

for (const query of ['?language=yaml', '?documents']) {
  test.describe(`the demo page ${query}`, () => {
    setupEditor(test, query);

    test('takes the theme variables without being styled as an editor', async ({ page }) => {
      const wrapper = page.locator('.demo').first();
      await expect(wrapper).not.toHaveClass(/(^|\s)config-editor(\s|$)/);
      await expect(wrapper).toHaveClass(/config-editor-theme/);
      const variable = (locator) =>
        locator.evaluate((element) => getComputedStyle(element).getPropertyValue('--config-editor-bg').trim());
      const editorRoot = page.locator('.config-editor:has(.config-editor__input)').first();
      expect(await variable(wrapper)).toBe(await variable(editorRoot));
      await page.locator('.demo__theme-select').selectOption('nord');
      await expect(wrapper).toHaveAttribute('data-color-theme', 'nord');
      await expect.poll(() => variable(wrapper)).toBe('#2e3440');
      expect(await variable(editorRoot)).toBe('#2e3440');
    });
  });
}

test.describe('an Edit-as switch that waits', () => {
  setupEditor(test, '?language=xml');

  test('the picker shows the language on screen and the name says it waits', async ({ page }) => {
    const e = editor(page);
    await e.paste('<r>\n  <mixed>text <b>bold</b> tail</mixed>\n</r>\n');
    await page.locator('.demo__edit-select').selectOption('yaml');
    const marker = page.locator('.config-editor__diagnostic-marker[data-message*="Cannot switch to YAML yet"]');
    await expect(marker).toHaveCount(1);
    expect(await e.value()).toContain('<mixed>');
    await expect(page.locator('.demo__edit-select')).toHaveValue('');
    await expect(page.locator('.config-editor__file')).toContainText('waiting to edit as YAML');
    await expect(page.locator('.config-editor__file')).not.toContainText('editing as');
    // Once the document converts, the switch completes and both follow.
    await e.paste('<r>\n  <mixed>text</mixed>\n</r>\n');
    await expect.poll(() => e.value()).toContain('mixed: text');
    await expect(page.locator('.demo__edit-select')).toHaveValue('yaml');
    await expect(page.locator('.config-editor__file')).toContainText('editing as YAML');
  });
});

test.describe('the page sample', () => {
  test('is named as what it is in the Sample picker', async ({ page, baseURL }) => {
    await page.addInitScript(() => {
      window.configEditorSample = 'a: 1\nb: 2\nc: 3\n';
    });
    await page.goto(`${baseURL}?language=yaml&sample=page`);
    await page.locator('.config-editor__input').waitFor();
    const picker = page.locator('.demo__sample-select');
    await expect(picker.locator('option:checked')).toHaveText(/Page sample \(3 lines\)/);
  });
});

// The npm package's example pages (built by `npm run build` in js/).
const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.describe('onSchemaWarning', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  test('arrives after new ConfigEditor returns, never inside it', async ({ page }) => {
    await page.goto(example('vanilla.html'));
    await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
    const result = await page.evaluate(async () => {
      const { ConfigEditor } = await import('../config-editor/index.js');
      const host = document.createElement('div');
      document.body.append(host);
      const schema = JSON.stringify({ type: 'object', patternProperties: { '^(?!x-)[a-z]+$': { type: 'string' } } });
      const heard = [];
      let editor;
      editor = new ConfigEditor(host, {
        language: 'yaml',
        value: 'a: b\n',
        schema,
        onSchemaWarning: (message) => heard.push({ message, instance: editor !== undefined }),
      });
      const during = heard.length;
      await new Promise((resolve) => setTimeout(resolve, 50));
      editor.destroy();
      return { during, heard };
    });
    expect(result.during).toBe(0);
    expect(result.heard).toHaveLength(1);
    expect(result.heard[0].instance).toBe(true);
    expect(result.heard[0].message).toContain('patternProperties');
  });
});

// An entry typed at a read-only field is judged by the locked text alone:
// inside a locked block every key of it is refused (only whitespace may
// stay), past the block's end or as a second copy of a locked key it is
// taken whole; never half of it.
async function caretAt(e, needle, offset) {
  await e.input.evaluate((element, [needle, offset]) => {
    const at = needle === null ? element.value.length : element.value.indexOf(needle) + offset;
    element.focus();
    element.setSelectionRange(at, at);
  }, [needle, offset]);
}

const flat = (text) => text.replace(/\s+/g, '');

for (const [language, place, opened, typed, inside] of [
  ['toml', 'burst = 20', true, 'extra = 5', false],
  ['hcl', 'burst = 20', true, 'extra = 5', true],
  ['yaml', 'burst: 20', true, 'extra: 5', false],
  ['xml', null, true, '<id>other</id>', false],
  ['hcl', null, true, 'id = "other"', false],
]) {
  test.describe(`${language}: an entry typed at a read-only field`, () => {
    setupEditor(test, `?schema=protected&language=${language}`);

    test(`${typed} is taken or refused whole`, async ({ page }) => {
      const e = editor(page);
      const before = await e.value();
      await caretAt(e, place, place === null ? 0 : place.length);
      if (opened) await page.keyboard.press('Enter');
      await page.keyboard.type(typed, { delay: 20 });
      const now = await e.value();
      if (inside) {
        await expect(page.locator('.config-editor__protect-tip--refused')).toBeVisible();
        expect(flat(now)).toBe(flat(before));
      } else {
        expect(flat(now)).toContain(flat(typed));
        expect(flat(now).length).toBe(flat(before).length + flat(typed).length);
      }
    });
  });
}
