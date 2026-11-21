// The example pages the browser fuzz drives (js/examples, built by
// `npm run build` in js/), and a probe installed in each: one way to read
// the editor's API, the host's value and the DOM on every page.
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', 'js');
export const BUILT =
  existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm')) && existsSync(join(js, 'examples', 'dist', 'react.js'));

// kind: `single` (one editor; `vanilla` its own host) or `documents` (tabs).
export const PAGES = {
  vanilla: { file: 'vanilla.html', kind: 'single', vanilla: true },
  react: { file: 'react.html', kind: 'single' },
  vue: { file: 'vue.html', kind: 'single' },
  svelte: { file: 'svelte.html', kind: 'single' },
  documents: { file: 'documents.html', kind: 'documents', vanilla: true },
  'documents-react': { file: 'documents-react.html', kind: 'documents' },
  'documents-vue': { file: 'documents-vue.html', kind: 'documents' },
  'documents-svelte': { file: 'documents-svelte.html', kind: 'documents' },
};

export const pageUrl = (name, query = '') => pathToFileURL(join(js, 'examples', PAGES[name].file)).href + query;

// Installed in the page once it is ready. `window.__fz` answers for the
// editor on screen whatever the page is.
export function installProbe({ kind, vanilla }) {
  const fz = {
    kind,
    hostSeen: [],
    extra: {},
    // The vanilla documents page keeps its list to itself: the documents it
    // hands the view are the host's state.
    hostDocs: null,
  };
  if (kind === 'documents' && vanilla) {
    // The list documents.html starts with (its values), until it hands the
    // view a new one.
    fz.hostDocs = [
      { id: 'main', value: 'listen: 7835\npublic_address: tunnel.example.com\n' },
      { id: 'limits', value: '[limits]\nmax_pending = 512\n' },
      { id: 'clients', value: '{\n  "clients": {}\n}\n' },
    ];
    const view = window.documentsView;
    const update = view.update.bind(view);
    view.update = (props) => {
      if (props && props.documents) fz.hostDocs = props.documents.map((file) => ({ ...file }));
      return update(props);
    };
  }
  fz.activeId = () => {
    if (kind !== 'documents') return null;
    if (vanilla) return window.documentsView.activeId;
    return window.documentsHandle.current?.activeId ?? null;
  };
  fz.editor = (id) => {
    if (kind === 'single') return vanilla ? window.editor : window.editorHandle.current;
    const target = id ?? fz.activeId();
    if (vanilla) return window.documentsView.editor(target);
    return window.documentsHandle.current?.editor(target) ?? null;
  };
  fz.files = () => {
    if (vanilla) return fz.hostDocs;
    return window.currentFiles().map((file) => ({ ...file }));
  };
  fz.host = (id) => {
    if (kind === 'single') return document.getElementById('saved').textContent;
    const file = fz.files().find((candidate) => candidate.id === (id ?? fz.activeId()));
    return file ? file.value : null;
  };
  // The text `readOnlyRanges` given alone name places in: the text the host
  // lists them with (its own, which a late echo may leave older than the
  // editor's), or on the vanilla editor page, whose `update` gives no value
  // with them, the editor's document.
  fz.rangesText = () => (kind === 'single' && vanilla ? window.editor.value : fz.host());
  // The editor on screen: the selected tab's panel's, or the one whose
  // textarea takes up room.
  fz.root = () => {
    if (kind === 'documents') {
      const tab = document.querySelector('[role="tab"][aria-selected="true"]');
      const panel = tab && document.getElementById(tab.getAttribute('aria-controls'));
      return panel?.querySelector('.config-editor') ?? null;
    }
    for (const input of document.querySelectorAll('.config-editor__input')) {
      if (input.offsetParent !== null && input.getClientRects().length) return input.closest('.config-editor');
    }
    return null;
  };
  // The index into `text` of the textarea's index `index`: the textarea
  // leaves out the lines inside closed folds.
  fz.docOffset = (text, folded, shown, index) => {
    const hidden = new Set();
    for (const { startLine, endLine } of folded) for (let line = startLine + 1; line <= endLine; line += 1) hidden.add(line);
    const lines = text.split('\n');
    const before = shown.slice(0, index).split('\n');
    const row = before.length - 1;
    const column = before[row].length;
    let offset = 0;
    for (let line = 1, seen = -1; line <= lines.length; line += 1) {
      if (!hidden.has(line)) seen += 1;
      if (seen === row && !hidden.has(line)) return offset + column;
      offset += lines[line - 1].length + 1;
    }
    return offset;
  };
  // Whether the editor on screen shows an error (a text that does not
  // parse: one writing a locked key again does not reach the host until it
  // parses).
  fz.errored = () =>
    [...(fz.root()?.querySelectorAll('.config-editor__diagnostic-marker') ?? [])].some(
      (marker) =>
        !marker.classList.contains('config-editor__diagnostic-marker-warning') &&
        !marker.classList.contains('config-editor__diagnostic-marker-info'),
    );
  // What a copy of the selection should put on the clipboard: the text
  // selected, hidden lines included; the caret's whole line when nothing
  // is selected. `doc` is the text it was read from.
  fz.selected = () => {
    const editor = fz.editor();
    const input = fz.root()?.querySelector('.config-editor__input');
    if (!editor || !input || typeof editor.text !== 'string') return null;
    const text = editor.text;
    const folded = (editor.foldedLines || []).map((range) => ({ startLine: range.startLine, endLine: range.endLine }));
    let start = fz.docOffset(text, folded, input.value, input.selectionStart);
    let end = fz.docOffset(text, folded, input.value, input.selectionEnd);
    // A selection cutting into a secret placeholder copies it whole.
    for (const token of start !== end ? text.matchAll(/«[^»\n]*»/g) : []) {
      const after = token.index + token[0].length;
      if (token.index < start && start < after) start = token.index;
      if (token.index < end && end < after) end = after;
    }
    if (start !== end) return { text: text.slice(start, end), line: false, doc: text };
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    const next = text.indexOf('\n', start);
    // A closed fold on the line: its block goes too (and what closes it).
    const line = text.slice(0, start).split('\n').length;
    const folds = folded.some((range) => range.startLine === line);
    return {
      text: text.slice(lineStart, next < 0 ? text.length : next + 1),
      line: true,
      rest: folds ? text.slice(lineStart) : null,
      doc: text,
    };
  };
  // The editor's own problems per document ('' on a single-editor page),
  // as `onProblems` reports them (a callback given later hears them at
  // once).
  fz.problems = {};
  const heard = (id, problems) => {
    fz.problems[id ?? ''] = problems;
  };
  if (kind === 'single' && vanilla) window.editor.update({ onProblems: (problems) => heard('', problems) });
  else if (kind === 'single') {
    fz.extra = { ...fz.extra, onProblems: (problems) => heard('', problems) };
    window.setExtraProps({ ...fz.extra });
  } else if (vanilla) window.documentsView.update({ onProblems: heard });
  else window.setViewSettings((settings) => ({ ...settings, onProblems: heard }));
  const lineStart = (text, line) =>
    text
      .split('\n')
      .slice(0, line - 1)
      .reduce((sum, before) => sum + before.length + 1, 0);
  const rowOf = (line) => fz.root()?.querySelector(`.config-editor__highlight .config-editor__code-line[data-line="${line}"]`) ?? null;
  // Each character a row shows: its UTF-16 index in the line and its box.
  const characters = (row) => {
    const out = [];
    const walker = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
    let index = 0;
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      for (let at = 0; at < node.length; at += 1) {
        const range = document.createRange();
        range.setStart(node, at);
        range.setEnd(node, at + 1);
        const rect = [...range.getClientRects()].find((box) => box.width > 0);
        if (rect) out.push({ index: index + at, rect });
      }
      index += node.length;
    }
    return out;
  };
  // The document place shown under the window point (x, y), by the
  // characters the rows show: `{ offset, line, column }` (UTF-16; line and
  // column from 1), or null off the characters.
  fz.charAt = (x, y) => {
    const editor = fz.editor();
    const root = fz.root();
    if (!editor || !root || typeof editor.text !== 'string') return null;
    for (const row of root.querySelectorAll('.config-editor__highlight .config-editor__code-line')) {
      const box = row.getBoundingClientRect();
      if (y < box.top || y >= box.bottom) continue;
      const line = Number(row.dataset.line);
      const hit = characters(row).find(({ rect }) => x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom);
      return hit ? { offset: lineStart(editor.text, line) + hit.index, line, column: hit.index + 1 } : null;
    }
    return null;
  };
  // The window point at the middle of the character at document offset
  // `offset`, or null when its row is not rendered.
  fz.pointOf = (offset) => {
    const text = fz.editor()?.text;
    if (typeof text !== 'string') return null;
    const line = text.slice(0, offset).split('\n').length;
    const row = rowOf(line);
    if (!row) return null;
    const index = offset - lineStart(text, line);
    const hit = characters(row).find((character) => character.index === index);
    if (!hit) return null;
    const { left, right, top, bottom } = hit.rect;
    return { x: (left + right) / 2, y: (top + bottom) / 2 };
  };
  // The window point at the middle of a character in view: of the rows
  // inside the text area's box, the one at `at` (a fraction), its
  // character at `column` (a fraction); null with none in view.
  fz.pointInView = (at, column) => {
    const input = fz.root()?.querySelector('.config-editor__input');
    if (!input) return null;
    const area = input.getBoundingClientRect();
    const rows = [...fz.root().querySelectorAll('.config-editor__highlight .config-editor__code-line')].filter((row) => {
      const box = row.getBoundingClientRect();
      return box.top >= area.top && box.bottom <= area.bottom && box.bottom <= innerHeight && row.textContent.trim() !== '';
    });
    if (!rows.length) return null;
    const shown = characters(rows[Math.min(rows.length - 1, Math.floor(at * rows.length))]).filter(
      ({ rect }) => rect.right <= area.right && rect.right <= innerWidth,
    );
    if (!shown.length) return null;
    const { left, right, top, bottom } = shown[Math.min(shown.length - 1, Math.floor(column * shown.length))].rect;
    return { x: (left + right) / 2, y: (top + bottom) / 2 };
  };
  // `start..end` of `text` widened to whole characters as the user sees
  // them (a flag's two halves, an emoji's joined parts, an accent and its
  // letter): where a user's selection can end.
  fz.snap = (text, start, end) => {
    const segments = [...new Intl.Segmenter().segment(text)];
    const before = (at) => (at >= text.length ? text.length : (segments.findLast((segment) => segment.index <= at)?.index ?? 0));
    const after = (at) => {
      const found = segments.find((segment) => segment.index + segment.segment.length >= at);
      return found ? found.index + (found.index === at ? 0 : found.segment.length) : text.length;
    };
    return { start: before(start), end: Math.max(before(start), after(end)) };
  };
  // The caret (the selection's moving end): its document offset, line and
  // column (UTF-16, from 1).
  fz.caret = () => {
    const editor = fz.editor();
    const input = fz.root()?.querySelector('.config-editor__input');
    if (!editor || !input || typeof editor.text !== 'string') return null;
    const folded = (editor.foldedLines || []).map((range) => ({ startLine: range.startLine, endLine: range.endLine }));
    const head = input.selectionDirection === 'backward' ? input.selectionStart : input.selectionEnd;
    const offset = fz.docOffset(editor.text, folded, input.value, head);
    const line = editor.text.slice(0, offset).split('\n').length;
    return { offset, line, column: offset - lineStart(editor.text, line) + 1 };
  };
  // Null when the caret is on one of the problems listed for the document
  // on screen (or none is listed), else what is wrong. A problem is
  // `{ line, column, endLine?, endColumn? }`; the caret steps out of a
  // secret placeholder, so either end of one holding the problem counts.
  fz.caretOffProblem = () => {
    const editor = fz.editor();
    const listed = (fz.problems[fz.activeId() ?? ''] ?? []).filter((problem) => problem.line);
    const caret = fz.caret();
    if (!listed.length || !caret) return null;
    const lineText = editor.text.split('\n')[caret.line - 1] ?? '';
    // The caret's column in characters too: either unit may be meant.
    const columns = [caret.column, [...lineText.slice(0, caret.column - 1)].length + 1];
    const tokens = [...lineText.matchAll(/«[^»\n]*»/g)].map((found) => [found.index + 1, found.index + 1 + found[0].length]);
    const on = (problem, column) => {
      if (problem.line !== caret.line) return false;
      const start = Math.min(problem.column ?? 1, lineText.length + 1);
      const end = problem.endLine && problem.endLine !== problem.line ? Infinity : Math.max(problem.endColumn ?? start, start);
      if (column >= start && column <= end) return true;
      return tokens.some(([from, to]) => start > from && start < to && (column === from || column === to));
    };
    if (listed.some((problem) => columns.some((column) => on(problem, column)))) return null;
    // The walk visits the problems with no place last (the footer shows
    // them): the caret stays, and the announcement names the one reached.
    const said = fz.root()?.querySelector('.config-editor__announcement')?.textContent ?? '';
    const unplaced = (fz.problems[fz.activeId() ?? ''] ?? []).filter((problem) => !problem.line);
    if (unplaced.some((problem) => problem.message && said.replace(/ +$/, '').endsWith(problem.message))) return null;
    const places = listed.map((problem) => [problem.line, problem.column, problem.endLine ?? null, problem.endColumn ?? null]);
    return `caret at ${caret.line}:${caret.column}, problems at ${JSON.stringify(places).slice(0, 200)}`;
  };
  // The hover card on screen: its key, the place under the point (x, y)
  // and the path there.
  fz.hoverCard = (x, y) => {
    const card = fz.root()?.querySelector('.config-editor__hover');
    if (!card) return { card: false };
    const under = fz.charAt(x, y);
    let pointer = null;
    try {
      pointer = under ? (fz.editor().pathAt(under.offset)?.pointer ?? null) : null;
    } catch (error) {
      pointer = { error: String(error) };
    }
    const box = card.getBoundingClientRect();
    return {
      card: true,
      key: card.querySelector('.config-editor__hover-key')?.textContent ?? null,
      // The pointer is in the card, which took it (one that lets it
      // through to the text under it is marked `data-passing`). Read off
      // what is at the point, not `:hover`: a headless browser moves the
      // hover only on a mouse move, so a card that let a pointer through
      // and then took it where it stopped keeps `:hover` on the text.
      inside: !card.hasAttribute('data-passing') && card.contains(document.elementFromPoint(x, y)),
      under,
      pointer,
      center: { x: box.left + box.width / 2, y: box.top + Math.min(box.height / 2, 24) },
    };
  };
  // Every secret placeholder (or a piece of one) anywhere on the page:
  // `whole`, as each row of an editor's text, attribute and field holds
  // it; `pieces`, as other text holds it (an underline repeats the
  // characters it marks, maybe a token's first only).
  fz.pageTokens = () => {
    const whole = new Set();
    const pieces = new Set();
    const add = (text, into) => {
      if (typeof text === 'string' && text.includes('«')) for (const token of text.match(/«[^»\n]{0,60}»?/g) ?? []) into.add(token);
    };
    const rows = '.config-editor__highlight .config-editor__code-line';
    for (const row of document.body.querySelectorAll(rows)) add(row.textContent, whole);
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      if (node.data.includes('«') && !node.parentElement?.closest(rows)) add(node.data, pieces);
    }
    for (const element of document.body.querySelectorAll('*')) {
      for (const attribute of element.attributes) add(attribute.value, whole);
      if (element instanceof HTMLTextAreaElement || element instanceof HTMLInputElement) add(element.value, whole);
    }
    return { whole: [...whole], pieces: [...pieces] };
  };
  window.__fz = fz;
}

// What the page shows and what its API says, in one read (runs in the page).
export function snapshot() {
  const fz = window.__fz;
  const editor = fz.editor();
  const root = fz.root();
  if (!editor || !root) return { missing: true };
  const input = root.querySelector('.config-editor__input');
  const style = getComputedStyle(input);
  const rows = [...root.querySelectorAll('.config-editor__highlight .config-editor__code-line')].map((line) => ({
    n: Number(line.dataset.line),
    t: line.textContent,
    // The text the editor marks locked on the row.
    m: [...line.querySelectorAll('.config-editor__locked')].map((span) => span.textContent).join(''),
  }));
  const gutter = [...root.querySelectorAll('.config-editor__gutter-content > div[data-line]')].map((line) => Number(line.dataset.line));
  const active = [...root.querySelectorAll('.config-editor__code-line-active')].map((line) => Number(line.dataset.line));
  const activeId = fz.activeId();
  let value = null;
  try {
    value = editor.value ?? null;
  } catch (error) {
    value = { error: String(error) };
  }
  // The caret (the selection's moving end) as an index into `text`: the
  // textarea leaves out the lines in closed folds.
  const text = editor.text;
  const folded = (editor.foldedLines || []).map((range) => ({ startLine: range.startLine, endLine: range.endLine }));
  const head = input.selectionDirection === 'backward' ? input.selectionStart : input.selectionEnd;
  let caretPath = null;
  if (typeof text === 'string') {
    const offset = fz.docOffset(text, folded, input.value, head);
    try {
      caretPath = { offset, at: editor.pathAt(offset)?.pointer ?? null, current: editor.currentPath?.pointer ?? null };
    } catch (error) {
      caretPath = { error: String(error) };
    }
  }
  const own = { limits: 'TOML', clients: 'JSON' }[activeId] ?? 'YAML';
  const label = root.querySelector('.config-editor__frame')?.getAttribute('aria-label') ?? '';
  return {
    activeId,
    ownLanguage: label === `${own} editor`,
    label,
    value,
    text,
    folded,
    caretPath,
    host: fz.host(),
    ta: {
      value: input.value,
      start: input.selectionStart,
      end: input.selectionEnd,
      head,
      scrollTop: input.scrollTop,
      clientHeight: input.clientHeight,
      lineHeight: parseFloat(style.lineHeight),
      paddingTop: parseFloat(style.paddingTop),
      readOnly: input.readOnly,
      focused: document.activeElement === input,
    },
    rows,
    gutter,
    active,
    locked: [...root.querySelectorAll('.config-editor__locked')].map((span) => span.textContent),
    problems: fz.problems[activeId ?? ''] ?? null,
    pageTokens: fz.pageTokens(),
    errored: fz.errored(),
  };
}
