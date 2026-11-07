import { expect, test } from '@playwright/test';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The Rust API and the JS packages stay in step: every public prop of
// `ConfigEditor` and `ConfigDocuments`, method of `EditorHandle`,
// `DocumentsHandle`, `Document`, `Language`, `ChangeSummary`, `Schema` and
// `EditorTheme`, and item re-exported from the crate root has a
// JavaScript counterpart (an option, a method, a function or a type) and a
// React one (a prop, a ref member or a re-export), or an exemption saying
// why not. `EditorSettings` is not mapped by hand: its TypeScript type is
// generated from the struct (js/config-editor/tests/settings.rs checks it
// is current); here every field must be in it, and the JS options and React
// props must extend it. Reads the sources only: no build, no browser.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (...path) => readFileSync(join(root, ...path), 'utf8');
const crate = (path) => read('crates', 'config-editor-leptos', 'src', ...path.split('/'));
// `alloc_stats.rs` is a debugging aid behind a cargo feature, never published.
const jsSources = readdirSync(join(root, 'js', 'config-editor', 'src'))
  .filter((name) => name.endsWith('.rs') && name !== 'alloc_stats.rs')
  .map((name) => read('js', 'config-editor', 'src', name))
  .join('\n');
const react = read('js', 'config-editor-react', 'src', 'index.tsx');

// --- The mapping: Rust name → JS name → React name, or an exemption. -------
//
// `js` names an option (props), a method of the `ConfigEditor` class
// (handle), or an exported function or type (everything else, as `kind`
// says). `react` defaults to the same name; `react: null` needs `reactWhy`.

const PROPS = {
  language: { js: 'language' },
  edit_as: { js: 'editAs' },
  initial_value: { exempt: '`value` is both the initial and the live document in JS; React has `defaultValue`' },
  value: { js: 'value' },
  baseline: { js: 'baseline' },
  schema: { js: 'schema' },
  file_name: { js: 'fileName' },
  class: { js: 'className', react: 'editorClassName' },
  settings: { js: 'EditorSettings', kind: 'extends' },
  on_change: { js: 'onChange' },
  on_edit_as_change: { js: 'onEditAsChange' },
  on_change_summary: { js: 'onChangeSummary' },
  on_schema_warning: { js: 'onSchemaWarning' },
  read_only: { js: 'readOnly' },
  handle: { exempt: 'the `ConfigEditor` instance (React: its ref) is the handle' },
  host_diagnostics: { js: 'hostDiagnostics' },
  on_path_change: { js: 'onPathChange' },
  on_save: { js: 'onSave' },
  on_dirty_change: { js: 'onDirtyChange' },
  on_baseline_conflict: { js: 'onBaselineConflict' },
  read_only_paths: { js: 'readOnlyPaths' },
  read_only_ranges: { js: 'readOnlyRanges' },
  secrets: { js: 'secrets' },
};

// React props with no JS option of their own.
const REACT_ONLY_PROPS = {
  defaultValue: 'the uncontrolled initial document (JS: `value`)',
  className: 'the class of the container `<div>` the component renders',
  style: 'the style of that container',
  onError: 'mount and prop errors, which JS throws instead',
};

const HANDLE = {
  new: { exempt: '`new ConfigEditor(element, options)` mounts and returns the handle' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (React: `ref.current.editor`)' },
  text: { js: 'text' },
  range_of: { js: 'rangeOf' },
  path_at: { js: 'pathAt' },
  current_path: { js: 'currentPath' },
  reveal: { js: 'reveal' },
  save: { js: 'save' },
  is_dirty: { js: 'isDirty' },
  fold: { js: 'fold' },
  unfold: { js: 'unfold' },
  fold_all: { js: 'foldAll' },
  unfold_all: { js: 'unfoldAll' },
  folded_lines: { js: 'foldedLines' },
  focus: { js: 'focus' },
};

const LANGUAGE = {
  new: { js: 'registerLanguage', kind: 'function' },
  info: { js: 'languageInfo', kind: 'function' },
  label: { js: 'languageInfo', kind: 'function' },
  file_icon: { js: 'languageInfo', kind: 'function' },
  extensions: { js: 'languageInfo', kind: 'function' },
  default_file_name: { js: 'languageInfo', kind: 'function' },
  from_path: { js: 'languageForFile', kind: 'function' },
  parse: { js: 'parse', kind: 'function' },
  serialize: { js: 'serialize', kind: 'function' },
  convert: { js: 'convert', kind: 'function' },
  range_of: { js: 'rangeOf', kind: 'function' },
  fold_ranges: { js: 'foldRanges', kind: 'function' },
  path_at: { js: 'pathAt', kind: 'function' },
};

const CHANGE_SUMMARY = {
  between: { js: 'changeSummary', kind: 'function' },
  is_empty: { exempt: 'a summary is a plain object in JS: `summary.hunks === 0`' },
};

const SCHEMA = {
  compile: { exempt: 'JS takes the schema as JSON text wherever one is used, and throws there when it is invalid' },
  warnings: { js: 'schemaWarnings', kind: 'function' },
  validate: { js: 'validate', kind: 'function' },
  convert: { js: 'convert', kind: 'function' },
};

const THEME = {
  as_str: { js: 'themes', kind: 'function' },
  label: { js: 'themeInfo', kind: 'function' },
  css_family: { exempt: 'a layout detail, shown as the root\'s `data-theme` attribute' },
};

// `ConfigDocuments`: props → options of the `ConfigDocuments` class (React:
// props of its component), handle → its methods (React: ref members).
const DOCUMENTS_PROPS = {
  documents: { js: 'documents' },
  initial_active: { js: 'activeId' },
  on_select: { js: 'onSelect' },
  on_close: { js: 'onClose' },
  on_change: { js: 'onChange' },
  on_save: { js: 'onSave' },
  on_dirty_change: { js: 'onDirtyChange' },
  settings: { js: 'EditorSettings', kind: 'extends' },
  class: { js: 'className', react: 'viewClassName' },
  label: { js: 'label' },
  handle: { exempt: 'the `ConfigDocuments` instance (React: its ref) is the handle' },
};

const DOCUMENTS_REACT_ONLY_PROPS = {
  className: 'the class of the container `<div>` the component renders',
  style: 'the style of that container',
  onError: 'mount and prop errors, which JS throws instead',
};

const DOCUMENTS_HANDLE = {
  new: { exempt: '`new ConfigDocuments(element, options)` mounts and returns the handle' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (React: `ref.current.instance`)' },
  active: { js: 'activeId' },
  select: { js: 'select' },
  editor: { js: 'editor' },
  active_editor: { exempt: '`editor(activeId)`' },
  is_dirty: { js: 'isDirty' },
  save: { js: 'save' },
  save_all: { js: 'saveAll' },
  focus: { js: 'focus' },
};

// `Document`'s builder methods are the fields of a `DocumentSpec`.
const DOCUMENT = {
  new: { js: 'DocumentSpec', kind: 'type' },
  language: { js: 'DocumentSpec', kind: 'type' },
  baseline: { js: 'DocumentSpec', kind: 'type' },
  read_only: { js: 'DocumentSpec', kind: 'type' },
  schema: { js: 'DocumentSpec', kind: 'type' },
  edit_as: { js: 'DocumentSpec', kind: 'type' },
  settings: { js: 'DocumentSpec', kind: 'type' },
  host_diagnostics: { js: 'DocumentSpec', kind: 'type' },
  read_only_paths: { js: 'DocumentSpec', kind: 'type' },
  read_only_ranges: { js: 'DocumentSpec', kind: 'type' },
  secrets: { js: 'DocumentSpec', kind: 'type' },
  resolved_language: { js: 'languageForFile', kind: 'function' },
};

const CRATE_EXPORTS = {
  language: { js: 'registerLanguage', kind: 'function' },
  BaselineConflict: { js: 'BaselineConflict', kind: 'type' },
  ChangeSummary: { js: 'ChangeSummary', kind: 'type' },
  CloseRequest: { js: 'CloseRequest', kind: 'type' },
  ConfigDocuments: { js: 'ConfigDocuments', kind: 'class' },
  Document: { js: 'DocumentSpec', kind: 'type' },
  DocumentsHandle: { js: 'ConfigDocuments', kind: 'class' },
  ConfigEditor: { js: 'ConfigEditor', kind: 'class' },
  ConfigEditorFooter: { js: 'showFooter', kind: 'setting' },
  ConfigEditorToolbar: { js: 'showToolbar', kind: 'setting' },
  Conversion: { js: 'Conversion', kind: 'type' },
  ConvertError: { exempt: 'thrown as an `Error` with its message' },
  Diagnostic: { js: 'Diagnostic', kind: 'type' },
  DiagnosticOrigin: { exempt: 'host problems are marked by their `source`; `validate` reports only the editor\'s own' },
  DiagnosticTarget: { js: 'HostDiagnostic', kind: 'type' },
  EditorHandle: { js: 'ConfigEditor', kind: 'class' },
  EditorSettings: { js: 'EditorSettings', kind: 'type' },
  EditorStrings: { js: 'EditorStrings', kind: 'type' },
  EditorTheme: { js: 'ThemeName', kind: 'type' },
  FieldPath: { js: 'FieldPath', kind: 'type' },
  HostDiagnostic: { js: 'HostDiagnostic', kind: 'type' },
  Language: { js: 'LanguageName', kind: 'type' },
  PRESET_CSS: { js: 'presetCss', kind: 'function' },
  ParseError: { exempt: 'thrown as an `Error` with its message and position' },
  SaveRequest: { exempt: 'its fields are `onSave`\'s arguments, `(value, language)`' },
  Schema: { js: 'schema', kind: 'option' },
  SchemaError: { exempt: 'thrown as an `Error` with its message' },
  SchemaSource: { js: 'schema', kind: 'option' },
  SecretOptions: { js: 'SecretOptions', kind: 'type' },
  StringsProblem: { exempt: 'the editor warns about each in the console, as it does in Rust' },
  Severity: { js: 'Severity', kind: 'type' },
  TextPosition: { js: 'EditorRange', kind: 'type' },
  TextRange: { js: 'EditorRange', kind: 'type' },
  ThemeColor: { js: 'colorTokens', kind: 'function' },
  ThemeColors: { js: 'ThemeColorMap', kind: 'type' },
  UnknownLanguage: { exempt: 'thrown as an `Error` with its message' },
  UnknownTheme: { exempt: 'thrown as an `Error` with its message' },
  check_syntax: { js: 'validate', kind: 'function' },
  float_of: { exempt: 'JS data holds `Infinity` and `NaN` as numbers: `parse` gives them as they are' },
  float_value: { exempt: 'JS data holds `Infinity` and `NaN` as numbers: `serialize` takes them as they are' },
  is_web_url: { exempt: 'a host diagnostic\'s `docsUrl` is checked where it is given (`setHostDiagnostics` throws on another)' },
  flush_pending_edits: { exempt: 'every JS method that reads or replaces the document flushes first' },
  flush_pending_input: { exempt: 'every JS method flushes the browser\'s pending input first' },
  unified_diff: { js: 'unifiedDiff', kind: 'function' },
};

// --- Reading the Rust side. ------------------------------------------------

/** The text of the block that opens at the first `{` at or after `from`. */
function block(source, from) {
  const open = source.indexOf('{', from);
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === '{') depth += 1;
    else if (source[index] === '}' && --depth === 0) return source.slice(open + 1, index);
  }
  throw new Error('unbalanced block');
}

/** Names of the `pub fn`s in every `impl <type> {` block of `source`. */
function implMethods(source, type) {
  const names = new Set();
  const header = new RegExp(`^impl ${type} \\{`, 'gm');
  for (const match of source.matchAll(header)) {
    for (const [, name] of block(source, match.index).matchAll(/^    pub (?:const )?fn (\w+)/gm)) names.add(name);
  }
  return [...names];
}

function componentProps(file = 'editor.rs', component = 'ConfigEditor') {
  const source = crate(file);
  const start = source.indexOf(`pub fn ${component}(`);
  const params = source.slice(start, source.indexOf(') -> impl IntoView', start));
  return [...params.matchAll(/^    (\w+): /gm)].map(([, name]) => name);
}

/** Names of the `pub` fields of `pub struct <type> {` in `source`. */
function structFields(source, type) {
  const body = block(source, source.indexOf(`pub struct ${type} {`));
  return [...body.matchAll(/^    pub (\w+): /gm)].map(([, name]) => name);
}

const camelCase = (name) => name.replace(/_(\w)/g, (_, letter) => letter.toUpperCase());

function crateExports() {
  const source = crate('lib.rs');
  const names = [];
  for (const [, list] of source.matchAll(/^pub use (?:editor|documents|settings|strings)::\{([^}]*)\};/gm)) {
    names.push(...list.split(',').map((name) => name.trim()).filter(Boolean));
  }
  for (const [, name] of source.matchAll(/^pub use (?:editor|documents|settings|strings)::(\w+);/gm)) names.push(name);
  return names;
}

const rust = {
  props: componentProps(),
  // The folding methods live with folding (an optional feature).
  handle: implMethods(crate('editor/handle.rs') + crate('editor/folding/handle.rs'), 'EditorHandle'),
  language: implMethods(crate('editor/languages/mod.rs'), 'Language'),
  changeSummary: implMethods(crate('editor/change_summary.rs') + crate('editor/changes/mod.rs'), 'ChangeSummary'),
  schema: implMethods(crate('editor/diagnostics.rs'), 'Schema'),
  theme: implMethods(crate('editor/theme.rs'), 'EditorTheme'),
  exports: crateExports(),
  documentsProps: componentProps('documents.rs', 'ConfigDocuments'),
  documentsHandle: implMethods(crate('documents/handle.rs'), 'DocumentsHandle'),
  document: implMethods(crate('documents/model.rs'), 'Document'),
  settings: structFields(crate('settings.rs'), 'EditorSettings'),
};

// --- Reading the JS package. -----------------------------------------------

/**
 * The JS names `#[wasm_bindgen]` gives the `pub fn`s in `source` (`js_name`,
 * else the Rust name). In a `#[wasm_bindgen] impl` every `pub fn` is
 * exported (`inImpl`); elsewhere only those carrying the attribute.
 */
function bindgenNames(source, inImpl) {
  const names = [];
  const lines = source.split('\n');
  lines.forEach((line, index) => {
    const name = line.match(/^ *pub fn (\w+)/)?.[1];
    if (!name) return;
    let attributes = '';
    for (let above = index - 1; above >= 0 && /^ *(#\[|\/\/\/)/.test(lines[above]); above -= 1) {
      attributes += lines[above];
    }
    if (!inImpl && !attributes.includes('wasm_bindgen')) return;
    if (attributes.includes('wasm_bindgen(constructor)')) return;
    names.push(attributes.match(/js_name = (\w+)/)?.[1] ?? name);
  });
  return names;
}

function jsPackage() {
  const classStart = jsSources.search(/^#\[wasm_bindgen\]\nimpl ConfigEditor \{/m);
  const classBody = block(jsSources, classStart);
  const documentsStart = jsSources.search(/^#\[wasm_bindgen\]\nimpl ConfigDocuments \{/m);
  const documentsBody = block(jsSources, documentsStart);
  const documentEditorStart = jsSources.search(/^#\[wasm_bindgen\]\nimpl DocumentEditor \{/m);
  const documentEditorBody = block(jsSources, documentEditorStart);
  const outside = jsSources.replace(classBody, '').replace(documentsBody, '').replace(documentEditorBody, '');
  const optionsList = jsSources.match(/const EDITOR_OPTIONS: &\[&str\] = &\[([^\]]*)\]/);
  const documentsList = jsSources.match(/const DOCUMENTS_OPTIONS: &\[&str\] = &\[([^\]]*)\]/);
  const generated = read('js', 'config-editor', 'src', 'settings.d.ts');
  const typings = [...jsSources.matchAll(/typescript_custom_section\)\]\nconst \w+: &str = r#"([\s\S]*?)"#;/g)]
    .map(([, text]) => text)
    .concat(generated)
    .join('\n');
  const optionsInterface = block(typings, typings.indexOf('export interface EditorOptions'));
  const documentsInterface = block(typings, typings.indexOf('export interface DocumentsOptions'));
  const names = (list) => new Set([...(list?.[1] ?? '').matchAll(/"(\w+)"/g)].map(([, name]) => name));
  const typed = (body) => new Set([...body.matchAll(/^  (\w+)\??:/gm)].map(([, name]) => name));
  const classNames = [...jsSources.matchAll(/^#\[wasm_bindgen\]\npub struct (\w+)/gm)].map(([, name]) => name);
  return {
    options: names(optionsList),
    typedOptions: typed(optionsInterface),
    methods: new Set(bindgenNames(classBody, true)),
    documentsOptions: names(documentsList),
    typedDocumentsOptions: typed(documentsInterface),
    documentsMethods: new Set(bindgenNames(documentsBody, true)),
    documentEditorMethods: new Set(bindgenNames(documentEditorBody, true)),
    typedSettings: typed(block(typings, typings.indexOf('export interface EditorSettings'))),
    extendsSettings: {
      options: /export interface EditorOptions extends EditorSettings \{/.test(typings),
      documents: /export interface DocumentsOptions extends EditorSettings \{/.test(typings),
    },
    functions: new Set([...bindgenNames(outside, false), ...classNames]),
    types: new Set([...typings.matchAll(/^export (?:interface|type) (\w+)/gm)].map(([, name]) => name)),
  };
}

// --- Reading the React package. --------------------------------------------

function members(source, name) {
  const body = block(source, source.indexOf(`export interface ${name}`));
  return new Set([...body.matchAll(/^  (?:readonly )?(\w+)\??[:(]/gm)].map(([, member]) => member));
}

function reactPackage() {
  const exported = new Set();
  for (const [, list] of react.matchAll(/^export (?:type )?\{([^}]*)\} from "@pngwasi\/config-editor";/gm)) {
    for (const name of list.split(',')) if (name.trim()) exported.add(name.trim());
  }
  for (const [, name] of react.matchAll(/^export (?:const|function|interface|type) (\w+)/gm)) exported.add(name);
  const forwardedIn = (type) =>
    new Set([...block(react, react.indexOf(`const options: ${type} = {`)).matchAll(/^ {10}(\w+)[:,]/gm)].map(([, name]) => name));
  const spreads = (type) => block(react, react.indexOf(`const options: ${type} = {`)).includes('...settings,');
  return {
    extendsSettings: {
      options: /export interface ConfigEditorProps extends EditorSettings \{/.test(react) && spreads('EditorOptions'),
      documents: /export interface ConfigDocumentsProps extends EditorSettings \{/.test(react) && spreads('DocumentsOptions'),
    },
    props: members(react, 'ConfigEditorProps'),
    handle: members(react, 'ConfigEditorHandle'),
    exported,
    forwarded: forwardedIn('EditorOptions'),
    documentsProps: members(react, 'ConfigDocumentsProps'),
    documentsHandle: members(react, 'ConfigDocumentsHandle'),
    documentsForwarded: forwardedIn('DocumentsOptions'),
  };
}

// --- The checks. -----------------------------------------------------------

const js = jsPackage();
const reactSide = reactPackage();

// Where options and methods are looked up: the editor's, or the documents
// view's.
const EDITOR_SIDE = {
  options: () => js.options,
  typedOptions: () => js.typedOptions,
  methods: () => js.methods,
  props: () => reactSide.props,
  handle: () => reactSide.handle,
  extends: () => js.extendsSettings.options && reactSide.extendsSettings.options,
  optionsWhere: 'EDITOR_OPTIONS and the EditorOptions typings',
};
const DOCUMENTS_SIDE = {
  options: () => js.documentsOptions,
  typedOptions: () => js.typedDocumentsOptions,
  methods: () => js.documentsMethods,
  props: () => reactSide.documentsProps,
  handle: () => reactSide.documentsHandle,
  extends: () => js.extendsSettings.documents && reactSide.extendsSettings.documents,
  optionsWhere: 'DOCUMENTS_OPTIONS and the DocumentsOptions typings',
};

function check(table, items, what, kindOf, side = EDITOR_SIDE) {
  const problems = [];
  for (const item of items) {
    const entry = table[item];
    if (!entry) {
      problems.push(`${what} \`${item}\` has no JS counterpart: map it in api-parity.spec.mjs or exempt it with a reason`);
      continue;
    }
    if (entry.exempt) continue;
    const kind = entry.kind ?? kindOf;
    if (kind === 'setting') {
      if (!js.typedSettings.has(entry.js)) problems.push(`${what} \`${item}\`: setting \`${entry.js}\` is missing from EditorSettings`);
      if (!reactSide.extendsSettings.options) problems.push(`${what} \`${item}\`: the React props must extend \`EditorSettings\``);
      continue;
    }
    if (kind === 'extends') {
      if (!side.extends()) {
        problems.push(`${what} \`${item}\`: the JS options and the React props must extend \`${entry.js}\` (and the React component spread \`...settings\` into its options)`);
      }
      if (!js.types.has(entry.js)) problems.push(`${what} \`${item}\`: type \`${entry.js}\` is missing from the typings`);
      continue;
    }
    const present = {
      option: side.options().has(entry.js) && side.typedOptions().has(entry.js),
      method: side.methods().has(entry.js),
      function: js.functions.has(entry.js),
      class: js.functions.has(entry.js),
      type: js.types.has(entry.js),
    }[kind];
    if (!present) {
      const where = kind === 'option' ? side.optionsWhere : kind === 'type' ? 'the TypeScript typings' : 'the wasm_bindgen exports';
      problems.push(`${what} \`${item}\`: JS ${kind} \`${entry.js}\` is missing from ${where} of js/config-editor/src`);
    }
    if (entry.react === null) {
      if (!entry.reactWhy) problems.push(`${what} \`${item}\`: \`react: null\` needs a \`reactWhy\``);
      continue;
    }
    const reactName = entry.react ?? entry.js;
    const inReact = {
      option: side.props().has(reactName),
      method: side.handle().has(reactName),
      function: reactSide.exported.has(reactName),
      class: reactSide.exported.has(reactName),
      type: reactSide.exported.has(reactName),
    }[kind];
    if (!inReact) {
      const where = { option: 'prop', method: 'ref member' }[kind] ?? 'export';
      problems.push(`${what} \`${item}\`: React ${where} \`${reactName}\` is missing from js/config-editor-react/src/index.tsx`);
    }
  }
  for (const item of Object.keys(table)) {
    if (!items.includes(item)) problems.push(`${what} \`${item}\` is in the mapping but no longer in the Rust API: remove it`);
  }
  return problems;
}

const groups = [
  ['ConfigEditor prop', PROPS, rust.props, 'option'],
  ['EditorHandle method', HANDLE, rust.handle, 'method'],
  ['Language method', LANGUAGE, rust.language, 'function'],
  ['ChangeSummary method', CHANGE_SUMMARY, rust.changeSummary, 'function'],
  ['Schema method', SCHEMA, rust.schema, 'function'],
  ['EditorTheme method', THEME, rust.theme, 'function'],
  ['crate export', CRATE_EXPORTS, rust.exports, 'type'],
  ['ConfigDocuments prop', DOCUMENTS_PROPS, rust.documentsProps, 'option', DOCUMENTS_SIDE],
  ['DocumentsHandle method', DOCUMENTS_HANDLE, rust.documentsHandle, 'method', DOCUMENTS_SIDE],
  ['Document method', DOCUMENT, rust.document, 'type'],
];

test.describe('API parity between the Rust crate and the JS packages', () => {
  test('every EditorSettings field is in the generated TypeScript type', () => {
    expect(rust.settings).toContain('folding');
    const missing = rust.settings.map(camelCase).filter((name) => !js.typedSettings.has(name));
    const extra = [...js.typedSettings].filter((name) => !rust.settings.map(camelCase).includes(name));
    expect({ missing, extra }).toEqual({ missing: [], extra: [] });
  });

  test('a document\'s editor (DocumentEditor) answers what a ConfigEditor does about its text', () => {
    const problems = Object.entries(HANDLE)
      .filter(([, entry]) => !entry.exempt)
      .map(([, entry]) => entry.js)
      .filter((name) => !js.documentEditorMethods.has(name))
      .map((name) => `DocumentEditor has no \`${name}\``);
    expect(problems).toEqual([]);
  });

  test('the Rust side was read', () => {
    // A parser that silently finds nothing would pass everything.
    expect(rust.props).toContain('settings');
    expect(rust.handle).toContain('reveal');
    expect(rust.language).toContain('convert');
    expect(rust.changeSummary).toContain('between');
    expect(rust.schema).toContain('validate');
    expect(rust.theme).toContain('label');
    expect(rust.exports).toContain('unified_diff');
    expect(js.options.has('readOnly')).toBe(true);
    expect(js.methods.has('reveal')).toBe(true);
    expect(js.functions.has('validate')).toBe(true);
    expect(js.types.has('EditorOptions')).toBe(true);
    expect(reactSide.props.has('readOnly')).toBe(true);
    expect(reactSide.handle.has('reveal')).toBe(true);
    expect(reactSide.exported.has('validate')).toBe(true);
  });

  for (const [what, table, items, kind, side] of groups) {
    test(`every ${what} is mapped to JS and React, or exempted`, () => {
      expect(check(table, items, what, kind, side)).toEqual([]);
    });
  }

  test('the documents view was read', () => {
    expect(rust.documentsProps).toContain('on_close');
    expect(rust.documentsHandle).toContain('save_all');
    expect(rust.document).toContain('read_only');
    expect(js.documentsOptions.has('onClose')).toBe(true);
    expect(js.documentsMethods.has('saveAll')).toBe(true);
    expect(reactSide.documentsProps.has('onClose')).toBe(true);
    expect(reactSide.documentsHandle.has('saveAll')).toBe(true);
  });

  test('the documents options: list, typings, React props and forwarding agree', () => {
    const toReact = new Map(
      Object.values(DOCUMENTS_PROPS)
        .filter((entry) => !entry.exempt)
        .map((entry) => [entry.js, entry.react ?? entry.js]),
    );
    const problems = [
      ...[...js.documentsOptions].filter((name) => !js.typedDocumentsOptions.has(name)).map((name) => `option \`${name}\` is in DOCUMENTS_OPTIONS but not typed in DocumentsOptions`),
      ...[...js.typedDocumentsOptions].filter((name) => !js.documentsOptions.has(name)).map((name) => `option \`${name}\` is typed in DocumentsOptions but not in DOCUMENTS_OPTIONS`),
    ];
    for (const option of js.documentsOptions) {
      const prop = toReact.get(option);
      if (!prop) problems.push(`JS option \`${option}\` has no Rust prop in the mapping (DOCUMENTS_PROPS)`);
      else if (!reactSide.documentsProps.has(prop)) problems.push(`JS option \`${option}\` has no React prop \`${prop}\``);
      if (!reactSide.documentsForwarded.has(option)) problems.push(`JS option \`${option}\` is not passed by the React component's \`options\``);
    }
    const mapped = new Set(toReact.values());
    for (const prop of reactSide.documentsProps) {
      if (!mapped.has(prop) && !(prop in DOCUMENTS_REACT_ONLY_PROPS)) {
        problems.push(`React prop \`${prop}\` maps to no documents option: add it to DOCUMENTS_PROPS or DOCUMENTS_REACT_ONLY_PROPS`);
      }
    }
    const jsOnly = {
      destroy: 'the component unmounts the view itself',
      setDocuments: 'the `documents` prop',
      settings: 'the setting props',
      setSettings: 'the setting props',
      setCallbacks: 'callback props',
    };
    for (const name of js.documentsMethods) {
      if (!reactSide.documentsHandle.has(name) && !(name in jsOnly)) problems.push(`JS method \`${name}\` has no React ref member`);
    }
    expect(problems).toEqual([]);
  });

  test('the option list and the EditorOptions typings name the same options', () => {
    const problems = [
      ...[...js.options].filter((name) => !js.typedOptions.has(name)).map((name) => `option \`${name}\` is in EDITOR_OPTIONS but not typed in EditorOptions`),
      ...[...js.typedOptions].filter((name) => !js.options.has(name)).map((name) => `option \`${name}\` is typed in EditorOptions but not in EDITOR_OPTIONS`),
    ];
    expect(problems).toEqual([]);
  });

  test('every JS option has a React prop that reaches the editor, and every React prop maps to an option', () => {
    const toReact = new Map(
      Object.values(PROPS)
        .filter((entry) => !entry.exempt && entry.react !== null)
        .map((entry) => [entry.js, entry.react ?? entry.js]),
    );
    const problems = [];
    for (const option of js.options) {
      const prop = toReact.get(option);
      if (!prop) problems.push(`JS option \`${option}\` has no Rust prop in the mapping (PROPS)`);
      else if (!reactSide.props.has(prop)) problems.push(`JS option \`${option}\` has no React prop \`${prop}\``);
      if (!reactSide.forwarded.has(option)) problems.push(`JS option \`${option}\` is not passed by the React component's \`options\``);
    }
    const mapped = new Set(toReact.values());
    for (const prop of reactSide.props) {
      if (!mapped.has(prop) && !(prop in REACT_ONLY_PROPS)) {
        problems.push(`React prop \`${prop}\` maps to no JS option: add it to PROPS or REACT_ONLY_PROPS`);
      }
    }
    expect(problems).toEqual([]);
  });

  test('every JS instance member has a React ref member', () => {
    const jsOnly = {
      destroy: 'the component unmounts the editor itself',
      setValue: 'a ref member too, with React semantics',
      setBaseline: 'the `baseline` prop',
      setEditAs: 'the `editAs` prop',
      settings: 'the setting props',
      setSettings: 'the setting props',
      setReadOnly: 'the `readOnly` prop',
      setCallbacks: 'callback props',
      setHostDiagnostics: 'the `hostDiagnostics` prop',
      setReadOnlyPaths: 'the `readOnlyPaths` prop',
      setReadOnlyRanges: 'the `readOnlyRanges` prop',
    };
    const problems = [...js.methods]
      .filter((name) => !reactSide.handle.has(name) && !(name in jsOnly))
      .map((name) => `JS method \`${name}\` has no React ref member`);
    expect(problems).toEqual([]);
  });

  test('every JS function and public type is re-exported by the React package', () => {
    const problems = [...js.functions, ...js.types]
      .filter((name) => !['ConfigEditor', 'EditorOptions', 'CallbackUpdate'].includes(name))
      .filter((name) => !reactSide.exported.has(name))
      .map((name) => `\`${name}\` is not re-exported by js/config-editor-react/src/index.tsx`);
    expect(problems).toEqual([]);
  });
});
