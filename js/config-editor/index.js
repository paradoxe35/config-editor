// The package entry, what an app uses: loading the WebAssembly module, the
// editor, the documents view and the language and theme catalogs. Text
// without an editor is `@pngwasi/config-editor/tools`; what a framework
// package builds on is `@pngwasi/config-editor/binding`. An explicit list:
// the generated module also holds wasm-bindgen's internals.
export {
  ConfigDocuments,
  ConfigEditor,
  languageForFile,
  languages,
  registerLanguage,
  themes,
} from "./pkg/config_editor.js";
export { loadConfigEditor } from "./load.js";
