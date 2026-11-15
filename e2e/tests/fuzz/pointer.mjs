// The pointer and problem actions: moving selected text by drag and drop,
// F8 / Shift+F8 through the problems, hovering for a hover card and into
// it, and the suggestion list opened and closed with the mouse. Each
// returns null, or `{ check, detail }` when what it did went wrong (the
// runner's checks after every step run as well).
import { at, focusText, textBox } from './perform.mjs';

const frames = (page) => page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));

// The characters of `text` other than white space, in order: a move keeps
// them all (a drop may indent its lines anew).
const kept = (text) => [...text.replace(/\s/g, '')].sort().join('');

// Selects `length` characters at `fraction` of the text, grabs the
// selection and drops it at a point of the text: the text after is the
// text before (the move refused, or dropped on itself) or the same
// characters moved, none lost or doubled.
export async function dragMove(run, action) {
  const { page } = run;
  const picked = await page.evaluate(({ at: fraction, length }) => {
    const editor = window.__fz.editor();
    const text = editor?.text;
    if (typeof text !== 'string' || !text.length) return null;
    let start = Math.floor(fraction * text.length);
    let end = Math.min(text.length, start + length);
    ({ start, end } = window.__fz.snap(text, start, end));
    if (end <= start) return null;
    editor.revealRange({ start, end });
    return { start, end, text };
  }, action);
  if (!picked) return null;
  await frames(page);
  const from = await page.evaluate(({ start, end }) => window.__fz.pointOf(Math.floor((start + end) / 2)), picked);
  const box = await textBox(page);
  if (!from || !box) return null;
  const to = at(box, action.x, action.y);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 4, from.y, { steps: 3 });
  await page.mouse.move(to.x, to.y, { steps: 8 });
  await page.mouse.up();
  // The drop lands at the end of the task, a refusal's repair with it.
  await page.waitForTimeout(150);
  const after = await page.evaluate(() => window.__fz.editor()?.text);
  if (typeof after === 'string' && after !== picked.text) run.reached('text moved by a drag');
  if (typeof after !== 'string' || after === picked.text || kept(after) === kept(picked.text)) return null;
  return {
    check: 'drag-move',
    detail: `dragging ${JSON.stringify(picked.text.slice(picked.start, picked.end))} made ${JSON.stringify(after).slice(0, 160)} of ${JSON.stringify(picked.text).slice(0, 160)}`,
  };
}

// F8 or Shift+F8, `n` times: after each the caret is on one of the
// problems the editor lists (`onProblems`), when it lists any. The list
// follows the text a moment later (F8 checks the text first).
export async function f8(run, action) {
  const { page } = run;
  await focusText(page);
  for (let press = 0; press < action.n; press += 1) {
    await page.keyboard.press(action.shift ? 'Shift+F8' : 'F8');
    let wrong = null;
    for (const started = Date.now(); Date.now() - started < 1200; ) {
      await frames(page);
      wrong = await page.evaluate(() => window.__fz.caretOffProblem());
      if (!wrong) break;
      await page.waitForTimeout(80);
    }
    if (wrong) return { check: 'f8', detail: `after ${action.shift ? 'Shift+F8' : 'F8'} ${press + 1}: ${wrong}` };
    if (await page.evaluate(() => (window.__fz.problems[window.__fz.activeId() ?? ''] ?? []).some((problem) => problem.line)))
      run.reached('F8 onto a listed problem');
  }
  return null;
}

// The key a JSON pointer ends on (a list item's: the list's).
function lastKey(pointer) {
  const keys = pointer
    .split('/')
    .slice(1)
    .map((key) => key.replace(/~1/g, '/').replace(/~0/g, '~'));
  while (keys.length && /^\d+$/.test(keys.at(-1))) keys.pop();
  return keys.at(-1) ?? null;
}

// What the hover card says against the place under the pointer: null when
// it documents the key the path there ends on (or there is no card, or no
// path to compare).
function cardMismatch(seen) {
  if (!seen.card || seen.key === null || !seen.under || typeof seen.pointer !== 'string' || !seen.pointer) return null;
  const key = lastKey(seen.pointer);
  if (key === null || key.replace(/^@/, '') === seen.key.replace(/^@/, '')) return null;
  return `the card shows ${JSON.stringify(seen.key)} with the pointer on ${seen.under.line}:${seen.under.column} (${JSON.stringify(seen.pointer)})`;
}

// The pointer rests on a character of a row in view (`at` of the rows,
// `column` of its characters): a hover card that shows documents the path
// under the pointer; with `into`, the pointer moves into the card and it
// stays; with `escape`, Escape closes it.
export async function hover(run, action) {
  const { page } = run;
  const point = await page.evaluate(({ at, column }) => window.__fz.pointInView(at, column), action);
  if (!point) return null;
  await page.mouse.move(point.x, point.y, { steps: 2 });
  // The card waits out a pause (350 ms); one for another key may take a
  // moment to go.
  await page.waitForTimeout(450);
  let seen = await page.evaluate(({ x, y }) => window.__fz.hoverCard(x, y), point);
  if (cardMismatch(seen)) {
    await page.waitForTimeout(500);
    seen = await page.evaluate(({ x, y }) => window.__fz.hoverCard(x, y), point);
    const wrong = cardMismatch(seen);
    if (wrong) return { check: 'hover', detail: wrong };
  }
  if (!seen.card) return null;
  run.reached(seen.under && typeof seen.pointer === 'string' ? 'hover card checked against its path' : 'hover card');
  if (action.into) {
    await page.mouse.move(seen.center.x, seen.center.y, { steps: 6 });
    await page.waitForTimeout(300);
    const still = await page.evaluate(() => !!window.__fz.root()?.querySelector('.config-editor__hover'));
    if (!still) return { check: 'hover-into', detail: `the card for ${JSON.stringify(seen.key)} closed as the pointer moved into it` };
    run.reached('pointer into a hover card');
  }
  if (action.escape) {
    // Escape closes it where the focus lets the editor have the key: in
    // its text, nowhere, or on the view's tabs.
    const free = await page.evaluate(() => {
      const focused = document.activeElement;
      return !focused || focused === document.body || focused.matches('.config-editor__input, [role="tab"]');
    });
    if (!free) return null;
    await page.keyboard.press('Escape');
    await frames(page);
    const open = await page.evaluate(() => !!window.__fz.root()?.querySelector('.config-editor__hover'));
    if (open) return { check: 'hover-escape', detail: `Escape left the card for ${JSON.stringify(seen.key)} open` };
    run.reached('Escape on a hover card');
  }
  return null;
}

const suggestionsOpen = (page) =>
  page.evaluate(() => {
    const popup = window.__fz.root()?.querySelector('.config-editor__completions');
    return !!popup && !popup.className.includes('hidden') && popup.getClientRects().length > 0;
  });

// The suggestion list asked for (Ctrl+Space), then one of its entries
// clicked, or a click outside the editor, which closes it.
export async function suggestMouse(run, action) {
  const { page } = run;
  await focusText(page);
  await page.keyboard.press('Control+Space');
  await page.waitForTimeout(100);
  if (!(await suggestionsOpen(page))) return null;
  if (action.outside) {
    // The page's corner, outside every editor.
    await page.mouse.click(2, 2);
    await frames(page);
    if (await suggestionsOpen(page)) return { check: 'suggest-close', detail: 'a click outside the editor left the suggestions open' };
    run.reached('suggestions closed by a click outside');
    return null;
  }
  const entries = page.locator('.config-editor__completion:visible');
  const count = await entries.count();
  if (count) run.reached('suggestion clicked');
  if (count)
    await entries
      .nth(action.pick % count)
      .click({ timeout: 2000 })
      .catch(() => {});
  return null;
}
