import { expect, test } from '@playwright/test';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Every feature of the Rust crate is reachable from JavaScript and from
// each framework package (React, Vue, Svelte). Each public prop of
// `ConfigEditor` and `ConfigDocuments`, method of `EditorHandle`,
// `DocumentsHandle`, `Document`, `Language`, `ChangeSummary`, `Schema` and
// `EditorTheme`, and item re-exported from the crate root is mapped below to
// the JS member that reaches it, or exempted with the reason it is Rust-only:
//
// - `option`: a key of the constructor's options (`EditorOptions`,
//   `DocumentsOptions`), which a binding prop passes and every component
//   has as a prop (under its framework's name). Each option either changes
//   live (`update`: a key of `EditorUpdate` / `DocumentsUpdate`) or is one
//   the instance is built from (`fixed`, with the reason: `update` refuses
//   it and the components re-mount for it).
// - `member`: an instance member of `ConfigEditor` (and of `EditorView`,
//   which a document's editor implements), or of `ConfigDocuments`; each is
//   a member of the binding's handle and of every component's instance API.
// - `main` / `tool`: a function of `@pngwasi/config-editor` (index.js) or
//   of `@pngwasi/config-editor/tools` (tools.js). The framework packages
//   re-export none: their apps import them from `@pngwasi/config-editor`, a
//   peer dependency of each.
// - `type`: a TypeScript type of the package; `setting`: a key of
//   `EditorSettings`; `extends`: the options (and `update`) take every
//   setting.
//
// `EditorSettings` is not mapped by hand: its TypeScript type is generated
// from the struct (js/config-editor/tests/settings.rs checks it is current);
// here every field must be in it. The components are thin wrappers around
// the bindings of `@pngwasi/config-editor/binding`: every option reaches the
// editor through the binding's props, every binding prop is a prop of each
// component, and every type a component's props or handle take is exported
// by its package. The last test proves the checks fail when a counterpart is
// missing. Reads the sources only: no build, no browser.

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (...path) => readFileSync(join(root, ...path), 'utf8');
const crate = (path) => read('crates', 'config-editor-leptos', 'src', ...path.split('/'));
// `alloc_stats.rs` is a debugging aid behind a cargo feature, never published.
const jsSources = readdirSync(join(root, 'js', 'config-editor', 'src'))
  .filter((name) => name.endsWith('.rs') && name !== 'alloc_stats.rs')
  .map((name) => read('js', 'config-editor', 'src', name))
  .join('\n');
const bindingJs = read('js', 'config-editor', 'binding.js');
const bindingTypes = read('js', 'config-editor', 'binding.d.ts');

// --- The mapping: Rust name → JS member, or an exemption. -------------------

const PROPS = {
  language: { option: 'language', fixed: 'the editor is built for its language' },
  edit_as: { option: 'editAs', update: 'editAs' },
  initial_value: { exempt: '`value` is both the initial and the live document in JS; the components have `defaultValue`' },
  value: { option: 'value', update: 'value' },
  baseline: { option: 'baseline', update: 'baseline' },
  schema: { option: 'schema', fixed: 'the schema is compiled once, when the editor is built' },
  file_name: { option: 'fileName', fixed: 'a toolbar label, read once' },
  class: { option: 'className', prop: 'editorClassName', fixed: 'a class of the editor\'s root, read once' },
  settings: { extends: 'EditorSettings' },
  on_change: { option: 'onChange', update: 'onChange' },
  on_edit_as_change: { option: 'onEditAsChange', update: 'onEditAsChange' },
  on_change_summary: { option: 'onChangeSummary', update: 'onChangeSummary' },
  on_schema_warning: { option: 'onSchemaWarning', update: 'onSchemaWarning' },
  read_only: { option: 'readOnly', update: 'readOnly' },
  handle: { exempt: 'the `ConfigEditor` instance (a component: its instance API) is the handle' },
  host_diagnostics: { option: 'hostDiagnostics', update: 'hostDiagnostics' },
  on_path_change: { option: 'onPathChange', update: 'onPathChange' },
  on_save: { option: 'onSave', update: 'onSave' },
  on_dirty_change: { option: 'onDirtyChange', update: 'onDirtyChange' },
  on_baseline_conflict: { option: 'onBaselineConflict', update: 'onBaselineConflict' },
  read_only_paths: { option: 'readOnlyPaths', update: 'readOnlyPaths' },
  read_only_ranges: { option: 'readOnlyRanges', update: 'readOnlyRanges' },
  secrets: { option: 'secrets', fixed: 'the placeholder patterns build the editor (`isPlaceholder` is called live)' },
};

// Binding props (so every component's) with no JS option of their own.
const BINDING_ONLY_PROPS = {
  defaultValue: 'the uncontrolled initial document (JS: `value`)',
  onError: 'mount and prop errors, which JS throws instead',
};

const HANDLE = {
  new: { exempt: '`new ConfigEditor(element, options)` mounts and returns the instance' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (a component: its `editor`)' },
  text: { member: 'text' },
  value: { member: 'value' },
  unified_diff: { member: 'unifiedDiff' },
  range_of: { member: 'rangeOf' },
  path_at: { member: 'pathAt' },
  current_path: { member: 'currentPath' },
  reveal: { member: 'reveal' },
  save: { member: 'save' },
  is_dirty: { member: 'isDirty' },
  fold: { member: 'fold' },
  unfold: { member: 'unfold' },
  fold_all: { member: 'foldAll' },
  unfold_all: { member: 'unfoldAll' },
  folded_lines: { member: 'foldedLines' },
  focus: { member: 'focus' },
};

const LANGUAGE = {
  new: { main: 'registerLanguage' },
  info: { main: 'languages' },
  label: { main: 'languages' },
  file_icon: { main: 'languages' },
  extensions: { main: 'languages' },
  default_file_name: { main: 'languages' },
  from_path: { main: 'languageForFile' },
  parse: { tool: 'parse' },
  serialize: { tool: 'serialize' },
  convert: { tool: 'convert' },
  range_of: { tool: 'rangeOf' },
  fold_ranges: { exempt: 'the editor folds by itself (`fold`, `foldAll`, `foldedLines`); a custom language describes its folds in Rust' },
  path_at: { tool: 'pathAt' },
};

const CHANGE_SUMMARY = {
  between: { tool: 'changeSummary' },
  is_empty: { exempt: 'a summary is a plain object in JS: `summary.hunks === 0`' },
};

const SCHEMA = {
  compile: { exempt: 'JS takes the schema as JSON text wherever one is used, and throws there when it is invalid' },
  warnings: { tool: 'schemaWarnings' },
  validate: { tool: 'validate' },
  convert: { tool: 'convert' },
};

const THEME = {
  as_str: { main: 'themes' },
  label: { main: 'themes' },
  css_family: { exempt: 'a stylesheet detail, shown as the root\'s `data-theme` attribute (`themes()` gives each theme\'s `dark` from it)' },
};

const DOCUMENTS_PROPS = {
  documents: { option: 'documents', update: 'documents' },
  initial_active: { option: 'activeId', update: 'activeId' },
  on_select: { option: 'onSelect', update: 'onSelect' },
  on_close: { option: 'onClose', update: 'onClose' },
  on_change: { option: 'onChange', update: 'onChange' },
  on_save: { option: 'onSave', update: 'onSave' },
  on_dirty_change: { option: 'onDirtyChange', update: 'onDirtyChange' },
  settings: { extends: 'EditorSettings' },
  class: { option: 'className', prop: 'viewClassName', fixed: 'a class of the view\'s root, read once' },
  label: { option: 'label', fixed: 'the tab list\'s name, read once' },
  handle: { exempt: 'the `ConfigDocuments` instance (a component: its instance API) is the handle' },
};

const DOCUMENTS_BINDING_ONLY_PROPS = {
  onError: 'mount and prop errors, which JS throws instead',
};

const DOCUMENTS_HANDLE = {
  new: { exempt: '`new ConfigDocuments(element, options)` mounts and returns the instance' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (a component: its `instance`)' },
  active: { member: 'activeId' },
  select: { member: 'select' },
  show: { option: 'activeId', update: 'activeId' },
  editor: { member: 'editor' },
  active_editor: { exempt: '`editor(activeId)`' },
  is_dirty: { member: 'isDirty' },
  save: { member: 'save' },
  save_all: { member: 'saveAll' },
  focus: { member: 'focus' },
};

// `Document`'s builder methods are the fields of a `DocumentSpec`.
const DOCUMENT = {
  new: { type: 'DocumentSpec' },
  language: { type: 'DocumentSpec' },
  baseline: { type: 'DocumentSpec' },
  read_only: { type: 'DocumentSpec' },
  schema: { type: 'DocumentSpec' },
  edit_as: { type: 'DocumentSpec' },
  settings: { type: 'DocumentSpec' },
  host_diagnostics: { type: 'DocumentSpec' },
  read_only_paths: { type: 'DocumentSpec' },
  read_only_ranges: { type: 'DocumentSpec' },
  secrets: { type: 'DocumentSpec' },
  resolved_language: { main: 'languageForFile' },
};

const CRATE_EXPORTS = {
  language: { main: 'registerLanguage' },
  BaselineConflict: { type: 'BaselineConflict' },
  ChangeSummary: { type: 'ChangeSummary' },
  CloseRequest: { type: 'CloseRequest' },
  ConfigDocuments: { main: 'ConfigDocuments' },
  Document: { type: 'DocumentSpec' },
  DocumentsHandle: { main: 'ConfigDocuments' },
  ConfigEditor: { main: 'ConfigEditor' },
  ConfigEditorFooter: { setting: 'showFooter' },
  ConfigEditorToolbar: { setting: 'showToolbar' },
  Conversion: { type: 'Conversion' },
  ConvertError: { exempt: 'thrown as an `Error` with its message' },
  Diagnostic: { type: 'Diagnostic' },
  DiagnosticOrigin: { exempt: 'host problems are marked by their `source`; `validate` reports only the editor\'s own' },
  DiagnosticTarget: { type: 'HostDiagnostic' },
  EditorHandle: { main: 'ConfigEditor' },
  EditorSettings: { type: 'EditorSettings' },
  EditorStrings: { type: 'EditorStrings' },
  EditorTheme: { type: 'ThemeName' },
  FieldPath: { type: 'FieldPath' },
  HostDiagnostic: { type: 'HostDiagnostic' },
  Language: { type: 'LanguageName' },
  PRESET_CSS: { tool: 'presetCss' },
  ParseError: { exempt: 'thrown as an `Error` with its message and position' },
  SaveRequest: { exempt: 'its fields are `onSave`\'s arguments, `(value, language)`' },
  Schema: { option: 'schema', fixed: 'the schema is compiled once, when the editor is built' },
  SchemaError: { exempt: 'thrown as an `Error` with its message' },
  SchemaSource: { option: 'schema', fixed: 'the schema is compiled once, when the editor is built' },
  SecretOptions: { type: 'SecretOptions' },
  StringsProblem: { exempt: 'the editor warns about each in the console, as it does in Rust' },
  Severity: { type: 'Severity' },
  TextPosition: { type: 'EditorRange' },
  TextRange: { type: 'EditorRange' },
  ThemeColor: { type: 'ThemeColorToken' },
  ThemeColors: { type: 'ThemeColorMap' },
  UnknownLanguage: { exempt: 'thrown as an `Error` with its message' },
  UnknownTheme: { exempt: 'thrown as an `Error` with its message' },
  check_syntax: { tool: 'validate' },
  float_of: { exempt: 'JS data holds `Infinity` and `NaN` as numbers: `parse` gives them as they are' },
  float_value: { exempt: 'JS data holds `Infinity` and `NaN` as numbers: `serialize` takes them as they are' },
  string_of: { exempt: 'JS strings are plain strings: `parse` gives them as written' },
  string_value: { exempt: 'JS strings are plain strings: `serialize` takes them as written' },
  flush_pending_edits: { exempt: 'every JS member that reads or replaces the document flushes first' },
  unified_diff: { tool: 'unifiedDiff' },
};

// Instance members of `ConfigEditor` / `ConfigDocuments` that a component
// does not expose, and why.
const EDITOR_JS_ONLY = {
  update: 'the props: a component applies each change itself',
  destroy: 'the component unmounts the editor itself',
  settings: 'the setting props',
};
const DOCUMENTS_JS_ONLY = {
  update: 'the props: a component applies each change itself',
  destroy: 'the component unmounts the view itself',
  settings: 'the setting props',
};

// Instance members (of the classes, handles and components) that no Rust
// method maps to, and why.
const EDITOR_MEMBERS_JS_ONLY = {
  editor: 'a component\'s way to the `ConfigEditor` instance it mounted (Rust: the `handle` prop is that instance)',
  setValue: 'a component\'s controlled value: the `value` prop in Rust, which a signal sets',
};
const DOCUMENTS_MEMBERS_JS_ONLY = {
  instance: 'a component\'s way to the `ConfigDocuments` instance it mounted (Rust: the `handle` prop is that instance)',
};

// Exports of the entries that no Rust item maps to.
const MAIN_ONLY = { loadConfigEditor: 'loads the WebAssembly module (Rust compiles it in)' };
const TOOLS_ONLY = { loadConfigEditor: 'the same load, for code that imports only the tools' };

// --- Reading the Rust side. --------------------------------------------------

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

/** The crate root's re-exports (the doc-hidden `js_support` module is not the API). */
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

// --- Reading the JS package. ---------------------------------------------------

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

/** The names an entry file lists in its `export { … } from "…";` statements. */
function entryExports(source) {
  const names = new Set();
  for (const [, list] of source.matchAll(/^export \{([^}]*)\} from "[^"]+";/gm)) {
    for (const item of list.split(',')) if (item.trim()) names.add(item.trim().split(/\s+/).at(-1));
  }
  return names;
}

/** Members of `export interface <name>` in `source` (quoted names unquoted). */
function members(source, name) {
  const body = block(source, source.search(new RegExp(`export interface ${name}\\b`)));
  return new Set([...body.matchAll(/^  (?:readonly )?(\w+|"[^"]+")\??[:(]/gm)].map(([, member]) => member.replaceAll('"', '')));
}

/** The members of `export interface <name>` typed `never`: keys it refuses. */
function refused(source, name) {
  const body = block(source, source.search(new RegExp(`export interface ${name}\\b`)));
  return new Set([...body.matchAll(/^  (\w+)\?: never;/gm)].map(([, member]) => member));
}

/** The `readonly` members (getters) of `export interface <name>`. */
function getters(source, name) {
  const body = block(source, source.search(new RegExp(`export interface ${name}\\b`)));
  return new Set([...body.matchAll(/^  readonly (\w+)\??:/gm)].map(([, member]) => member));
}

/** The JS crate's sources without the bodies of its `#[wasm_bindgen] impl` blocks. */
function outsideClasses() {
  let outside = jsSources;
  for (const match of jsSources.matchAll(/^#\[wasm_bindgen\]\nimpl \w+ \{/gm)) outside = outside.replace(block(jsSources, match.index), '');
  return outside;
}

function jsPackage() {
  const impl = (type) => block(jsSources, jsSources.search(new RegExp(`^#\\[wasm_bindgen\\]\\nimpl ${type} \\{`, 'm')));
  const list = (name) => new Set([...(jsSources.match(new RegExp(`const ${name}: &\\[&str\\] =\\s*&\\[([^\\]]*)\\]`))?.[1] ?? '').matchAll(/"(\w+)"/g)].map(([, key]) => key));
  const generated = read('js', 'config-editor', 'src', 'settings.d.ts');
  const typings = [...jsSources.matchAll(/typescript_custom_section\)\]\nconst \w+: &str = r#"([\s\S]*?)"#;/g)]
    .map(([, text]) => text)
    .concat(generated)
    .join('\n');
  const typed = (name) => new Set([...block(typings, typings.indexOf(`export interface ${name}`)).matchAll(/^  (\w+)\??:/gm)].map(([, key]) => key));
  const live = (name) => new Set([...typed(name)].filter((key) => !refused(typings, name).has(key)));
  return {
    typings,
    options: list('EDITOR_OPTIONS'),
    remount: list('EDITOR_REMOUNT_OPTIONS'),
    typedOptions: typed('EditorOptions'),
    updateKeys: live('EditorUpdate'),
    refusedKeys: refused(typings, 'EditorUpdate'),
    members: new Set(bindgenNames(impl('ConfigEditor'), true)),
    viewMembers: members(typings, 'EditorView'),
    documentEditorMembers: new Set(bindgenNames(impl('DocumentEditorView'), true)),
    documentsOptions: list('DOCUMENTS_OPTIONS'),
    documentsRemount: list('DOCUMENTS_REMOUNT_OPTIONS'),
    typedDocumentsOptions: typed('DocumentsOptions'),
    documentsUpdateKeys: live('DocumentsUpdate'),
    documentsRefusedKeys: refused(typings, 'DocumentsUpdate'),
    documentsMembers: new Set(bindgenNames(impl('ConfigDocuments'), true)),
    typedSettings: typed('EditorSettings'),
    extendsSettings: {
      options:
        /export interface EditorOptions extends EditorSettings \{/.test(typings) && /export interface EditorUpdate extends SettingsUpdate \{/.test(typings),
      documents:
        /export interface DocumentsOptions extends EditorSettings \{/.test(typings) &&
        /export interface DocumentsUpdate extends SettingsUpdate \{/.test(typings),
    },
    main: entryExports(read('js', 'config-editor', 'index.js')),
    tools: entryExports(read('js', 'config-editor', 'tools.js')),
    mainTypes: entryExports(read('js', 'config-editor', 'index.d.ts')),
    toolsTypes: entryExports(read('js', 'config-editor', 'tools.d.ts')),
    // What wasm-bindgen exports: the functions outside the classes, and the classes.
    bindgen: new Set([
      ...bindgenNames(outsideClasses(), false),
      ...[...jsSources.matchAll(/^#\[wasm_bindgen\]\npub struct (\w+)/gm)].map(([, name]) => name),
    ]),
    types: new Set([...typings.matchAll(/^export (?:interface|type) (\w+)/gm)].map(([, name]) => name)),
  };
}

// --- Reading the bindings (js/config-editor/binding.js, binding.d.ts). -------

function bindingPackage() {
  const classBody = (name) => block(bindingJs, bindingJs.indexOf(`export class ${name} `));
  const options = (body, type) => block(body, body.indexOf(`return new ${type}(element, {`));
  const handle = (body) => block(body, body.indexOf('return {', body.indexOf('#handle() {')));
  const names = (body) => new Set([...body.matchAll(/^ {6}(?:get )?(\w+)(?:[:,(]| \()/gm)].map(([, name]) => name));
  const editor = classBody('EditorBinding');
  const documents = classBody('DocumentsBinding');
  // The settings are read by the generated list of their keys and spread
  // into the options.
  const settingsRead = /for \(const key of Object\.keys\(SETTING_TYPES\)\)/.test(bindingJs);
  return {
    props: members(bindingTypes, 'EditorBindingProps'),
    documentsProps: members(bindingTypes, 'DocumentsBindingProps'),
    forwarded: names(options(editor, 'ConfigEditor')),
    documentsForwarded: names(options(documents, 'ConfigDocuments')),
    extendsSettings: {
      options:
        /export interface EditorBindingProps extends EditorSettings \{/.test(bindingTypes) &&
        options(editor, 'ConfigEditor').includes('...settings,') &&
        settingsRead,
      documents:
        /export interface DocumentsBindingProps extends EditorSettings \{/.test(bindingTypes) &&
        options(documents, 'ConfigDocuments').includes('...settings,') &&
        settingsRead,
    },
    handle: members(bindingTypes, 'ConfigEditorHandle'),
    handleGetters: getters(bindingTypes, 'ConfigEditorHandle'),
    documentsHandle: members(bindingTypes, 'ConfigDocumentsHandle'),
    documentsHandleGetters: getters(bindingTypes, 'ConfigDocumentsHandle'),
    handleImplemented: names(handle(editor)),
    documentsHandleImplemented: names(handle(documents)),
    exports: entryExports(read('js', 'config-editor', 'binding.d.ts')),
  };
}

// --- Reading the framework packages. -------------------------------------------

/** Every name an entry file exports (re-exports, `default as`, declarations). */
function exportsOf(source) {
  const exported = new Set();
  for (const [, list] of source.matchAll(/^export (?:type )?\{([^}]*)\} from "[^"]+";/gm)) {
    for (const item of list.split(',')) if (item.trim()) exported.add(item.trim().split(/\s+/).at(-1));
  }
  for (const [, name] of source.matchAll(/^export (?:const|function|interface|type|class) (\w+)/gm)) exported.add(name);
  return exported;
}

/** The keys of the object literal `const <name> = {` in `source`. */
function objectKeys(source, name) {
  const body = block(source, source.indexOf(`const ${name} = {`));
  return new Set([...body.matchAll(/^  (\w+): /gm)].map(([, key]) => key));
}

/** The package's type names (`EditorPath`, …) that `text` mentions. */
const typeNames = (text, known) => new Set([...text.matchAll(/\b([A-Z]\w+)\b/g)].map(([, name]) => name).filter((name) => known.has(name)));

const capitalized = (name) => name[0].toUpperCase() + name.slice(1);

// Each framework: where its sources are, how it names a binding prop
// (`rename`: binding name → its own), its props with no binding counterpart
// (`only`, with the reason), and how it reads (props, instance API, exports,
// the text of its props' types, and how it hands its props to the binding).
const FRAMEWORKS = {
  React: {
    path: 'js/config-editor-react/src/index.tsx',
    packageJson: 'js/config-editor-react/package.json',
    rename: {},
    documentsRename: {},
    only: {
      className: 'the class of the container `<div>` the component renders',
      style: 'the style of that container',
    },
    documentsOnly: {
      className: 'the class of the container `<div>` the component renders',
      style: 'the style of that container',
    },
    read() {
      const source = read('js', 'config-editor-react', 'src', 'index.tsx');
      // The ref is the binding's handle, the props go to it as they are.
      const wired = (type) =>
        source.includes(`new ${type}("config-editor-react", props)`) &&
        source.split('useEffect(() => binding.current!.update(props));').length === 3 &&
        source.split('useImperativeHandle(ref, () => binding.current!.handle, []);').length === 3;
      return {
        props: members(source, 'ConfigEditorProps'),
        documentsProps: members(source, 'ConfigDocumentsProps'),
        propTypes: block(source, source.indexOf('export interface ConfigEditorProps')) + block(source, source.indexOf('export interface ConfigDocumentsProps')),
        handle: wired('EditorBinding') ? binding.handle : new Set(),
        documentsHandle: wired('DocumentsBinding') ? binding.documentsHandle : new Set(),
        exported: exportsOf(source),
        extendsSettings: {
          options: /export interface ConfigEditorProps extends EditorSettings \{/.test(source),
          documents: /export interface ConfigDocumentsProps extends EditorSettings \{/.test(source),
        },
        wiring: [
          ...(wired('EditorBinding') ? [] : ['ConfigEditor must create an `EditorBinding`, update it with its props and expose its handle as the ref']),
          ...(wired('DocumentsBinding') ? [] : ['ConfigDocuments must create a `DocumentsBinding`, update it with its props and expose its handle as the ref']),
        ],
      };
    },
  },
  Vue: {
    path: 'js/config-editor-vue/src',
    packageJson: 'js/config-editor-vue/package.json',
    rename: { value: 'modelValue', onChange: 'update:modelValue', editorClassName: 'editorClass' },
    documentsRename: { viewClassName: 'viewClass' },
    // `class`, `style` and other attributes fall through to the component's
    // `<div>`, as Vue does for every component: no props of their own.
    only: {},
    documentsOnly: {},
    read() {
      const editor = read('js', 'config-editor-vue', 'src', 'ConfigEditor.ts');
      const documents = read('js', 'config-editor-vue', 'src', 'ConfigDocuments.ts');
      const settings = read('js', 'config-editor-vue', 'src', 'settings.ts');
      const emits = new Set([...block(editor, editor.indexOf('emits: {')).matchAll(/^ {4}"?([\w:]+)"?: /gm)].map(([, name]) => name));
      const fromSettingTypes = /Object\.entries\(SETTING_TYPES\)/.test(settings);
      const wiring = [];
      for (const [source, what, snippets] of [
        [editor, 'ConfigEditor', ['new EditorBinding("config-editor-vue")', 'value: modelValue', 'editorClassName: editorClass', 'emit("update:modelValue", value)', '...rest', 'expose(binding.handle)', 'deep: true']],
        [documents, 'ConfigDocuments', ['new DocumentsBinding("config-editor-vue")', 'viewClassName: viewClass', '...rest', 'expose(binding.handle)', 'deep: true']],
      ]) {
        for (const snippet of snippets) if (!source.includes(snippet)) wiring.push(`${what}: \`${snippet}\` is missing from how it hands its props to the binding`);
      }
      return {
        props: new Set([...objectKeys(editor, 'editorProps'), ...emits]),
        documentsProps: objectKeys(documents, 'documentsProps'),
        propTypes: block(editor, editor.indexOf('const editorProps = {')) + block(editor, editor.indexOf('emits: {')) + block(documents, documents.indexOf('const documentsProps = {')),
        handle: editor.includes('expose(binding.handle)') ? binding.handle : new Set(),
        documentsHandle: documents.includes('expose(binding.handle)') ? binding.documentsHandle : new Set(),
        exported: exportsOf(read('js', 'config-editor-vue', 'src', 'index.ts')),
        extendsSettings: {
          options: editor.includes('{ ...editorProps, ...settingProps }') && fromSettingTypes,
          documents: documents.includes('{ ...documentsProps, ...settingProps }') && fromSettingTypes,
        },
        wiring,
      };
    },
  },
  Svelte: {
    path: 'js/config-editor-svelte/src',
    packageJson: 'js/config-editor-svelte/package.json',
    rename: { editorClassName: 'editorClass' },
    documentsRename: { viewClassName: 'viewClass' },
    only: {
      class: 'the class of the container `<div>` the component renders',
      style: 'the style of that container',
    },
    documentsOnly: {
      class: 'the class of the container `<div>` the component renders',
      style: 'the style of that container',
    },
    read() {
      const types = read('js', 'config-editor-svelte', 'src', 'types.ts');
      const editor = read('js', 'config-editor-svelte', 'src', 'ConfigEditor.svelte');
      const documents = read('js', 'config-editor-svelte', 'src', 'ConfigDocuments.svelte');
      // Svelte exports functions: a getter of the handle is `get<Name>()`
      // (`isDirty()` keeps its name).
      const exportedFunctions = (source) => new Set([...source.matchAll(/^ {2}export function (\w+)/gm)].map(([, name]) => name));
      const svelteName = (member, readonly) => (readonly.has(member) && !/^is[A-Z]/.test(member) ? `get${capitalized(member)}` : member);
      const instance = (source, handle, readonly) => {
        const functions = exportedFunctions(source);
        return new Set([...handle].filter((member) => functions.has(svelteName(member, readonly))));
      };
      const wiring = [];
      for (const [source, what, snippets] of [
        [editor, 'ConfigEditor', ['new EditorBinding("config-editor-svelte")', 'value: hostValue,', '{ ...rest, value: hostValue, editorClassName: editorClass, onChange: changed }', 'readAll(props);']],
        [documents, 'ConfigDocuments', ['new DocumentsBinding("config-editor-svelte")', '{ ...rest, viewClassName: viewClass }', 'readAll(props);']],
      ]) {
        for (const snippet of snippets) if (!source.includes(snippet)) wiring.push(`${what}: \`${snippet}\` is missing from how it hands its props to the binding`);
      }
      return {
        props: members(types, 'ConfigEditorProps'),
        documentsProps: members(types, 'ConfigDocumentsProps'),
        propTypes: types,
        handle: instance(editor, binding.handle, binding.handleGetters),
        documentsHandle: instance(documents, binding.documentsHandle, binding.documentsHandleGetters),
        exported: exportsOf(read('js', 'config-editor-svelte', 'src', 'index.ts')),
        extendsSettings: {
          options: /export interface ConfigEditorProps extends EditorSettings \{/.test(types),
          documents: /export interface ConfigDocumentsProps extends EditorSettings \{/.test(types),
        },
        wiring,
      };
    },
  },
};

// --- The checks. ----------------------------------------------------------------
//
// Each takes what was read (`js`, `binding`, `frameworks`) as arguments, so
// the last test can run them on copies with something taken out.

const js = jsPackage();
const binding = bindingPackage();
const frameworks = Object.entries(FRAMEWORKS).map(([name, framework]) => ({ name, ...framework, ...framework.read() }));

// Where options and members are looked up: the editor's, or the view's.
const EDITOR_SIDE = {
  what: 'ConfigEditor',
  options: (js) => js.options,
  remount: (js) => js.remount,
  typedOptions: (js) => js.typedOptions,
  updateKeys: (js) => js.updateKeys,
  refusedKeys: (js) => js.refusedKeys,
  members: (js) => js.members,
  bindingProps: (binding) => binding.props,
  bindingHandle: (binding) => binding.handle,
  props: (framework) => framework.props,
  handle: (framework) => framework.handle,
  rename: (framework, prop) => framework.rename[prop] ?? prop,
  extends: (js, binding, frameworks) => js.extendsSettings.options && binding.extendsSettings.options && frameworks.every((framework) => framework.extendsSettings.options),
};
const DOCUMENTS_SIDE = {
  what: 'ConfigDocuments',
  options: (js) => js.documentsOptions,
  remount: (js) => js.documentsRemount,
  typedOptions: (js) => js.typedDocumentsOptions,
  updateKeys: (js) => js.documentsUpdateKeys,
  refusedKeys: (js) => js.documentsRefusedKeys,
  members: (js) => js.documentsMembers,
  bindingProps: (binding) => binding.documentsProps,
  bindingHandle: (binding) => binding.documentsHandle,
  props: (framework) => framework.documentsProps,
  handle: (framework) => framework.documentsHandle,
  rename: (framework, prop) => framework.documentsRename[prop] ?? prop,
  extends: (js, binding, frameworks) =>
    js.extendsSettings.documents && binding.extendsSettings.documents && frameworks.every((framework) => framework.extendsSettings.documents),
};

const KINDS = ['option', 'member', 'main', 'tool', 'type', 'setting', 'extends'];

/** Problems with one mapped Rust item `item` (entry `entry`), reached through `side`. */
function entryProblems(what, item, entry, side, { js, binding, frameworks }) {
  const problems = [];
  const kind = KINDS.find((name) => name in entry);
  const name = entry[kind];
  const missing = (where) => problems.push(`${what} \`${item}\`: ${where}`);
  if (kind === 'setting') {
    if (!js.typedSettings.has(name)) missing(`setting \`${name}\` is missing from EditorSettings`);
    if (!EDITOR_SIDE.extends(js, binding, frameworks)) missing('the JS options, the binding\'s props and every component\'s props must extend `EditorSettings`');
  } else if (kind === 'extends') {
    if (!side.extends(js, binding, frameworks)) {
      missing(`the JS options (and \`update\`), the binding's props and every component's props must take every setting (\`extends ${name}\`; the binding spreading \`...settings\` into the options)`);
    }
    if (!js.types.has(name)) missing(`type \`${name}\` is missing from the typings`);
  } else if (kind === 'option') {
    if (!side.options(js).has(name) || !side.typedOptions(js).has(name)) missing(`option \`${name}\` is missing from the ${side.what} options (the options list and its typings)`);
    if (entry.update) {
      if (!side.updateKeys(js).has(entry.update)) missing(`\`update\` key \`${entry.update}\` is missing from the ${side.what} update typings`);
    } else if (entry.fixed) {
      if (!side.remount(js).has(name)) missing(`\`${name}\` is fixed: it must be in the ${side.what} re-mount list, which \`update\` refuses`);
      if (!side.refusedKeys(js).has(name)) missing(`\`${name}\` is fixed: the ${side.what} update typings must type it \`never\``);
    } else {
      missing('an option needs `update` (its key) or `fixed` (why it cannot change live)');
    }
    const prop = entry.prop ?? name;
    if (!side.bindingProps(binding).has(prop)) missing(`binding prop \`${prop}\` is missing from js/config-editor/binding.d.ts`);
    for (const framework of frameworks) {
      const own = side.rename(framework, prop);
      if (!side.props(framework).has(own)) missing(`${framework.name} prop \`${own}\` is missing from ${framework.path}`);
    }
  } else if (kind === 'member') {
    if (!side.members(js).has(name)) missing(`member \`${name}\` is missing from the ${side.what} class`);
    if (side === EDITOR_SIDE) {
      if (!js.viewMembers.has(name)) missing(`member \`${name}\` is missing from the EditorView interface`);
      if (!js.documentEditorMembers.has(name)) missing(`member \`${name}\` is missing from a document's editor (DocumentEditorView)`);
    }
    if (!side.bindingHandle(binding).has(name)) missing(`handle member \`${name}\` is missing from js/config-editor/binding.d.ts`);
    for (const framework of frameworks) {
      if (!side.handle(framework).has(name)) missing(`${framework.name} instance member \`${name}\` is missing from ${framework.path}`);
    }
  } else if (kind === 'main' || kind === 'tool') {
    const entryNames = kind === 'main' ? js.main : js.tools;
    const file = kind === 'main' ? 'js/config-editor/index.js' : 'js/config-editor/tools.js';
    if (!entryNames.has(name)) missing(`\`${name}\` is not exported by ${file}`);
    if (!js.bindgen.has(name)) missing(`\`${name}\` is not a wasm_bindgen export of js/config-editor/src`);
  } else if (kind === 'type') {
    if (!js.types.has(name)) missing(`type \`${name}\` is missing from the TypeScript typings`);
    if (!js.mainTypes.has(name) && !js.toolsTypes.has(name)) missing(`type \`${name}\` is exported by neither js/config-editor/index.d.ts nor tools.d.ts`);
  } else if (!entry.exempt) {
    missing(`the mapping names no JS member (one of ${KINDS.join(', ')}) and no exemption`);
  }
  return problems;
}

function check(table, items, what, side, read) {
  const problems = [];
  for (const item of items) {
    const entry = table[item];
    if (!entry) {
      problems.push(`${what} \`${item}\` has no JS counterpart: map it in api-parity.spec.mjs or exempt it with a reason`);
      continue;
    }
    problems.push(...entryProblems(what, item, entry, side, read));
  }
  for (const item of Object.keys(table)) {
    if (!items.includes(item)) problems.push(`${what} \`${item}\` is in the mapping but no longer in the Rust API: remove it`);
  }
  return problems;
}

const groups = [
  ['ConfigEditor prop', PROPS, rust.props, EDITOR_SIDE],
  ['EditorHandle method', HANDLE, rust.handle, EDITOR_SIDE],
  ['Language method', LANGUAGE, rust.language, EDITOR_SIDE],
  ['ChangeSummary method', CHANGE_SUMMARY, rust.changeSummary, EDITOR_SIDE],
  ['Schema method', SCHEMA, rust.schema, EDITOR_SIDE],
  ['EditorTheme method', THEME, rust.theme, EDITOR_SIDE],
  ['crate export', CRATE_EXPORTS, rust.exports, EDITOR_SIDE],
  ['ConfigDocuments prop', DOCUMENTS_PROPS, rust.documentsProps, DOCUMENTS_SIDE],
  ['DocumentsHandle method', DOCUMENTS_HANDLE, rust.documentsHandle, DOCUMENTS_SIDE],
  ['Document method', DOCUMENT, rust.document, DOCUMENTS_SIDE],
];

/**
 * The options list, its typings and the `update` typings agree: every
 * option is typed, and either an `update` key or refused (re-mount list and
 * `never`); every `update` key is an option.
 */
function optionListProblems(side, { js }) {
  const problems = [];
  const options = side.options(js);
  const typed = side.typedOptions(js);
  const update = side.updateKeys(js);
  const refusedKeys = side.refusedKeys(js);
  const remount = side.remount(js);
  for (const name of options) {
    if (!typed.has(name)) problems.push(`${side.what} option \`${name}\` is in the options list but not typed`);
    if (update.has(name) === remount.has(name)) problems.push(`${side.what} option \`${name}\` must be either an \`update\` key or in the re-mount list (not both)`);
    if (remount.has(name) !== refusedKeys.has(name)) problems.push(`${side.what} option \`${name}\`: the re-mount list and the \`never\` keys of the update typings disagree`);
  }
  for (const name of typed) if (!options.has(name)) problems.push(`${side.what} option \`${name}\` is typed but not in the options list`);
  for (const name of [...update, ...refusedKeys]) {
    if (!options.has(name) && !js.typedSettings.has(name)) problems.push(`${side.what} \`update\` key \`${name}\` is no option`);
  }
  return problems;
}

/**
 * Every JS option (`options`) is passed by the binding (`forwarded`) under
 * its binding prop, which every component has under its own name; every
 * binding prop and component prop maps back to an option or is exempted.
 */
function optionProblems({ tables, options, forwarded, bindingProps, bindingOnly, frameworks, frameworkProps, rename, only, where }) {
  const toProp = new Map(
    tables
      .flatMap((table) => Object.values(table))
      .filter((entry) => entry.option)
      .map((entry) => [entry.option, entry.prop ?? entry.option]),
  );
  const problems = [];
  for (const option of options) {
    const prop = toProp.get(option);
    if (!prop) {
      problems.push(`JS option \`${option}\` has no Rust prop in the mapping (${where})`);
      continue;
    }
    if (!forwarded.has(option)) problems.push(`JS option \`${option}\` is not passed by the binding (js/config-editor/binding.js)`);
    if (!bindingProps.has(prop)) problems.push(`JS option \`${option}\` has no binding prop \`${prop}\` (js/config-editor/binding.d.ts)`);
    for (const framework of frameworks) {
      const name = rename(framework, prop);
      if (!frameworkProps(framework).has(name)) problems.push(`JS option \`${option}\` has no ${framework.name} prop \`${name}\``);
    }
  }
  const mapped = new Set(toProp.values());
  for (const prop of bindingProps) {
    if (!mapped.has(prop) && !(prop in bindingOnly)) problems.push(`binding prop \`${prop}\` maps to no JS option: add it to the mapping or its exemptions`);
  }
  for (const framework of frameworks) {
    const back = new Map([...bindingProps].map((prop) => [rename(framework, prop), prop]));
    for (const prop of frameworkProps(framework)) {
      if (!back.has(prop) && !(prop in only(framework))) {
        problems.push(`${framework.name} prop \`${prop}\` maps to no binding prop: rename it in FRAMEWORKS, or add it to its \`only\` with the reason`);
      }
    }
    for (const prop of bindingProps) {
      if (!frameworkProps(framework).has(rename(framework, prop))) problems.push(`binding prop \`${prop}\` has no ${framework.name} prop \`${rename(framework, prop)}\``);
    }
  }
  return problems;
}

/** Every JS member (but `jsOnly`) is a handle member, implemented by the binding and in every component's instance API. */
function memberProblems({ members, jsOnly, handle, implemented, frameworks, frameworkHandle }) {
  const problems = [];
  for (const name of members) {
    if (name in jsOnly) continue;
    if (!handle.has(name)) problems.push(`JS member \`${name}\` has no handle member (js/config-editor/binding.d.ts)`);
  }
  for (const name of handle) {
    if (!implemented.has(name)) problems.push(`handle member \`${name}\` is not implemented by the binding's \`#handle()\``);
    for (const framework of frameworks) {
      if (!frameworkHandle(framework).has(name)) problems.push(`handle member \`${name}\` is not in the ${framework.name} instance API (${framework.path})`);
    }
  }
  return problems;
}

/**
 * The reverse of the tables: every instance member of the JS classes, the
 * bindings' handles and the components' instance APIs comes from a Rust
 * item (a table entry naming it as its `member`), or is listed in
 * `jsOnly` / `exempt` with the reason it has no Rust counterpart.
 */
function memberSourceProblems({ js, binding, frameworks }) {
  const named = (tables) => new Set(tables.flatMap((table) => Object.values(table)).filter((entry) => entry.member).map((entry) => entry.member));
  const editorMapped = named([HANDLE]);
  const documentsMapped = named([DOCUMENTS_HANDLE]);
  const problems = [];
  const each = (names, mapped, exempt, where) => {
    for (const name of names) {
      if (!mapped.has(name) && !(name in exempt)) problems.push(`${where} member \`${name}\` maps to no Rust item (add it to a table, or exempt it with its reason)`);
    }
  };
  const editorExempt = { ...EDITOR_JS_ONLY, ...EDITOR_MEMBERS_JS_ONLY };
  const documentsExempt = { ...DOCUMENTS_JS_ONLY, ...DOCUMENTS_MEMBERS_JS_ONLY };
  each(js.members, editorMapped, editorExempt, 'ConfigEditor');
  each(js.viewMembers, editorMapped, editorExempt, 'EditorView');
  each(js.documentEditorMembers, editorMapped, editorExempt, 'DocumentEditorView');
  each(js.documentsMembers, documentsMapped, documentsExempt, 'ConfigDocuments');
  each(binding.handle, editorMapped, editorExempt, 'ConfigEditorHandle (binding.d.ts)');
  each(binding.documentsHandle, documentsMapped, documentsExempt, 'ConfigDocumentsHandle (binding.d.ts)');
  for (const framework of frameworks) {
    each(framework.handle, editorMapped, editorExempt, `${framework.name} ConfigEditor instance API`);
    each(framework.documentsHandle, documentsMapped, documentsExempt, `${framework.name} ConfigDocuments instance API`);
  }
  return problems;
}

/** Every export of the main and tools entries is mapped from a Rust item, or listed with its reason. */
function entryListProblems({ js }) {
  const tables = groups.map(([, table]) => table);
  const named = (kind) => new Set(tables.flatMap((table) => Object.values(table)).filter((entry) => entry[kind]).map((entry) => entry[kind]));
  const problems = [];
  for (const name of js.main) if (!named('main').has(name) && !(name in MAIN_ONLY)) problems.push(`js/config-editor/index.js exports \`${name}\`, which no Rust item maps to`);
  for (const name of js.tools) if (!named('tool').has(name) && !(name in TOOLS_ONLY)) problems.push(`js/config-editor/tools.js exports \`${name}\`, which no Rust item maps to`);
  for (const name of js.bindgen) {
    if (!js.main.has(name) && !js.tools.has(name)) problems.push(`wasm_bindgen export \`${name}\` is in neither entry (index.js, tools.js)`);
  }
  return problems;
}

/**
 * A framework package exports its components, their props and handle
 * types, and every type of the package its props and handle take; no
 * function of `@pngwasi/config-editor` (its apps import them from there).
 */
function frameworkExportProblems({ js, frameworks }) {
  const problems = [];
  const handleTypes = block(bindingTypes, bindingTypes.indexOf('export interface ConfigEditorHandle')) + block(bindingTypes, bindingTypes.indexOf('export interface ConfigDocumentsHandle'));
  for (const framework of frameworks) {
    for (const name of ['ConfigEditor', 'ConfigDocuments', 'ConfigEditorProps', 'ConfigDocumentsProps', 'ConfigEditorHandle', 'ConfigDocumentsHandle']) {
      if (!framework.exported.has(name)) problems.push(`\`${name}\` is not exported by ${framework.path}`);
    }
    for (const name of typeNames(framework.propTypes + handleTypes, js.types)) {
      if (!framework.exported.has(name)) problems.push(`${framework.name}: type \`${name}\`, which its props or handle take, is not exported by ${framework.path}`);
    }
    for (const name of [...js.main, ...js.tools]) {
      if (['ConfigEditor', 'ConfigDocuments'].includes(name)) continue;
      if (framework.exported.has(name)) problems.push(`${framework.name} re-exports \`${name}\`: apps import it from \`@pngwasi/config-editor\``);
    }
    const peers = JSON.parse(read(...framework.packageJson.split('/'))).peerDependencies ?? {};
    if (!('@pngwasi/config-editor' in peers)) problems.push(`${framework.packageJson}: \`@pngwasi/config-editor\` must be a peer dependency (its apps import the helpers from it)`);
  }
  return problems;
}

/** Every problem the checks find in `read` (what was read, or a copy). */
function allProblems(read) {
  return [
    ...groups.flatMap(([what, table, items, side]) => check(table, items, what, side, read)),
    ...optionListProblems(EDITOR_SIDE, read),
    ...optionListProblems(DOCUMENTS_SIDE, read),
    ...optionProblems({
      tables: [PROPS],
      options: read.js.options,
      forwarded: read.binding.forwarded,
      bindingProps: read.binding.props,
      bindingOnly: BINDING_ONLY_PROPS,
      frameworks: read.frameworks,
      frameworkProps: (framework) => framework.props,
      rename: EDITOR_SIDE.rename,
      only: (framework) => framework.only,
      where: 'PROPS',
    }),
    ...optionProblems({
      tables: [DOCUMENTS_PROPS],
      options: read.js.documentsOptions,
      forwarded: read.binding.documentsForwarded,
      bindingProps: read.binding.documentsProps,
      bindingOnly: DOCUMENTS_BINDING_ONLY_PROPS,
      frameworks: read.frameworks,
      frameworkProps: (framework) => framework.documentsProps,
      rename: DOCUMENTS_SIDE.rename,
      only: (framework) => framework.documentsOnly,
      where: 'DOCUMENTS_PROPS',
    }),
    ...memberProblems({
      members: read.js.members,
      jsOnly: EDITOR_JS_ONLY,
      handle: read.binding.handle,
      implemented: read.binding.handleImplemented,
      frameworks: read.frameworks,
      frameworkHandle: (framework) => framework.handle,
    }),
    ...memberProblems({
      members: read.js.documentsMembers,
      jsOnly: DOCUMENTS_JS_ONLY,
      handle: read.binding.documentsHandle,
      implemented: read.binding.documentsHandleImplemented,
      frameworks: read.frameworks,
      frameworkHandle: (framework) => framework.documentsHandle,
    }),
    ...memberSourceProblems(read),
    ...entryListProblems(read),
    ...frameworkExportProblems(read),
  ];
}

/** A deep copy of what was read (sets copied), to take things out of. */
function copy(value) {
  if (value instanceof Set) return new Set(value);
  if (Array.isArray(value)) return value.map(copy);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, typeof item === 'function' ? item : copy(item)]));
  return value;
}

test.describe('API parity between the Rust crate and the JS packages', () => {
  test('every EditorSettings field is in the generated TypeScript type', () => {
    expect(rust.settings).toContain('folding');
    const missing = rust.settings.map(camelCase).filter((name) => !js.typedSettings.has(name));
    const extra = [...js.typedSettings].filter((name) => !rust.settings.map(camelCase).includes(name));
    expect({ missing, extra }).toEqual({ missing: [], extra: [] });
  });

  test('the Rust side, the JS package, the bindings and every framework were read', () => {
    // A parser that silently finds nothing would pass everything.
    expect(rust.props).toContain('settings');
    expect(rust.handle).toContain('reveal');
    expect(rust.language).toContain('convert');
    expect(rust.changeSummary).toContain('between');
    expect(rust.schema).toContain('validate');
    expect(rust.theme).toContain('label');
    expect(rust.exports).toContain('unified_diff');
    expect(rust.documentsProps).toContain('on_close');
    expect(rust.documentsHandle).toContain('save_all');
    expect(rust.document).toContain('read_only');
    expect(js.options.has('readOnly')).toBe(true);
    expect(js.remount.has('language')).toBe(true);
    expect(js.updateKeys.has('readOnly')).toBe(true);
    expect(js.refusedKeys.has('schema')).toBe(true);
    expect(js.members.has('update')).toBe(true);
    expect(js.viewMembers.has('unifiedDiff')).toBe(true);
    expect(js.documentEditorMembers.has('value')).toBe(true);
    expect(js.documentsOptions.has('onClose')).toBe(true);
    expect(js.documentsUpdateKeys.has('activeId')).toBe(true);
    expect(js.documentsMembers.has('saveAll')).toBe(true);
    expect(js.main.has('languages')).toBe(true);
    expect(js.tools.has('validate')).toBe(true);
    expect(js.bindgen.has('validate')).toBe(true);
    expect(js.types.has('EditorOptions')).toBe(true);
    expect(js.mainTypes.has('EditorView')).toBe(true);
    expect(binding.props.has('readOnly')).toBe(true);
    expect(binding.forwarded.has('readOnly')).toBe(true);
    expect(binding.handle.has('reveal')).toBe(true);
    expect(binding.handleGetters.has('isDirty')).toBe(true);
    expect(binding.handleImplemented.has('isDirty')).toBe(true);
    expect(binding.documentsForwarded.has('onClose')).toBe(true);
    expect(frameworks.map((framework) => framework.name)).toEqual(['React', 'Vue', 'Svelte']);
    for (const framework of frameworks) {
      expect(framework.props.has('readOnly'), framework.name).toBe(true);
      expect(framework.handle.has('reveal'), framework.name).toBe(true);
      expect(framework.handle.has('isDirty'), framework.name).toBe(true);
      expect(framework.exported.has('ConfigEditor'), framework.name).toBe(true);
      expect(framework.exported.has('EditorPath'), framework.name).toBe(true);
      expect(framework.documentsProps.has('onClose'), framework.name).toBe(true);
      expect(framework.documentsHandle.has('saveAll'), framework.name).toBe(true);
      expect(typeNames(framework.propTypes, js.types).has('HostDiagnostic'), framework.name).toBe(true);
    }
  });

  const read = { js, binding, frameworks };

  for (const [what, table, items, side] of groups) {
    test(`every ${what} is reachable from JS and every framework, or exempted`, () => {
      expect(check(table, items, what, side, read)).toEqual([]);
    });
  }

  test('a ConfigEditor and a document\'s editor are both an EditorView', () => {
    const problems = [];
    for (const name of js.viewMembers) {
      if (!js.members.has(name)) problems.push(`ConfigEditor has no \`${name}\` (EditorView)`);
      if (!js.documentEditorMembers.has(name)) problems.push(`DocumentEditorView has no \`${name}\` (EditorView)`);
    }
    for (const name of js.members) if (!js.viewMembers.has(name) && !['update', 'destroy'].includes(name)) problems.push(`ConfigEditor member \`${name}\` is not in EditorView`);
    for (const name of js.documentEditorMembers) if (!js.viewMembers.has(name)) problems.push(`DocumentEditorView member \`${name}\` is not in EditorView`);
    expect(problems).toEqual([]);
  });

  test('the options lists, their typings and the update typings agree', () => {
    expect([...optionListProblems(EDITOR_SIDE, read), ...optionListProblems(DOCUMENTS_SIDE, read)]).toEqual([]);
  });

  test('every component hands its props to its binding and exposes its handle', () => {
    expect(frameworks.flatMap((framework) => framework.wiring.map((problem) => `${framework.name}: ${problem}`))).toEqual([]);
  });

  test('every JS option reaches the editor through the binding and is a prop of every component, and back', () => {
    expect(
      optionProblems({
        tables: [PROPS],
        options: js.options,
        forwarded: binding.forwarded,
        bindingProps: binding.props,
        bindingOnly: BINDING_ONLY_PROPS,
        frameworks,
        frameworkProps: (framework) => framework.props,
        rename: EDITOR_SIDE.rename,
        only: (framework) => framework.only,
        where: 'PROPS',
      }),
    ).toEqual([]);
  });

  test('the documents options: binding, and every component\'s props agree', () => {
    expect(
      optionProblems({
        tables: [DOCUMENTS_PROPS],
        options: js.documentsOptions,
        forwarded: binding.documentsForwarded,
        bindingProps: binding.documentsProps,
        bindingOnly: DOCUMENTS_BINDING_ONLY_PROPS,
        frameworks,
        frameworkProps: (framework) => framework.documentsProps,
        rename: DOCUMENTS_SIDE.rename,
        only: (framework) => framework.documentsOnly,
        where: 'DOCUMENTS_PROPS',
      }),
    ).toEqual([]);
  });

  test('every JS instance member is in every component\'s instance API', () => {
    expect([
      ...memberProblems({
        members: js.members,
        jsOnly: EDITOR_JS_ONLY,
        handle: binding.handle,
        implemented: binding.handleImplemented,
        frameworks,
        frameworkHandle: (framework) => framework.handle,
      }),
      ...memberProblems({
        members: js.documentsMembers,
        jsOnly: DOCUMENTS_JS_ONLY,
        handle: binding.documentsHandle,
        implemented: binding.documentsHandleImplemented,
        frameworks,
        frameworkHandle: (framework) => framework.documentsHandle,
      }),
    ]).toEqual([]);
  });

  test('every JS instance member, handle member and component member maps back to the Rust API, or is exempted', () => {
    expect(memberSourceProblems(read)).toEqual([]);
  });

  test('every export of the entries maps to the Rust API', () => {
    expect(entryListProblems(read)).toEqual([]);
  });

  test('each framework exports its components and the types they take, and no helper functions', () => {
    expect(frameworkExportProblems(read)).toEqual([]);
  });

  test('the checks fail when a counterpart is missing', () => {
    // Each change takes one counterpart out of a copy of what was read; the
    // checks must name it.
    const cases = [
      ['an option', (read) => read.js.options.delete('readOnly'), 'readOnly'],
      ['an update key', (read) => read.js.updateKeys.delete('baseline'), '`update` key `baseline`'],
      ['a refused key', (read) => read.js.refusedKeys.delete('schema'), 'schema'],
      ['a documents update key', (read) => read.js.documentsUpdateKeys.delete('activeId'), 'activeId'],
      ['an instance member', (read) => read.js.members.delete('fold'), 'fold'],
      ['an EditorView member', (read) => read.js.viewMembers.delete('reveal'), 'EditorView'],
      ['a document editor member', (read) => read.js.documentEditorMembers.delete('foldAll'), 'DocumentEditorView'],
      ['a documents member', (read) => read.js.documentsMembers.delete('saveAll'), 'saveAll'],
      ['a main export', (read) => read.js.main.delete('languages'), 'languages'],
      ['a tools export', (read) => read.js.tools.delete('validate'), 'validate'],
      ['a type', (read) => read.js.types.delete('HostDiagnostic'), 'HostDiagnostic'],
      ['a setting', (read) => read.js.typedSettings.delete('showFooter'), 'showFooter'],
      ['a binding prop', (read) => read.binding.props.delete('hostDiagnostics'), 'hostDiagnostics'],
      ['a forwarded option', (read) => read.binding.forwarded.delete('secrets'), 'secrets'],
      ['a binding handle member', (read) => read.binding.handle.delete('unfold'), 'unfold'],
      ['a React prop', (read) => read.frameworks[0].props.delete('baseline'), 'React prop `baseline`'],
      ['a Vue documents prop', (read) => read.frameworks[1].documentsProps.delete('onClose'), 'Vue prop `onClose`'],
      ['a Svelte instance member', (read) => read.frameworks[2].handle.delete('save'), 'Svelte'],
      ['a framework type export', (read) => read.frameworks[0].exported.delete('FieldPath'), 'FieldPath'],
      ['a re-exported helper', (read) => read.frameworks[1].exported.add('validate'), 'Vue re-exports `validate`'],
      ['a JS-only instance member', (read) => read.js.members.add('jsOnlyThing'), 'ConfigEditor member `jsOnlyThing` maps to no Rust item'],
      ['a JS-only documents member', (read) => read.js.documentsMembers.add('jsOnlyThing'), 'ConfigDocuments member `jsOnlyThing` maps to no Rust item'],
      ['a JS-only handle member', (read) => read.binding.handle.add('jsOnlyThing'), 'ConfigEditorHandle (binding.d.ts) member `jsOnlyThing`'],
      ['a JS-only component member', (read) => read.frameworks[2].documentsHandle.add('jsOnlyThing'), 'Svelte ConfigDocuments instance API member `jsOnlyThing`'],
    ];
    const silent = [];
    for (const [what, takeOut, mentioned] of cases) {
      const changed = copy(read);
      takeOut(changed);
      const problems = allProblems(changed);
      if (!problems.some((problem) => problem.includes(mentioned))) silent.push(`${what}: no problem mentions ${mentioned}`);
    }
    expect(silent).toEqual([]);
    // And nothing is missing as it is.
    expect(allProblems(read)).toEqual([]);
  });
});
