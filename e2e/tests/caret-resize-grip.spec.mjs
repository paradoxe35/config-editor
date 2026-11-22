import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { DOCUMENTS_FRAMEWORK_PAGES } from './helpers.mjs';

// A press on the text's resize grip (its bottom-right corner) is a click
// the browser does not place the caret for: Chrome collapses a selection
// there and tells only the document (`selectionchange` on `document`, none
// on the textarea). The active line follows the caret all the same, from
// any selection, on the demo and on every documents page.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const example = (name) => pathToFileURL(join(js, 'examples', name)).href;

// How the selection is made before the press: [label, select(element)].
const SELECTIONS = [
  ['all, by Ctrl+A', null],
  ['backward over the last lines', (element) => element.setSelectionRange(5, element.value.length - 1, 'backward')],
  ['forward over the last lines', (element) => element.setSelectionRange(5, element.value.length - 1, 'forward')],
  ['a caret on the last line', (element) => element.setSelectionRange(element.value.length, element.value.length)],
];

// Points on the grip's left edge, in px from the text's right and bottom
// edges: Chrome collapses the selection there (to the text's start).
const EDGE = [15.4, 15.2, 15.03, 14.9, 14.7];

async function pressGrip(page, input, how, fromRight) {
  await input.focus();
  if (how) await input.evaluate(how);
  else await page.keyboard.press('Control+A');
  await page.waitForTimeout(60);
  const box = await input.boundingBox();
  await page.mouse.click(box.x + box.width - fromRight, box.y + box.height - 6.5);
  await page.waitForTimeout(120);
}

// The caret's line (the selection's moving end) and the lines drawn active.
const lines = (input) =>
  input.evaluate((element) => {
    const head = element.selectionDirection === 'backward' ? element.selectionStart : element.selectionEnd;
    const root = element.closest('.config-editor');
    return {
      caret: element.value.slice(0, head).split('\n').length,
      active: [...root.querySelectorAll('.config-editor__code-line-active')].map((line) => Number(line.dataset.line)),
    };
  });

// Each press on the grip's edge from the selection `how` makes: the
// active line is the caret's after every one.
async function expectAgree(page, input, how) {
  for (const fromRight of EDGE) {
    await pressGrip(page, input, how, fromRight);
    await expect
      .poll(async () => {
        const { caret, active } = await lines(input);
        return active.join() === String(caret) ? 'agree' : `caret on line ${caret}, active ${JSON.stringify(active)}`;
      }, { message: `pressed ${fromRight}px from the right` })
      .toBe('agree');
  }
}

test.describe('the resize grip pressed: the active line follows the caret', () => {
  for (const [label, how] of SELECTIONS) {
    test(`demo, from ${label}`, async ({ page, baseURL }) => {
      await page.goto(baseURL);
      const input = page.locator('.config-editor__input');
      await input.waitFor();
      await expectAgree(page, input, how);
    });
  }

  for (const name of ['documents.html', ...DOCUMENTS_FRAMEWORK_PAGES]) {
    for (const [label, how] of SELECTIONS) {
      test(`${name}, from ${label}`, async ({ page }) => {
        test.skip(!built, 'needs the npm build (cd js && npm run build)');
        await page.goto(example(name));
        await page.waitForFunction(() => document.body.dataset.ready === 'true');
        const input = page.locator('.config-editor__input:visible');
        await input.waitFor();
        await expectAgree(page, input, how);
      });
    }
  }
});
