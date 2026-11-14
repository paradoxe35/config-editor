import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Entries typed a key at a time beside a protected field, through the real
// keys: a new HCL block of a protected block's type with other labels is
// taken with auto-closing off (QA 37: the line was wiped when `{` was
// typed), a real second copy of the block is not; and a second copy of a
// read-only JSON key typed with auto-closing on is refused whole, as with
// it off (QA 37: `"password" "x",` was left).

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
    window.qa = new module.ConfigEditor(host, options);
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
    test(`HCL, ${name}, auto-closing off: a block with other labels is taken, a second copy is not`, async ({ page }) => {
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
      // Its header is refused: the field's block is written once.
      expect((await value(input(page))).split('resource "aws_instance" "web"').length).toBe(2);
    });
  }

  for (const autoClosing of [true, false]) {
    test(`JSON, auto-closing ${autoClosing ? 'on' : 'off'}: a second copy of a read-only key is refused whole`, async ({ page }) => {
      const text = '{\n  "db": {\n    "user": "app",\n    "password": "hunter2"\n  }\n}\n';
      await mount(page, { language: 'json', value: text, autoClosing, readOnlyPaths: ['/db/password'] });
      await caretAt(page, text.indexOf('"app",') + '"app",'.length);
      await page.keyboard.press('Enter');
      await typeLines(page, ['"password": "x",']);
      const lines = (await value(input(page))).split('\n');
      expect(lines[3].trim()).toBe('');
      expect(lines.join('\n')).not.toContain('"x"');
    });
  }
});
