import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// QA round 19 (H19-F6): text arriving whole in one browser command over a
// large selection (an IME commit, a drop, CDP `Input.insertText`) comes as
// one input event per line, each followed by a microtask, with the whole
// text in the textarea from the first. It is applied once, as a paste is:
// the later events find nothing new. Chrome's own textarea takes time
// growing with the square of the lines for such a command, so the editor is
// measured against a bare textarea given the same text: applying it once
// per line took 4–5 times as long as the bare textarea (47 s for 3000
// lines); now it takes about as long.

const LINES = 2000;

function document(indent) {
  const pad = ' '.repeat(indent);
  let text = `{\n${pad}"id": "srv-42",\n`;
  for (let index = 0; index < LINES; index += 1) text += `${pad}"k${index}": ${index},\n`;
  return `${text}${pad}"limits": {\n${pad}${pad}"max": 10\n${pad}}\n}\n`;
}

/** How long a bare textarea takes for `after` typed in one command over `before`. */
async function bareTextarea(page, before, after) {
  await page.evaluate((text) => {
    const area = document.createElement('textarea');
    area.id = 'bare';
    area.style.cssText = 'position:fixed;left:0;top:0;width:600px;height:300px';
    document.body.append(area);
    area.value = text;
    area.focus();
    area.setSelectionRange(0, text.length);
  }, before);
  const started = Date.now();
  await page.keyboard.insertText(after);
  const took = Date.now() - started;
  expect(await page.locator('#bare').inputValue()).toBe(after);
  await page.evaluate(() => document.getElementById('bare').remove());
  return took;
}

test.describe('text arriving whole in one command', () => {
  setupEditor(test, '?language=json');
  test.describe.configure({ timeout: 180_000 });

  test('a large text over the whole document is applied once, in about the time of a bare textarea, as one undo step', async ({
    page,
  }) => {
    const e = editor(page);
    const before = document(2);
    const after = document(4);
    const bare = await bareTextarea(page, before, after);
    await e.paste(before);
    await expect.poll(() => e.value()).toBe(before);
    await e.input.evaluate((element) => {
      element.focus();
      element.setSelectionRange(0, element.value.length);
    });
    const started = Date.now();
    await page.keyboard.insertText(after);
    await expect.poll(() => e.value()).toBe(after);
    const took = Date.now() - started;
    console.log(`insertText of ${LINES} lines over everything: editor ${took} ms, bare textarea ${bare} ms`);
    expect(took).toBeLessThan(bare * 1.8 + 1000);
    await page.keyboard.press('Control+z');
    await expect.poll(() => e.value()).toBe(before);
  });
});
