// The random actions: what a user does with keyboard, mouse, clipboard and
// IME, and what a host does with props, values, locks and tabs. Each action
// is plain JSON, so a failing sequence prints and replays as it is.
import { COMPOSITIONS, PIECES, WIDE, WORDS, docsFor } from './corpus.mjs';

const MOVES = ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown', 'Control+Home', 'Control+End'];
const LINE_COMMANDS = [
  'Alt+ArrowUp',
  'Alt+ArrowDown',
  'Shift+Alt+ArrowUp',
  'Shift+Alt+ArrowDown',
  'Control+Shift+K',
  'Control+Enter',
  'Control+Shift+Enter',
  'Control+/',
  'Control+]',
  'Control+[',
  'Tab',
  'Shift+Tab',
];
const FOLDS = [['Control+Shift+BracketLeft'], ['Control+Shift+BracketRight'], ['Control+K', 'Control+0'], ['Control+K', 'Control+J']];
const VIEWPORTS = [
  { width: 1100, height: 620 },
  { width: 700, height: 400 },
  { width: 420, height: 300 },
  { width: 1400, height: 900 },
  { width: 900, height: 180 },
];
const EDIT_AS = ['', 'json', 'toml', 'hcl', 'xml'];

function someText(r) {
  const kind = r.weighted([
    [5, WORDS],
    [3, PIECES],
    [3, WIDE],
  ]);
  let text = r.pick(kind);
  if (r.chance(0.2)) text += r.pick(WIDE);
  if (r.chance(0.1)) text += '\n' + r.pick(WORDS);
  return text;
}

// One random action for a page of `kind` ('single' | 'documents').
// `userOnly`: no host actions (so undo all the way can be checked).
export function generate(r, { kind }, { userOnly = false } = {}) {
  const user = [
    [14, () => ({ k: 'type', text: someText(r) })],
    [6, () => ({ k: 'key', key: r.pick(['Enter', 'Backspace', 'Delete']), n: r.int(1, 3) })],
    [8, () => ({ k: 'key', key: (r.chance(0.5) ? 'Shift+' : '') + r.pick(MOVES), n: r.int(1, 4) })],
    [5, () => ({ k: 'click', x: round(r.next()), y: round(r.next()) })],
    [3, () => ({ k: 'drag', x1: round(r.next()), y1: round(r.next()), x2: round(r.next()), y2: round(r.next()) })],
    [2, () => ({ k: 'key', key: 'Control+A', n: 1 })],
    [3, () => ({ k: 'clip', key: r.pick(['Control+C', 'Control+X']) })],
    [3, () => ({ k: 'clip', key: 'Control+V' })],
    [5, () => ({ k: 'key', key: r.pick(['Control+Z', 'Control+Shift+Z', 'Control+Y']), n: r.int(1, 3) })],
    [5, () => ({ k: 'key', key: r.pick(LINE_COMMANDS), n: 1 })],
    [3, () => ({ k: 'suggest', open: r.chance(0.6), pick: r.int(0, 2) })],
    [3, () => ({ k: 'chord', keys: r.pick(FOLDS) })],
    [1, () => ({ k: 'foldClick', at: round(r.next()) })],
    [
      2,
      () => ({
        k: 'find',
        query: r.pick(['e', 'key', ':', '1', '😀', 'value']),
        replace: r.chance(0.5) ? r.pick(['', 'Z', '日本', 'ab']) : null,
        all: r.chance(0.5),
      }),
    ],
    [2, () => ({ k: 'goto', line: r.int(1, 30), column: r.chance(0.5) ? r.int(1, 12) : null })],
    [3, () => ({ k: 'wheel', dy: r.pick([-600, -120, 120, 600, 3000]) })],
    [1, () => ({ k: 'resize', ...r.pick(VIEWPORTS) })],
    [
      3,
      () => {
        const composition = r.pick(COMPOSITIONS);
        return { k: 'ime', steps: composition.steps, commit: r.chance(0.85) ? composition.commit : null };
      },
    ],
    [2, () => ({ k: 'editAs', language: r.pick(EDIT_AS) })],
    // The caret (or a selection) anywhere in the text, through `revealRange`:
    // reaches locked and folded places a click on a long text rarely does.
    // The host's echoes settle; the runner checks the host holds the text.
    [2, () => ({ k: 'pause' })],
    [3, () => ({ k: 'jump', at: round(r.next()), length: r.pick([0, 0, 1, 4, 12]) })],
    // Selected text moved by mouse drag and drop.
    [3, () => ({ k: 'dragMove', at: round(r.next()), length: r.pick([1, 3, 6, 14, 40]), x: round(r.next()), y: round(r.next()) })],
    [2, () => ({ k: 'f8', shift: r.chance(0.35), n: r.int(1, 3) })],
    [3, () => ({ k: 'hover', at: round(r.next()), column: round(r.next() * 0.45), into: r.chance(0.5), escape: r.chance(0.5) })],
    [1, () => ({ k: 'suggestMouse', outside: r.chance(0.4), pick: r.int(0, 3) })],
  ];
  if (kind === 'documents') {
    // Copy from one document, paste into another: a secret placeholder
    // most of the time, which the host gives the first document first
    // (`load`) when host actions are on.
    user.push([
      5,
      () => ({
        k: 'crossCopy',
        from: r.int(0, 2),
        to: r.int(0, 2),
        load: !userOnly && r.chance(0.6),
        token: r.chance(0.7),
        at: round(r.next()),
        length: r.pick([1, 5, 14]),
        at2: round(r.next()),
        over: r.pick([0, 0, 1, 4]),
      }),
    ]);
  }
  const host =
    kind === 'single'
      ? [
          [2, () => ({ k: 'hostValue', text: r.pick(docsFor('yaml')), how: r.pick(['prop', 'setValue']) })],
          [2, () => ({ k: 'echo' })],
          [2, () => ({ k: 'lock', range: lockRange(r) })],
          [1, () => ({ k: 'lockPath', on: r.chance(0.6) })],
          [1, () => ({ k: 'unlock' })],
          [1, () => ({ k: 'prop', prop: r.pick(['theme', 'readOnly', 'footer', 'rename']) })],
          [2, () => hostChange(r)],
          [1, () => ({ k: 'hideShow', wait: r.pick([0, 0, 50, 300]) })],
          [1, () => lateDiagnostics(r)],
        ]
      : [
          [3, () => ({ k: 'tab', index: r.int(0, 3) })],
          [1, () => ({ k: 'tabKey', key: r.pick(['Alt+PageDown', 'Alt+PageUp']) })],
          [2, () => ({ k: 'hostValue', text: null, pick: r.int(0, 9), how: r.pick(['file', 'setValue']) })],
          [2, () => ({ k: 'echo' })],
          [2, () => ({ k: 'lock', range: lockRange(r) })],
          [1, () => ({ k: 'unlock' })],
          [2, () => hostChange(r)],
          [1, () => lateDiagnostics(r)],
        ];
  return r.weighted(userOnly ? user : [...user, ...host])();
}

const round = (value) => Math.round(value * 1000) / 1000;

// The host's own change (`applyChanges`): a line put in at a line start (a
// fraction of the text), on the text the host holds (named by its
// revision, maybe behind the editor's) or on the editor's text now.
const hostChange = (r) => ({ k: 'hostChange', at: round(r.next()), base: r.chance(0.6), line: r.pick(['# host note', 'host_key: 1']) });

// Problems the host found on a text it holds, given late: anchored to that
// text (its revision or the text itself), on a line of it (a fraction).
const lateDiagnostics = (r) => ({ k: 'lateDiagnostics', at: round(r.next()), by: r.pick(['revision', 'text']), wait: r.pick([0, 200]) });

// Where in the host's text a lock starts (a fraction of it) and how long it
// is; the runner turns it into `{ start, end }` string indices.
function lockRange(r) {
  return { at: round(r.next()), length: r.pick([3, 5, 8, 14, 30]), line: r.chance(0.4) };
}

export const generateLock = (r) => ({ k: 'lock', range: lockRange(r) });

// A simpler version of `action` for shrinking, or null when it is as simple
// as it gets.
export function simplify(action) {
  if (action.n > 1) return { ...action, n: 1 };
  if (action.k === 'type' && [...action.text].length > 1) return { ...action, text: [...action.text].slice(0, -1).join('') };
  if (action.k === 'ime' && action.steps.length > 1) return { ...action, steps: action.steps.slice(-1) };
  if (action.k === 'find' && action.replace !== null) return { ...action, replace: null, all: false };
  if (action.k === 'drag') return { k: 'click', x: action.x2, y: action.y2 };
  if (action.k === 'hover' && (action.into || action.escape)) return { ...action, into: false, escape: false };
  if (action.k === 'crossCopy' && action.load) return { ...action, load: false };
  return null;
}
