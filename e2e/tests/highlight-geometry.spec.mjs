import { expect, test } from '@playwright/test';
import { editor, setupEditor } from './helpers.mjs';

// Find highlights cover the glyphs the browser paints for the found text, as
// it lays them out: under the user's letter and word spacing (hard-bar
// `D4.a11y-2`: 20 px short and left of the painted word) and in right-to-left
// runs (`D4.a11y-5`: a substring's box landed over the neighbouring word).

const DOCUMENT = [
  'name: مرحبا بالعالم',
  'message: hello مرحبا world',
  'hebrew: שלום עולם',
  'plain: public_address here, public_address again',
  '',
].join('\n');

/** The first match box's rect and a DOM Range's over the same text in the painted line, for the first occurrence of `needle`. */
function measure(page, needle) {
  return page.evaluate((needle) => {
    const text = document.querySelector('.config-editor__input').value;
    const line = text.slice(0, text.indexOf(needle)).split('\n').length - 1;
    const row = [...document.querySelectorAll('.config-editor__code-line')][line];
    const start = row.textContent.indexOf(needle);
    const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
    const range = document.createRange();
    let offset = 0;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const end = offset + node.length;
      if (start >= offset && start < end) range.setStart(node, start - offset);
      if (start + needle.length > offset && start + needle.length <= end) range.setEnd(node, start + needle.length - offset);
      offset = end;
    }
    const painted = range.getBoundingClientRect();
    const box = document.querySelector('.config-editor__search-match-current').getBoundingClientRect();
    return {
      painted: [painted.left, painted.right, (painted.top + painted.bottom) / 2],
      box: [box.left, box.right, (box.top + box.bottom) / 2],
    };
  }, needle);
}

async function expectCovers(page, needle) {
  const e = editor(page);
  await e.input.click();
  await page.keyboard.press('Control+Home');
  await page.keyboard.press('Control+f');
  const find = page.getByRole('textbox', { name: 'Find', exact: true });
  await find.fill(needle);
  await expect(page.locator('.config-editor__search-match-current')).toHaveCount(1);
  const { painted, box } = await measure(page, needle);
  expect(box[0], `${needle}: left edge`).toBeCloseTo(painted[0], 0);
  expect(box[1], `${needle}: right edge`).toBeCloseTo(painted[1], 0);
  expect(Math.abs(box[2] - painted[2]), `${needle}: on its row`).toBeLessThan(3);
}

test.describe('find highlights follow the painted text', () => {
  setupEditor(test, '?language=yaml');

  test('under the user letter and word spacing (D4.a11y-2)', async ({ page }) => {
    await editor(page).paste(DOCUMENT);
    await page.addStyleTag({ content: '* { letter-spacing: 0.12em !important; word-spacing: 0.16em !important; }' });
    for (const needle of ['public_address', 'here', 'again', 'hello']) await expectCovers(page, needle);
  });

  test('in right-to-left runs (D4.a11y-5)', async ({ page }) => {
    await editor(page).paste(DOCUMENT);
    for (const needle of ['مرحبا', 'بالعالم', 'שלום', 'עולם', 'مرحبا بالعالم', 'hello']) await expectCovers(page, needle);
  });

  test('without either, as before', async ({ page }) => {
    await editor(page).paste(DOCUMENT);
    for (const needle of ['public_address', 'here', 'hello']) await expectCovers(page, needle);
  });
});
