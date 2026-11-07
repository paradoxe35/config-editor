import { expect, test } from '@playwright/test';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The Rust API and the JS packages stay in step: every public prop of
// `ConfigEditor`, method of `EditorHandle`, `Language`, `ChangeSummary`,
// `Schema` and `EditorTheme`, and item re-exported from the crate root has a
// JavaScript counterpart (an option, a method, a function or a type) and a
// React one (a prop, a ref member or a re-export), or an exemption saying
// why not. Reads the sources only: no build, no browser.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (...path) => readFileSync(join(root, ...path), 'utf8');
const crate = (path) => read('crates', 'config-editor-leptos', 'src', ...path.split('/'));
const jsSources = readdirSync(join(root, 'js', 'config-editor', 'src'))
  .filter((name) => name.endsWith('.rs'))
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
  theme: { js: 'theme' },
  colors: { js: 'colors' },
  on_change: { js: 'onChange' },
  on_edit_as_change: { js: 'onEditAsChange' },
  on_change_summary: { js: 'onChangeSummary' },
  on_schema_warning: { js: 'onSchemaWarning' },
  styled: { js: 'styled' },
  show_toolbar: { js: 'showToolbar' },
  show_footer: { js: 'showFooter' },
  show_diagnostics: { js: 'showDiagnostics' },
  enable_completions: { js: 'completions' },
  enable_hover_docs: { js: 'hoverDocs' },
  auto_trigger_completions: { js: 'autoTriggerCompletions' },
  enable_highlighting: { js: 'highlighting' },
  enable_auto_closing: { js: 'autoClosing' },
  show_diff: { js: 'showDiff' },
  enable_search: { js: 'search' },
  enable_go_to_line: { js: 'goToLine' },
  enable_folding: { js: 'folding' },
  read_only: { js: 'readOnly' },
  handle: { exempt: 'the `ConfigEditor` instance (React: its ref) is the handle' },
  host_diagnostics: { js: 'hostDiagnostics' },
  on_path_change: { js: 'onPathChange' },
  on_save: { js: 'onSave' },
  enable_save_shortcut: { js: 'saveShortcut' },
  on_dirty_change: { js: 'onDirtyChange' },
  on_baseline_conflict: { js: 'onBaselineConflict' },
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

const CRATE_EXPORTS = {
  language: { js: 'registerLanguage', kind: 'function' },
  BaselineConflict: { js: 'BaselineConflict', kind: 'type' },
  ChangeSummary: { js: 'ChangeSummary', kind: 'type' },
  ConfigEditor: { js: 'ConfigEditor', kind: 'class' },
  ConfigEditorFooter: { js: 'showFooter', kind: 'option' },
  ConfigEditorToolbar: { js: 'showToolbar', kind: 'option' },
  Conversion: { js: 'Conversion', kind: 'type' },
  ConvertError: { exempt: 'thrown as an `Error` with its message' },
  Diagnostic: { js: 'Diagnostic', kind: 'type' },
  DiagnosticOrigin: { exempt: 'host problems are marked by their `source`; `validate` reports only the editor\'s own' },
  DiagnosticTarget: { js: 'HostDiagnostic', kind: 'type' },
  EditorHandle: { js: 'ConfigEditor', kind: 'class' },
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
  Severity: { js: 'Severity', kind: 'type' },
  TextPosition: { js: 'EditorRange', kind: 'type' },
  TextRange: { js: 'EditorRange', kind: 'type' },
  ThemeColor: { js: 'colorTokens', kind: 'function' },
  ThemeColors: { js: 'ThemeColorMap', kind: 'type' },
  UnknownLanguage: { exempt: 'thrown as an `Error` with its message' },
  UnknownTheme: { exempt: 'thrown as an `Error` with its message' },
  check_syntax: { js: 'validate', kind: 'function' },
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

function componentProps() {
  const source = crate('editor.rs');
  const start = source.indexOf('pub fn ConfigEditor(');
  const params = source.slice(start, source.indexOf(') -> impl IntoView', start));
  return [...params.matchAll(/^    (\w+): /gm)].map(([, name]) => name);
}

function crateExports() {
  const source = crate('lib.rs');
  const names = [];
  for (const [, list] of source.matchAll(/^pub use editor::\{([^}]*)\};/gm)) {
    names.push(...list.split(',').map((name) => name.trim()).filter(Boolean));
  }
  for (const [, name] of source.matchAll(/^pub use editor::(\w+);/gm)) names.push(name);
  return names;
}

const rust = {
  props: componentProps(),
  handle: implMethods(crate('editor/handle.rs'), 'EditorHandle'),
  language: implMethods(crate('editor/languages/mod.rs'), 'Language'),
  changeSummary: implMethods(crate('editor/changes/mod.rs'), 'ChangeSummary'),
  schema: implMethods(crate('editor/diagnostics.rs'), 'Schema'),
  theme: implMethods(crate('editor/theme.rs'), 'EditorTheme'),
  exports: crateExports(),
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
  const outside = jsSources.slice(0, classStart) + jsSources.slice(classStart + classBody.length);
  const optionsList = jsSources.match(/const OPTIONS: \[&str; \d+\] = \[([^\]]*)\]/);
  const typings = [...jsSources.matchAll(/typescript_custom_section\)\]\nconst \w+: &str = r#"([\s\S]*?)"#;/g)]
    .map(([, text]) => text)
    .join('\n');
  const optionsInterface = block(typings, typings.indexOf('export interface EditorOptions'));
  const classNames = [...jsSources.matchAll(/^#\[wasm_bindgen\]\npub struct (\w+)/gm)].map(([, name]) => name);
  return {
    options: new Set([...(optionsList?.[1] ?? '').matchAll(/"(\w+)"/g)].map(([, name]) => name)),
    typedOptions: new Set([...optionsInterface.matchAll(/^  (\w+)\??:/gm)].map(([, name]) => name)),
    methods: new Set(bindgenNames(classBody, true)),
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
  const optionsStart = react.indexOf('const options: EditorOptions = {');
  const forwarded = new Set([...block(react, optionsStart).matchAll(/^ {10}(\w+)[:,]/gm)].map(([, name]) => name));
  return { props: members(react, 'ConfigEditorProps'), handle: members(react, 'ConfigEditorHandle'), exported, forwarded };
}

// --- The checks. -----------------------------------------------------------

const js = jsPackage();
const reactSide = reactPackage();

function check(table, items, what, kindOf) {
  const problems = [];
  for (const item of items) {
    const entry = table[item];
    if (!entry) {
      problems.push(`${what} \`${item}\` has no JS counterpart: map it in api-parity.spec.mjs or exempt it with a reason`);
      continue;
    }
    if (entry.exempt) continue;
    const kind = entry.kind ?? kindOf;
    const present = {
      option: js.options.has(entry.js) && js.typedOptions.has(entry.js),
      method: js.methods.has(entry.js),
      function: js.functions.has(entry.js),
      class: js.functions.has(entry.js),
      type: js.types.has(entry.js),
    }[kind];
    if (!present) {
      const where = kind === 'option' ? 'OPTIONS and the EditorOptions typings' : kind === 'type' ? 'the TypeScript typings' : 'the wasm_bindgen exports';
      problems.push(`${what} \`${item}\`: JS ${kind} \`${entry.js}\` is missing from ${where} of js/config-editor/src`);
    }
    if (entry.react === null) {
      if (!entry.reactWhy) problems.push(`${what} \`${item}\`: \`react: null\` needs a \`reactWhy\``);
      continue;
    }
    const reactName = entry.react ?? entry.js;
    const inReact = {
      option: reactSide.props.has(reactName),
      method: reactSide.handle.has(reactName),
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
];

test.describe('API parity between the Rust crate and the JS packages', () => {
  test('the Rust side was read', () => {
    // A parser that silently finds nothing would pass everything.
    expect(rust.props).toContain('enable_folding');
    expect(rust.handle).toContain('reveal');
    expect(rust.language).toContain('convert');
    expect(rust.changeSummary).toContain('between');
    expect(rust.schema).toContain('validate');
    expect(rust.theme).toContain('label');
    expect(rust.exports).toContain('unified_diff');
    expect(js.options.has('folding')).toBe(true);
    expect(js.methods.has('reveal')).toBe(true);
    expect(js.functions.has('validate')).toBe(true);
    expect(js.types.has('EditorOptions')).toBe(true);
    expect(reactSide.props.has('folding')).toBe(true);
    expect(reactSide.handle.has('reveal')).toBe(true);
    expect(reactSide.exported.has('validate')).toBe(true);
  });

  for (const [what, table, items, kind] of groups) {
    test(`every ${what} is mapped to JS and React, or exempted`, () => {
      expect(check(table, items, what, kind)).toEqual([]);
    });
  }

  test('the option list and the EditorOptions typings name the same options', () => {
    const problems = [
      ...[...js.options].filter((name) => !js.typedOptions.has(name)).map((name) => `option \`${name}\` is in OPTIONS but not typed in EditorOptions`),
      ...[...js.typedOptions].filter((name) => !js.options.has(name)).map((name) => `option \`${name}\` is typed in EditorOptions but not in OPTIONS`),
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
      setTheme: 'the `theme` prop',
      setColors: 'the `colors` prop',
      setReadOnly: 'the `readOnly` prop',
      setSaveShortcut: 'the `saveShortcut` prop',
      setCallbacks: 'callback props',
      setHostDiagnostics: 'the `hostDiagnostics` prop',
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
