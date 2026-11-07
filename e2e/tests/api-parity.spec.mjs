import { expect, test } from '@playwright/test';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The Rust API and the JS packages stay in step: every public prop of
// `ConfigEditor` and `ConfigDocuments`, method of `EditorHandle`,
// `DocumentsHandle`, `Document`, `Language`, `ChangeSummary`, `Schema` and
// `EditorTheme`, and item re-exported from the crate root has a
// JavaScript counterpart (an option, a method, a function or a type) and one
// in each framework package, React, Vue and Svelte (a prop or event, an
// instance member or a re-export), or an exemption saying why not. The
// framework components are thin wrappers around the bindings of
// `@pngwasi/config-editor` (js/config-editor/binding.js): every JS option
// must reach the editor through the binding's props, and every binding prop
// must be a prop of each component (under its framework's name: Vue's
// `modelValue`, …). `EditorSettings` is not mapped by hand: its TypeScript
// type is generated from the struct (js/config-editor/tests/settings.rs
// checks it is current); here every field must be in it, and the JS options
// and each component's props must extend it. Reads the sources only: no
// build, no browser.

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

// --- The mapping: Rust name → JS name → binding prop, or an exemption. -----
//
// `js` names an option (props), a method of the `ConfigEditor` class
// (handle), or an exported function or type (everything else, as `kind`
// says). `prop` is the binding's (and React's) name for an option, by
// default the same; each framework renames a few (FRAMEWORKS below).

const PROPS = {
  language: { js: 'language' },
  edit_as: { js: 'editAs' },
  initial_value: { exempt: '`value` is both the initial and the live document in JS; React has `defaultValue`' },
  value: { js: 'value' },
  baseline: { js: 'baseline' },
  schema: { js: 'schema' },
  file_name: { js: 'fileName' },
  class: { js: 'className', prop: 'editorClassName' },
  settings: { js: 'EditorSettings', kind: 'extends' },
  on_change: { js: 'onChange' },
  on_edit_as_change: { js: 'onEditAsChange' },
  on_change_summary: { js: 'onChangeSummary' },
  on_schema_warning: { js: 'onSchemaWarning' },
  read_only: { js: 'readOnly' },
  handle: { exempt: 'the `ConfigEditor` instance (a component: its instance API) is the handle' },
  host_diagnostics: { js: 'hostDiagnostics' },
  on_path_change: { js: 'onPathChange' },
  on_save: { js: 'onSave' },
  on_dirty_change: { js: 'onDirtyChange' },
  on_baseline_conflict: { js: 'onBaselineConflict' },
  read_only_paths: { js: 'readOnlyPaths' },
  read_only_ranges: { js: 'readOnlyRanges' },
  secrets: { js: 'secrets' },
};

// Binding props (so every component's) with no JS option of their own.
const BINDING_ONLY_PROPS = {
  defaultValue: 'the uncontrolled initial document (JS: `value`)',
  onError: 'mount and prop errors, which JS throws instead',
};

const HANDLE = {
  new: { exempt: '`new ConfigEditor(element, options)` mounts and returns the handle' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (a component: its `editor`)' },
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
  class: { js: 'className', prop: 'viewClassName' },
  label: { js: 'label' },
  handle: { exempt: 'the `ConfigDocuments` instance (a component: its instance API) is the handle' },
};

const DOCUMENTS_BINDING_ONLY_PROPS = {
  onError: 'mount and prop errors, which JS throws instead',
};

const DOCUMENTS_HANDLE = {
  new: { exempt: '`new ConfigDocuments(element, options)` mounts and returns the handle' },
  is_mounted: { exempt: 'an instance is mounted from its constructor until `destroy()` (a component: its `instance`)' },
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

// --- Reading the bindings (js/config-editor/binding.js, binding.d.ts). -----

/** Members of `export interface <name>` in `source` (quoted names unquoted). */
function members(source, name) {
  const body = block(source, source.search(new RegExp(`export interface ${name}\\b`)));
  return new Set([...body.matchAll(/^  (?:readonly )?(\w+|"[^"]+")\??[:(]/gm)].map(([, member]) => member.replaceAll('"', '')));
}

/** The `readonly` members (getters) of `export interface <name>`. */
function getters(source, name) {
  const body = block(source, source.search(new RegExp(`export interface ${name}\\b`)));
  return new Set([...body.matchAll(/^  readonly (\w+)\??:/gm)].map(([, member]) => member));
}

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
  };
}

const binding = bindingPackage();

// --- Reading the framework packages. ---------------------------------------

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

const capitalized = (name) => name[0].toUpperCase() + name.slice(1);

// Each framework: where its sources are, how it names a binding prop
// (`rename`: binding name → its own), its props with no binding counterpart
// (`only`, with the reason), and how it reads (props, instance API, exports,
// and how it hands its props to the binding).
const FRAMEWORKS = {
  React: {
    path: 'js/config-editor-react/src/index.tsx',
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

const frameworks = Object.entries(FRAMEWORKS).map(([name, framework]) => ({ name, ...framework, ...framework.read() }));

// --- The checks. -----------------------------------------------------------

const js = jsPackage();

// Where options and methods are looked up: the editor's, or the documents
// view's.
const EDITOR_SIDE = {
  options: () => js.options,
  typedOptions: () => js.typedOptions,
  methods: () => js.methods,
  bindingProps: () => binding.props,
  bindingHandle: () => binding.handle,
  props: (framework) => framework.props,
  handle: (framework) => framework.handle,
  rename: (framework, prop) => framework.rename[prop] ?? prop,
  extends: () => js.extendsSettings.options && binding.extendsSettings.options && frameworks.every((framework) => framework.extendsSettings.options),
  optionsWhere: 'EDITOR_OPTIONS and the EditorOptions typings',
};
const DOCUMENTS_SIDE = {
  options: () => js.documentsOptions,
  typedOptions: () => js.typedDocumentsOptions,
  methods: () => js.documentsMethods,
  bindingProps: () => binding.documentsProps,
  bindingHandle: () => binding.documentsHandle,
  props: (framework) => framework.documentsProps,
  handle: (framework) => framework.documentsHandle,
  rename: (framework, prop) => framework.documentsRename[prop] ?? prop,
  extends: () =>
    js.extendsSettings.documents && binding.extendsSettings.documents && frameworks.every((framework) => framework.extendsSettings.documents),
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
      for (const framework of frameworks) {
        if (!framework.extendsSettings.options) problems.push(`${what} \`${item}\`: the ${framework.name} props must extend \`EditorSettings\``);
      }
      continue;
    }
    if (kind === 'extends') {
      if (!side.extends()) {
        problems.push(
          `${what} \`${item}\`: the JS options, the binding's props and every component's props must extend \`${entry.js}\` (the binding spreading \`...settings\` into the options)`,
        );
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
    const prop = entry.prop ?? entry.js;
    if (kind === 'option' && !side.bindingProps().has(prop)) {
      problems.push(`${what} \`${item}\`: binding prop \`${prop}\` is missing from js/config-editor/binding.d.ts`);
    }
    if (kind === 'method' && !side.bindingHandle().has(entry.js)) {
      problems.push(`${what} \`${item}\`: handle member \`${entry.js}\` is missing from js/config-editor/binding.d.ts`);
    }
    for (const framework of frameworks) {
      const name = kind === 'option' ? side.rename(framework, prop) : entry.js;
      const has = {
        option: side.props(framework).has(name),
        method: side.handle(framework).has(name),
        function: framework.exported.has(name),
        class: framework.exported.has(name),
        type: framework.exported.has(name),
      }[kind];
      if (!has) {
        const where = { option: 'prop', method: 'instance member' }[kind] ?? 'export';
        problems.push(`${what} \`${item}\`: ${framework.name} ${where} \`${name}\` is missing from ${framework.path}`);
      }
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

/**
 * Every JS option (`options`) is passed by the binding (`forwarded`) under
 * its binding prop, which every component has under its own name; every
 * binding prop and component prop maps back to an option or is exempted.
 */
function optionProblems({ table, options, forwarded, bindingProps, bindingOnly, frameworkProps, rename, only, where }) {
  const toProp = new Map(
    Object.values(table)
      .filter((entry) => !entry.exempt)
      .map((entry) => [entry.js, entry.prop ?? entry.js]),
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

/** Every JS method (but `jsOnly`) is a handle member, implemented by the binding and in every component's instance API. */
function methodProblems({ methods, jsOnly, handle, implemented, frameworkHandle }) {
  const problems = [];
  for (const name of methods) {
    if (name in jsOnly) continue;
    if (!handle.has(name)) problems.push(`JS method \`${name}\` has no handle member (js/config-editor/binding.d.ts)`);
  }
  for (const name of handle) {
    if (!implemented.has(name)) problems.push(`handle member \`${name}\` is not implemented by the binding's \`#handle()\``);
    for (const framework of frameworks) {
      if (!frameworkHandle(framework).has(name)) problems.push(`handle member \`${name}\` is not in the ${framework.name} instance API (${framework.path})`);
    }
  }
  return problems;
}

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

  test('the Rust side, the bindings and every framework were read', () => {
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
      expect(framework.exported.has('validate'), framework.name).toBe(true);
      expect(framework.exported.has('ConfigEditor'), framework.name).toBe(true);
      expect(framework.documentsProps.has('onClose'), framework.name).toBe(true);
      expect(framework.documentsHandle.has('saveAll'), framework.name).toBe(true);
    }
  });

  for (const [what, table, items, kind, side] of groups) {
    test(`every ${what} is mapped to JS and every framework, or exempted`, () => {
      expect(check(table, items, what, kind, side)).toEqual([]);
    });
  }

  test('the documents view was read', () => {
    expect(rust.documentsProps).toContain('on_close');
    expect(rust.documentsHandle).toContain('save_all');
    expect(rust.document).toContain('read_only');
    expect(js.documentsOptions.has('onClose')).toBe(true);
    expect(js.documentsMethods.has('saveAll')).toBe(true);
  });

  test('every component hands its props to its binding and exposes its handle', () => {
    expect(frameworks.flatMap((framework) => framework.wiring.map((problem) => `${framework.name}: ${problem}`))).toEqual([]);
  });

  test('the documents options: list, typings, binding, and every component\'s props agree', () => {
    const problems = [
      ...[...js.documentsOptions].filter((name) => !js.typedDocumentsOptions.has(name)).map((name) => `option \`${name}\` is in DOCUMENTS_OPTIONS but not typed in DocumentsOptions`),
      ...[...js.typedDocumentsOptions].filter((name) => !js.documentsOptions.has(name)).map((name) => `option \`${name}\` is typed in DocumentsOptions but not in DOCUMENTS_OPTIONS`),
      ...optionProblems({
        table: DOCUMENTS_PROPS,
        options: js.documentsOptions,
        forwarded: binding.documentsForwarded,
        bindingProps: binding.documentsProps,
        bindingOnly: DOCUMENTS_BINDING_ONLY_PROPS,
        frameworkProps: (framework) => framework.documentsProps,
        rename: DOCUMENTS_SIDE.rename,
        only: (framework) => framework.documentsOnly,
        where: 'DOCUMENTS_PROPS',
      }),
    ];
    expect(problems).toEqual([]);
  });

  test('every JS documents method is in every component\'s instance API', () => {
    const jsOnly = {
      destroy: 'the component unmounts the view itself',
      setDocuments: 'the `documents` prop',
      settings: 'the setting props',
      setSettings: 'the setting props',
      setCallbacks: 'callback props',
    };
    expect(
      methodProblems({
        methods: js.documentsMethods,
        jsOnly,
        handle: binding.documentsHandle,
        implemented: binding.documentsHandleImplemented,
        frameworkHandle: (framework) => framework.documentsHandle,
      }),
    ).toEqual([]);
  });

  test('the option list and the EditorOptions typings name the same options', () => {
    const problems = [
      ...[...js.options].filter((name) => !js.typedOptions.has(name)).map((name) => `option \`${name}\` is in EDITOR_OPTIONS but not typed in EditorOptions`),
      ...[...js.typedOptions].filter((name) => !js.options.has(name)).map((name) => `option \`${name}\` is typed in EditorOptions but not in EDITOR_OPTIONS`),
    ];
    expect(problems).toEqual([]);
  });

  test('every JS option reaches the editor through the binding and is a prop of every component, and back', () => {
    expect(
      optionProblems({
        table: PROPS,
        options: js.options,
        forwarded: binding.forwarded,
        bindingProps: binding.props,
        bindingOnly: BINDING_ONLY_PROPS,
        frameworkProps: (framework) => framework.props,
        rename: EDITOR_SIDE.rename,
        only: (framework) => framework.only,
        where: 'PROPS',
      }),
    ).toEqual([]);
  });

  test('every JS instance member is in every component\'s instance API', () => {
    const jsOnly = {
      destroy: 'the component unmounts the editor itself',
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
    expect(
      methodProblems({
        methods: js.methods,
        jsOnly,
        handle: binding.handle,
        implemented: binding.handleImplemented,
        frameworkHandle: (framework) => framework.handle,
      }),
    ).toEqual([]);
  });

  test('every JS function and public type is re-exported by every framework package', () => {
    const problems = [];
    for (const framework of frameworks) {
      for (const name of [...js.functions, ...js.types, 'loadConfigEditor', 'ConfigEditorHandle', 'ConfigDocumentsHandle']) {
        if (['ConfigEditor', 'ConfigDocuments', 'EditorOptions', 'CallbackUpdate'].includes(name)) continue;
        if (!framework.exported.has(name)) problems.push(`\`${name}\` is not re-exported by ${framework.path}`);
      }
      for (const name of ['ConfigEditor', 'ConfigDocuments', 'ConfigEditorProps', 'ConfigDocumentsProps']) {
        if (!framework.exported.has(name)) problems.push(`\`${name}\` (the component or its props) is not exported by ${framework.path}`);
      }
    }
    expect(problems).toEqual([]);
  });
});
