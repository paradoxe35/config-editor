import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Entries typed a key at a time beside a protected field, through the real
// keys: a new HCL block of a protected block's type with other labels is
// taken with auto-closing off (the line was wiped when `{` was
// typed); a real second copy of the block, and a second copy of a
// read-only JSON key typed with auto-closing on or off (`"password" "x",`
// was left), are taken whole too, the locked value kept
// and the save waiting while the copy is there.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const js = join(root, 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;
const input = (page) => page.locator('#qa .config-editor__input');

async function mount(page, options) {
  await page.goto(example('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (options) => {
    const module = await import('../config-editor/index.js');
    window.editor?.destroy();
    const host = document.createElement('div');
    host.id = 'qa';
    document.body.prepend(host);
    window.qa = new module.ConfigEditor(host, { ...options, onSave: () => {} });
  }, options);
  await input(page).waitFor();
}

async function caretAt(page, at) {
  await input(page).evaluate((element, at) => {
    const offset = at < 0 ? element.value.length : at;
    element.focus();
    element.setSelectionRange(offset, offset);
  }, at);
}

// Types `lines` one key at a time, Enter between them; the indent Enter
// writes is kept, so each line is typed without its own.
async function typeLines(page, lines) {
  for (const [index, line] of lines.entries()) {
    if (index) await page.keyboard.press('Enter');
    await page.keyboard.type(line.trimStart(), { delay: 10 });
  }
  await page.waitForTimeout(300);
}

const value = (input) => input.inputValue();
const flat = (text) => text.replace(/\s+/g, '');

const AMI = 'resource "aws_instance" "web" {\n  ami = "ami-123"\n  instance_type = "t3.micro"\n}\n';
const SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    resource: {
      type: 'object',
      additionalProperties: {
        type: 'object',
        additionalProperties: { type: 'object', properties: { ami: { type: 'string', readOnly: true } } },
      },
    },
  },
});
const LOCKS = {
  readOnlyPaths: { readOnlyPaths: ['/resource/aws_instance/web/ami'] },
  readOnlyRanges: { readOnlyRanges: [{ startLine: 2, startColumn: 1, endLine: 2, endColumn: 18 }] },
  schema: { schema: SCHEMA },
};

test.describe('entries typed beside a protected field', () => {
  test.skip(!built, 'js packages not built: run `npm run build` in js/');

  for (const [name, lock] of Object.entries(LOCKS)) {
    test(`HCL, ${name}, auto-closing off: a block with other labels is taken, a second copy is not saved`, async ({ page }) => {
      for (const labels of ['"aws_s3_bucket" "logs"', '"aws_instance" "db"', '"aws_instance" "web2"']) {
        await mount(page, { language: 'hcl', value: AMI, autoClosing: false, ...lock });
        await caretAt(page, -1);
        const block = [`resource ${labels} {`, '  bucket = "x"', '}'];
        await typeLines(page, block);
        expect(flat(await value(input(page))), labels).toContain(flat(block.join('\n')));
      }
      await mount(page, { language: 'hcl', value: AMI, autoClosing: false, ...lock });
      await caretAt(page, -1);
      await typeLines(page, ['resource "aws_instance" "web" {', '  ami = "evil"', '}']);
      // Taken whole; the locked value is kept, and the save waits.
      expect(flat(await value(input(page)))).toContain(flat('resource "aws_instance" "web" {\n  ami = "evil"\n}'));
      expect(await value(input(page))).toContain('ami = "ami-123"');
      expect(await page.evaluate(() => window.qa.save())).toBe(false);
    });
  }

  for (const autoClosing of [true, false]) {
    test(`JSON, auto-closing ${autoClosing ? 'on' : 'off'}: a second copy of a read-only key is taken whole and not saved`, async ({ page }) => {
      const text = '{\n  "db": {\n    "user": "app",\n    "password": "hunter2"\n  }\n}\n';
      await mount(page, { language: 'json', value: text, autoClosing, readOnlyPaths: ['/db/password'] });
      await caretAt(page, text.indexOf('"app",') + '"app",'.length);
      await page.keyboard.press('Enter');
      await typeLines(page, ['"password": "x",']);
      const now = await value(input(page));
      expect(now).toContain('"password": "x",');
      expect(now).toContain('"password": "hunter2"');
      expect(await page.evaluate(() => window.qa.save())).toBe(false);
    });
  }
});
