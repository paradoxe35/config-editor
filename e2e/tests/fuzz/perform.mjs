// Runs one action in the page, as a user (real key events, the mouse, the
// clipboard, CDP's IME) or as the page's host (props, values, locks, tabs).
// `run` is the sequence's state: the page, its CDP session and the host
// model (the locks given, the extra props passed).

import { dragMove, f8, hover, suggestMouse } from './pointer.mjs';
import { clipboardTokens, crossCopy } from './secrets.mjs';

export const DOC_LANGUAGE = { main: 'yaml', limits: 'toml', clients: 'json' };

export async function focusText(page) {
  await page.evaluate(() => {
    const root = window.__fz.root();
    const input = root?.querySelector('.config-editor__input');
    if (input && document.activeElement !== input) input.focus();
  });
}

// The visible part of the editor's text area, in page pixels.
export async function textBox(page) {
  return page.evaluate(() => {
    const input = window.__fz.root()?.querySelector('.config-editor__input');
    if (!input) return null;
    const box = input.getBoundingClientRect();
    const top = Math.max(box.top, 0);
    const bottom = Math.min(box.bottom, innerHeight);
    const left = Math.max(box.left, 0);
    const right = Math.min(box.right, innerWidth);
    return bottom - top < 4 || right - left < 4 ? null : { x: left, y: top, width: right - left, height: bottom - top };
  });
}

export const at = (box, fx, fy) => ({ x: box.x + 2 + fx * (box.width - 4), y: box.y + 2 + fy * (box.height - 4) });

// Settles the host: its value follows the editor's (late echoes included).
export async function settle(page, ms = 1500) {
  const end = Date.now() + ms;
  let last = null;
  while (Date.now() < end) {
    const now = await page.evaluate(() => {
      const fz = window.__fz;
      const editor = fz.editor();
      return editor ? JSON.stringify([editor.value, fz.host()]) : null;
    });
    if (now && now === last) {
      const [value, host] = JSON.parse(now);
      if (value === host) return true;
    }
    last = now;
    await page.waitForTimeout(60);
  }
  return false;
}

// How long a syntax error takes to show (the editor's diagnostics pause,
// 250 ms, and a margin).
const ERROR_SHOWN_MS = 400;

// The lock a path names in `state` (the editor's value, `rangeOf` the
// path): its value, read off the field's text. A path locks the value; its
// key may be renamed. A field with no value (`public_address:` alone) is
// its key alone: nothing to mark.
function pathLock(state) {
  const field = state.value.slice(state.range.start, state.range.end);
  const colon = field.indexOf(':');
  const text = colon < 0 ? '' : field.slice(colon + 1).trim();
  return text ? { range: state.range, text, path: '/public_address' } : null;
}

// The lock a path names in a text that does not parse: `rangeOf` reads the
// field off its lines through the character breaking it, while the editor
// locks the field as the text reads with that character blanked, leaving
// it free for the repair (`protect::scan::stand_in`). The fuzz has no
// reading of its own of a value in such a text, so it takes the part of
// the value the editor marks, and from then on checks that it holds. None
// when a line of the value is out of view or its marks are not inside it.
async function markedLock(page, state, lock) {
  const at = state.value.indexOf(lock.text, state.range.start);
  const first = state.value.slice(0, at).split('\n').length;
  const lines = lock.text.split('\n');
  const marks = await page.evaluate(
    async ([first, count]) => {
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const root = window.__fz.root();
      return Array.from({ length: count }, (_, index) => {
        const row = root?.querySelector(`.config-editor__highlight .config-editor__code-line[data-line="${first + index}"]`);
        return row ? [...row.querySelectorAll('.config-editor__locked')].map((span) => span.textContent).join('').trim() : null;
      });
    },
    [first, lines.length],
  );
  // Each line's part of the value inside its marks (the editor may mark
  // more: the key's `:` before it).
  const kept = marks.map((mark, index) => {
    const piece = lines[index].trim();
    if (!mark || !piece) return null;
    if (mark.includes(piece)) return piece;
    return lines[index].includes(mark) ? mark : null;
  });
  const last = lines.length - 1;
  if (kept.some((part, index) => !part || (index > 0 && index < last && part !== lines[index].trim()))) return null;
  // From the first line's part to the end of the last line's.
  const start = lines[0].indexOf(kept[0]);
  const lastStart = lock.text.length - lines[last].length;
  const end = lastStart + lines[last].indexOf(kept[last]) + kept[last].length;
  return { ...lock, text: lock.text.slice(start, end) };
}

export async function host(page, call, arg) {
  return page.evaluate(
    ([call, arg]) => {
      const fz = window.__fz;
      const id = fz.activeId();
      const vanilla = !!window.documentsView || !!window.editor;
      const calls = {
        // A new text with the read-only ranges the host works out on it
        // (`ranges`), given together where the API allows.
        singleValue: ({ text, how, ranges }) => {
          const saved = document.getElementById('saved');
          if (window.editor && how === 'setValue') {
            // The host's own text: the editor does not report it back.
            window.editor.replaceValue(text);
            saved.textContent = text;
            window.editor.update({ readOnlyRanges: ranges });
          } else if (window.editor) {
            // The vanilla host's state is its `#saved`: what it sets, then
            // what `onChange` reports.
            saved.textContent = text;
            window.editor.update({ value: text, readOnlyRanges: ranges });
          } else {
            fz.extra = { ...fz.extra, readOnlyRanges: ranges };
            if (how === 'setValue') window.editorHandle.current?.setValue(text);
            else window.setParentValue(text);
            window.setExtraProps({ ...fz.extra });
          }
        },
        singleEcho: () => {
          const text = fz.host();
          if (window.editor) window.editor.update({ value: text });
          else window.setParentValue(text);
        },
        singleProps: (props) => {
          if (window.editor) window.editor.update(props);
          else {
            fz.extra = { ...fz.extra, ...props };
            window.setExtraProps({ ...fz.extra });
          }
        },
        docValue: ({ text, how, ranges }) => {
          if (how === 'setValue') {
            // The host lists the text it replaced the document with.
            if (vanilla) window.documentsView.replaceValue(id, text);
            else window.documentsHandle.current?.setValue(id, text);
            if (vanilla) window.updateFile(id, { value: text, readOnlyRanges: ranges });
            else window.updateFile(id, { readOnlyRanges: ranges });
          } else window.updateFile(id, { value: text, readOnlyRanges: ranges });
        },
        docEcho: () => typeof fz.host() === 'string' && window.updateFile(id, { value: fz.host() }),
        docProps: (props) => window.updateFile(id, props),
        editAs: (language) => {
          if (fz.kind === 'single') {
            const select = document.getElementById('edit-as');
            select.value = language;
            select.dispatchEvent(new Event('change', { bubbles: true }));
          } else {
            const own = { main: 'yaml', limits: 'toml', clients: 'json' }[id] ?? 'yaml';
            const target = language || own;
            if (vanilla) window.documentsView.editAs(id, target);
            else window.documentsHandle.current?.editAs(id, target);
            // The host keeps the language in its list, as the handle's
            // contract asks: its next list (an echo of an edit) would
            // switch back to the one it lists.
            window.updateFile(id, { editAs: target });
          }
        },
      };
      calls[call](arg);
    },
    [call, arg],
  );
}

// A lock's `{ start, end }` in `value` from the action's fractions; whole
// lines when `line`. Never splits a surrogate pair.
export function lockIndices(value, { at: fraction, length, line }) {
  if (!value.length) return null;
  let start = Math.floor(fraction * value.length);
  let end = Math.min(value.length, start + length);
  if (line) {
    start = value.lastIndexOf('\n', start - 1) + 1;
    const next = value.indexOf('\n', end);
    end = next < 0 ? value.length : next + 1;
  }
  const low = (index) => index > 0 && index < value.length && /[\uDC00-\uDFFF]/.test(value[index]);
  while (low(start)) start -= 1;
  while (low(end)) end += 1;
  return end > start ? { start, end } : null;
}

// Whether `got` is what a copy of `want` (an `fz.selected()`) puts on the
// clipboard.
function copied(got, want) {
  if (want.rest) return got.startsWith(want.text) && (want.rest.startsWith(got) || `${want.rest}\n`.startsWith(got));
  return want.line ? got === want.text || got === `${want.text}\n` : got === want.text;
}

// Null when the clipboard holds what a copy of `want` (`fz.selected()`
// before it) puts there, else the `copy` failure. The editor may rewrite
// its text between that reading and the key (an Edit-as switch waiting
// for the text to parse goes through after the typing pause): the copy
// then took the selection as the text after it left it, read again now.
export async function copyFits(page, want) {
  const got = await page.evaluate(() => navigator.clipboard.readText());
  if (copied(got, want)) return null;
  const now = await page.evaluate(() => window.__fz.selected());
  if (now && now.doc !== want.doc && copied(got, now)) return null;
  return {
    check: 'copy',
    detail: `copied ${JSON.stringify(got).slice(0, 120)}, selection ${JSON.stringify(want.text).slice(0, 120)}`,
  };
}

export async function perform(run, action) {
  const { page } = run;
  switch (action.k) {
    case 'type':
      await focusText(page);
      await page.keyboard.type(action.text);
      return;
    case 'key':
      await focusText(page);
      for (let index = 0; index < action.n; index += 1) await page.keyboard.press(action.key);
      return;
    case 'clip': {
      await focusText(page);
      // A copy puts the selection (or the caret's line) on the clipboard.
      const copy = action.key === 'Control+C';
      const want = copy
        ? await page.evaluate(async () => {
            await navigator.clipboard.writeText('\u2400 before the copy');
            return window.__fz.selected();
          })
        : null;
      await page.keyboard.press(action.key);
      await page.waitForTimeout(40);
      const wrong = await clipboardTokens(run);
      if (wrong) return wrong;
      if (!want || !want.text) return;
      return copyFits(page, want);
    }
    case 'chord':
      await focusText(page);
      for (const key of action.keys) await page.keyboard.press(key);
      return;
    case 'click':
    case 'drag': {
      const box = await textBox(page);
      if (!box) return;
      if (action.k === 'click') {
        const point = at(box, action.x, action.y);
        await page.mouse.click(point.x, point.y);
        return;
      }
      const from = at(box, action.x1, action.y1);
      const to = at(box, action.x2, action.y2);
      await page.mouse.move(from.x, from.y);
      await page.mouse.down();
      await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 3 });
      await page.mouse.move(to.x, to.y, { steps: 3 });
      await page.mouse.up();
      return;
    }
    case 'foldClick': {
      const chevrons = page.locator('.config-editor__fold-toggle:visible');
      const count = await chevrons.count();
      if (count)
        await chevrons
          .nth(Math.floor(action.at * count))
          .click({ timeout: 2000 })
          .catch(() => {});
      return;
    }
    case 'suggest': {
      await focusText(page);
      if (action.open) await page.keyboard.press('Control+Space');
      await page.waitForTimeout(80);
      const open = await page.evaluate(() => {
        const popup = window.__fz.root()?.querySelector('.config-editor__completions');
        return !!popup && !popup.className.includes('hidden');
      });
      if (!open) return;
      for (let index = 0; index < action.pick; index += 1) await page.keyboard.press('ArrowDown');
      await page.keyboard.press('Enter');
      return;
    }
    case 'find': {
      await focusText(page);
      await page.keyboard.press(action.replace === null ? 'Control+F' : 'Control+H');
      const inputs = page.locator('.config-editor__find-input:visible');
      if (!(await inputs.count())) return;
      await inputs.first().fill(action.query);
      await inputs.first().press('Enter');
      if (action.replace !== null && (await inputs.count()) > 1) {
        await inputs.nth(1).fill(action.replace);
        await inputs.nth(1).press(action.all ? 'Control+Alt+Enter' : 'Enter');
      }
      await page.keyboard.press('Escape');
      return;
    }
    case 'goto': {
      await focusText(page);
      await page.keyboard.press('Control+G');
      const goto = page.locator('.config-editor__goto-input:visible');
      if (!(await goto.count())) return;
      await goto.first().fill(action.column ? `${action.line}:${action.column}` : `${action.line}`);
      await goto.first().press('Enter');
      return;
    }
    case 'wheel': {
      const box = await textBox(page);
      if (!box) return;
      const point = at(box, 0.5, 0.5);
      await page.mouse.move(point.x, point.y);
      await page.mouse.wheel(0, action.dy);
      await page.waitForTimeout(60);
      return;
    }
    case 'resize':
      await page.setViewportSize({ width: action.width, height: action.height });
      await page.waitForTimeout(60);
      return;
    case 'ime': {
      await focusText(page);
      for (const step of action.steps) {
        await run.cdp.send('Input.imeSetComposition', { text: step, selectionStart: step.length, selectionEnd: step.length });
      }
      if (action.commit !== null) await run.cdp.send('Input.insertText', { text: action.commit });
      else await run.cdp.send('Input.imeSetComposition', { text: '', selectionStart: 0, selectionEnd: 0 });
      return;
    }
    case 'jump':
      await page.evaluate(({ at, length }) => {
        const editor = window.__fz.editor();
        const text = editor?.text;
        if (typeof text !== 'string') return;
        let start = Math.floor(at * text.length);
        if (/[\uDC00-\uDFFF]/.test(text[start] ?? '')) start -= 1;
        let end = Math.min(text.length, start + length);
        if (/[\uDC00-\uDFFF]/.test(text[end] ?? '')) end += 1;
        editor.revealRange({ start, end });
      }, action);
      return;
    case 'editAs':
      await host(page, 'editAs', action.language);
      run.editedAs = true;
      return;
    case 'hostValue': {
      run.hostReplaced = true;
      await settle(page);
      if (run.kind === 'single') {
        const ranges = run.keepLocks(null, action.text);
        run.give(null, action.text);
        return host(page, 'singleValue', { ...action, ranges });
      }
      const id = await page.evaluate(() => window.__fz.activeId());
      const { docsFor } = await import('./corpus.mjs');
      const docs = docsFor(DOC_LANGUAGE[id] ?? 'yaml');
      const text = docs[action.pick % docs.length];
      const ranges = run.keepLocks(id, text);
      run.give(id, text);
      return host(page, 'docValue', { text, how: action.how, ranges });
    }
    case 'echo':
      await settle(page, 600);
      return host(page, run.kind === 'single' ? 'singleEcho' : 'docEcho');
    case 'lock': {
      if (!(await settle(page))) return;
      const state = await page.evaluate(() => ({ value: window.__fz.editor()?.value, id: window.__fz.activeId() }));
      if (typeof state.value !== 'string') return;
      const range = lockIndices(state.value, action.range);
      if (!range) return;
      run.locked = true;
      run.setLock(state.id, 'range', { range, text: state.value.slice(range.start, range.end) });
      return host(page, run.kind === 'single' ? 'singleProps' : 'docProps', { readOnlyRanges: [range] });
    }
    case 'lockPath': {
      // A locked path may hold undo back (it would write the path again).
      run.locked = true;
      if (!(await settle(page))) return;
      // The syntax error, if any, is shown once its pause is over.
      await page.waitForTimeout(ERROR_SHOWN_MS);
      const state = await page.evaluate(() => {
        const editor = window.__fz.editor();
        return {
          value: editor?.value,
          text: editor?.text,
          range: editor?.rangeOf('/public_address') ?? null,
          errored: window.__fz.errored(),
        };
      });
      const lock = action.on && state.range && state.value === state.text ? pathLock(state) : null;
      const given = await host(page, 'singleProps', { readOnlyPaths: action.on ? ['/public_address'] : [] });
      run.setLock(null, 'path', lock && state.errored ? await markedLock(page, state, lock) : lock);
      return given;
    }
    case 'unlock': {
      run.locked = true;
      run.unlocked = true;
      const id = await page.evaluate(() => window.__fz.activeId());
      run.forgetLocks(id);
      return host(
        page,
        run.kind === 'single' ? 'singleProps' : 'docProps',
        run.kind === 'single' ? { readOnlyRanges: [], readOnlyPaths: [] } : { readOnlyRanges: [] },
      );
    }
    case 'prop':
      return performProp(run, action.prop);
    case 'tab': {
      const tabs = page.locator('[role="tab"]:visible');
      const count = await tabs.count();
      if (count)
        await tabs
          .nth(action.index % count)
          .click({ timeout: 2000 })
          .catch(() => {});
      return;
    }
    case 'tabKey':
      await focusText(page);
      await page.keyboard.press(action.key);
      return;
    case 'dragMove':
      return dragMove(run, action);
    case 'f8':
      return f8(run, action);
    case 'hover':
      return hover(run, action);
    case 'suggestMouse':
      return suggestMouse(run, action);
    case 'crossCopy':
      return crossCopy(run, action);
    default:
      throw new Error(`unknown action ${action.k}`);
  }
}

async function performProp(run, prop) {
  const { page } = run;
  if (prop === 'theme') {
    await page.evaluate(() => {
      const select = document.getElementById('theme');
      if (!select || select.options.length < 2) return;
      select.selectedIndex = (select.selectedIndex + 1) % select.options.length;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
  } else if (prop === 'readOnly') {
    await page.locator('#read-only').click();
  } else if (prop === 'footer') {
    run.footer = !run.footer;
    await host(page, 'singleProps', { showFooter: run.footer });
  } else if (prop === 'rename') {
    const rename = page.locator('#rename');
    if (await rename.count()) await rename.click();
  }
}
