import { expect, test } from '@playwright/test';
import { editor } from './helpers.mjs';

// Round 16: undoing an Edit-as switch left the other language's whole text
// selected, so the next key replaced the document; and any edit to a CRLF
// file turned every line ending into LF. Both with real keys, in the demo.

const saved = (page) => page.locator('.demo__output-text').evaluate((element) => element.textContent);
const pick = (page, value) => page.locator('.demo__edit-select').selectOption(value);

// Opens the demo on `text` exactly as given (line endings included).
async function open(page, baseURL, query, text) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.addInitScript((sample) => {
    window.configEditorSample = sample;
  }, text);
  await page.goto(`${baseURL}${query}&sample=page`);
  await page.locator('.config-editor__input').waitFor();
}

// Puts the caret just after `needle`'s first `offset` characters.
async function caretAt(e, needle, offset) {
  await e.input.evaluate(
    (element, [find, at]) => {
      const start = element.value.indexOf(find);
      if (start < 0) throw new Error(`no ${find}`);
      element.focus();
      element.setSelectionRange(start + at, start + at);
    },
    [needle, offset],
  );
}

const selection = (e) => e.input.evaluate((element) => [element.selectionStart, element.selectionEnd]);
const offsetOf = (e, needle, at) => e.input.evaluate((element, [find, by]) => element.value.indexOf(find) + by, [needle, at]);

test('undoing a switch leaves a caret where it was, and typing edits only there', async ({ page, baseURL }) => {
  const e = editor(page);
  const yaml = '# c\nlisten: 1\nweb:\n  http: 80\n';
  await open(page, baseURL, '?language=yaml', yaml);
  await caretAt(e, 'http', 2);
  await pick(page, 'json');
  await expect.poll(() => e.value()).toContain('"http": 80');
  // The caret keeps its place in the key.
  await expect.poll(() => selection(e)).toEqual(await Promise.all([offsetOf(e, 'http', 2), offsetOf(e, 'http', 2)]));
  await e.input.focus();
  await page.keyboard.press('Control+z');
  await expect.poll(() => e.value()).toBe(yaml);
  const at = yaml.indexOf('http') + 2;
  await expect.poll(() => selection(e)).toEqual([at, at]);
  await page.keyboard.type('X');
  await expect.poll(() => e.value()).toBe(yaml.replace('http', 'htXtp'));
  expect(page.errors).toEqual([]);
});

const CRLF = {
  yaml: ['# c\r\na: 1\r\nb: x\r\n', 'json'],
  json: ['{\r\n  "a": 1,\r\n  "b": "x"\r\n}\r\n', 'yaml'],
  toml: ['# c\r\na = 1\r\nb = "x"\r\n', 'yaml'],
  hcl: ['# c\r\na = 1\r\nb = "x"\r\n', 'yaml'],
  xml: ['<!-- c -->\r\n<a>1</a>\r\n<b>x</b>\r\n', 'yaml'],
};

for (const [language, [text, other]] of Object.entries(CRLF)) {
  for (const view of [language, other]) {
    test(`a ${language} CRLF file edited in the ${view} view keeps its line endings`, async ({ page, baseURL }) => {
      const e = editor(page);
      await open(page, baseURL, `?language=${language}${view === language ? '' : `&edit=${view}`}`, text);
      await expect.poll(() => e.value()).toContain('1');
      await e.input.evaluate((element) => {
        const at = element.value.indexOf('1') + 1;
        element.focus();
        element.setSelectionRange(at, at);
      });
      await page.keyboard.type('2');
      await expect.poll(() => saved(page)).toBe(text.replace('1', '12'));
      expect(page.errors).toEqual([]);
    });
  }
}
