import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// QA round 29 (editing). The host's byte order mark survives every edit of
// the first line, once, at the start; keys reordered beside members a view
// shows but the host writes elsewhere (XML attributes, YAML merged keys)
// reach the host; an HCL expression moved to another block stays an
// expression; an element copied in an XML view keeps the host's types, and
// a secret copied so waits. Build the packages first with `npm run build`
// in js/.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const BOM = '\u{FEFF}';

/** Mounts a fresh ConfigEditor with `options` on the vanilla page. */
async function open(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const { ConfigEditor } = await import('../config-editor/index.js');
    const parent = document.querySelector('.config-editor').parentElement;
    window.editor.destroy();
    window.saved = null;
    window.problems = [];
    window.editor = new ConfigEditor(parent, {
      ...options,
      onSave: (text) => (window.saved = text),
      onProblems: (problems) => (window.problems = problems),
    });
  }, options);
  await page.locator('.config-editor__input').waitFor();
}

const input = (page) => page.locator('.config-editor__input');
const host = (page) => page.evaluate(() => window.editor.value);

/** Puts the caret at the start of the `nth` match of `needle` on screen. */
async function caretAt(page, needle, nth = 0) {
  await input(page).evaluate((element, [needle, nth]) => {
    let at = -1;
    for (let index = 0; index <= nth; index++) at = element.value.indexOf(needle, at + 1);
    if (at < 0) throw new Error(`no ${needle}`);
    element.focus();
    element.setSelectionRange(at, at);
  }, [needle, nth]);
}

async function press(page, ...keys) {
  for (const key of keys) await page.keyboard.press(key);
}

async function editAs(page, language) {
  await page.evaluate((language) => window.editor.update({ editAs: language }), language);
  await page.waitForTimeout(200);
}

/** The host's value once the typing pause is over. */
async function settled(page) {
  await page.waitForTimeout(700);
  return host(page);
}

test.describe('QA round 29: editing', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  const marked = {
    yaml: '# head\nname: a\nport: 1\n',
    json: '{\n  "name": "a",\n  "port": 1\n}\n',
    toml: '# head\nname = "a"\nport = 1\n',
    hcl: '# head\nname = "a"\nport = 1\n',
    xml: '<?xml version="1.0"?>\n<c>\n  <name>a</name>\n</c>\n',
  };
  const firstLineEdits = {
    'Shift+Down Delete': (page) => press(page, 'Shift+ArrowDown', 'Delete'),
    'Ctrl+X': (page) => press(page, 'Control+x'),
    'Ctrl+A and a paste': async (page) => {
      await press(page, 'Control+a');
      await page.keyboard.insertText('x: 1\n');
    },
    'Ctrl+Shift+K': (page) => press(page, 'Control+Shift+K'),
    'typing at the start': (page) => page.keyboard.type('x'),
    'Ctrl+A Tab': (page) => press(page, 'Control+a', 'Tab'),
    'Tab on the line': (page) => press(page, 'Shift+End', 'Tab'),
    'Ctrl+]': (page) => press(page, 'Control+BracketRight'),
    'Alt+Down': (page) => press(page, 'Alt+ArrowDown'),
    'Shift+Alt+Up': (page) => press(page, 'Shift+Alt+ArrowUp'),
  };
  for (const [language, text] of Object.entries(marked)) {
    for (const crlf of [false, true]) {
      test(`a ${language} host's byte order mark outlasts every edit of its first line${crlf ? ' (CRLF)' : ''}`, async ({ page }) => {
        const value = BOM + (crlf ? text.replaceAll('\n', '\r\n') : text);
        const failures = [];
        for (const [name, edit] of Object.entries(firstLineEdits)) {
          await open(page, { language, value });
          expect(await input(page).inputValue()).not.toContain(BOM);
          await input(page).evaluate((element) => {
            element.focus();
            element.setSelectionRange(0, 0);
          });
          await edit(page);
          const saved = await settled(page);
          const marks = [...saved].filter((character) => character === BOM).length;
          if (!saved.startsWith(BOM) || marks !== 1) failures.push(`${name}: ${JSON.stringify(saved)}`);
        }
        expect(failures).toEqual([]);
      });
    }
  }

  test('a byte order mark outlasts a line moved over the first in another view', async ({ page }) => {
    await open(page, { language: 'yaml', value: `${BOM}name: a\nport: 1\n` });
    await editAs(page, 'toml');
    await caretAt(page, 'port');
    await press(page, 'Alt+ArrowUp');
    const saved = await settled(page);
    expect(saved).toBe(`${BOM}port: 1\nname: a\n`);
  });

  for (const view of ['yaml', 'json', 'toml', 'hcl']) {
    test(`children of an XML element with attributes reordered as ${view} reach the host`, async ({ page }) => {
      const value = '<project xmlns="urn:x" a="1">\n  <groupId>g</groupId>\n  <artifactId>x</artifactId>\n  <version>1</version>\n</project>\n';
      await open(page, { language: 'xml', value });
      await editAs(page, view);
      await caretAt(page, 'groupId');
      await press(page, 'Alt+ArrowDown');
      expect(await settled(page)).toBe(value.replace('  <groupId>g</groupId>\n  <artifactId>x</artifactId>\n', '  <artifactId>x</artifactId>\n  <groupId>g</groupId>\n'));
    });
  }

  for (const view of ['toml', 'hcl', 'xml', 'json']) {
    test(`a YAML map's own keys beside a merge reordered as ${view} reach the host`, async ({ page }) => {
      const value = 'base: &b\n  level: info\nsvc:\n  <<: *b\n  password: p # dev only\n  db: app\n  user: u\n';
      await open(page, { language: 'yaml', value });
      await editAs(page, view);
      await caretAt(page, 'password');
      await press(page, 'Alt+ArrowDown');
      expect(await settled(page)).toBe(value.replace('  password: p # dev only\n  db: app\n', '  db: app\n  password: p # dev only\n'));
    });
  }

  for (const [kind, written] of [
    ['a reference', 'var.z'],
    ['an interpolated string', '"x-${var.y}"'],
    ['a call', 'lower(var.q)'],
  ]) {
    for (const view of ['yaml', 'toml']) {
      test(`${kind} cut and pasted into another block as ${view} stays an expression`, async ({ page }) => {
        const value = `locals {\n  n = 1\n  r = ${written}\n}\n\nother {\n  k = 1\n}\n`;
        await open(page, { language: 'hcl', value });
        await editAs(page, view);
        await caretAt(page, view === 'yaml' ? '  r:' : 'r =');
        await press(page, 'Control+x');
        await caretAt(page, view === 'yaml' ? '  k:' : 'k =');
        await press(page, 'Control+v');
        const saved = await settled(page);
        expect(saved).not.toContain('$${');
        expect(saved).toContain(`other {\n  r = ${written}\n  k = 1\n}`);
      });
    }
  }

  test('a heredoc with an interpolation moved by lines into a nested block as JSON stays a heredoc', async ({ page }) => {
    const value = 'resource "x" "a" {\n  user_data = <<-EOT\n    echo ${var.y}\n  EOT\n  n = 1\n\n  inner {\n    v = 2\n  }\n}\n';
    await open(page, { language: 'hcl', value });
    await editAs(page, 'json');
    await caretAt(page, '"user_data"');
    await press(page, 'Alt+ArrowDown', 'Alt+ArrowDown');
    const shown = await input(page).inputValue();
    expect(shown.indexOf('"user_data"')).toBeGreaterThan(shown.indexOf('"inner"'));
    const saved = await settled(page);
    expect(saved).not.toContain('$${');
    expect(saved).toMatch(/inner \{\n[^}]*user_data = <<-EOT\n\s*echo \$\{var\.y\}\n\s*EOT/);
  });

  for (const language of ['yaml', 'toml', 'hcl']) {
    test(`a ${language} string element copied in an XML view stays a list of strings`, async ({ page }) => {
      const value = language === 'yaml' ? 'deps:\n  anyhow: "1"\n' : '[deps]\nanyhow = "1"\n';
      const text = language === 'hcl' ? 'deps {\n  anyhow = "1"\n}\n' : value;
      await open(page, { language, value: text });
      await editAs(page, 'xml');
      await caretAt(page, '<anyhow>');
      await press(page, 'Shift+Alt+ArrowDown');
      const saved = await settled(page);
      expect(saved).toContain('"1", "1"');
    });
  }

  test('a secret element copied in an XML view is flagged on every copy and the save waits', async ({ page }) => {
    const token = '«secret 3f2a»';
    const schema = JSON.stringify({ type: 'object', properties: { db: { type: 'object', properties: { password: { type: 'string', writeOnly: true } } } } });
    await open(page, { language: 'toml', value: `[db]\nhost = "h"\npassword = "${token}"\nname = "app"\n`, schema });
    await editAs(page, 'xml');
    await caretAt(page, token);
    await press(page, 'Shift+Alt+ArrowDown');
    await page.waitForTimeout(800);
    const problems = await page.evaluate(() => window.problems.map((problem) => problem.message));
    for (const item of ['/db/password/0', '/db/password/1']) {
      expect(problems.some((message) => message.startsWith(`${item}:`))).toBe(true);
    }
    expect(await page.evaluate(() => window.editor.save())).toBe(false);
    expect(await page.evaluate(() => window.saved)).toBe(null);
  });
});
