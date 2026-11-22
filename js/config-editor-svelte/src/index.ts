export { default as ConfigEditor } from "./ConfigEditor.svelte";
export { default as ConfigDocuments } from "./ConfigDocuments.svelte";
export type { ConfigDocumentsProps, ConfigEditorProps } from "./types.js";
export type {
  BaselineConflict,
  ChangeSummary,
  CloseRequest,
  Diagnostic,
  DocumentSpec,
  EditorPath,
  EditorRange,
  EditorSettings,
  EditorStrings,
  EditorView,
  FieldPath,
  GoToDefinition,
  HostDiagnostic,
  KeyName,
  KeyNames,
  LanguageName,
  LineRange,
  SchemaFormatName,
  SecretOptions,
  Severity,
  TextRangeInput,
  TextChange,
  ApplyChangesOptions,
  ThemeColorMap,
  ThemeName,
} from "@pngwasi/config-editor";
export type { ConfigDocumentsHandle, ConfigEditorHandle } from "@pngwasi/config-editor/binding";
