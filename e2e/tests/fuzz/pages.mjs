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
  // What a copy of the selection should put on the clipboard: the text
  // selected, hidden lines included; the caret's whole line when nothing
  // is selected.
  fz.selected = () => {
    const editor = fz.editor();
    const input = fz.root()?.querySelector('.config-editor__input');
    if (!editor || !input || typeof editor.text !== 'string') return null;
    const text = editor.text;
    const folded = (editor.foldedLines || []).map((range) => ({ startLine: range.startLine, endLine: range.endLine }));
    const start = fz.docOffset(text, folded, input.value, input.selectionStart);
    const end = fz.docOffset(text, folded, input.value, input.selectionEnd);
    if (start !== end) return { text: text.slice(start, end), line: false };
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    const next = text.indexOf('\n', start);
    // A closed fold on the line: its block goes too (and what closes it).
    const line = text.slice(0, start).split('\n').length;
    const folds = folded.some((range) => range.startLine === line);
    return { text: text.slice(lineStart, next < 0 ? text.length : next + 1), line: true, rest: folds ? text.slice(lineStart) : null };
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
  };
}
