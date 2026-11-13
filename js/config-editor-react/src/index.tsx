import type {
  BaselineConflict,
  ChangeSummary,
  CloseRequest,
  Diagnostic,
  DocumentSpec,
  EditorPath,
  EditorSettings,
  FieldPath,
  HostDiagnostic,
  LanguageName,
  SchemaFormatName,
  SecretOptions,
  TextRangeInput,
} from "@pngwasi/config-editor";
import {
  type ConfigDocumentsHandle,
  type ConfigEditorHandle,
  DocumentsBinding,
  EditorBinding,
} from "@pngwasi/config-editor/binding";
import { forwardRef, useEffect, useImperativeHandle, useRef, type CSSProperties } from "react";

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
  HostDiagnostic,
  LanguageName,
  LineRange,
  SchemaFormatName,
  SecretOptions,
  Severity,
  TextRangeInput,
  ThemeColorMap,
  ThemeName,
} from "@pngwasi/config-editor";
export type { ConfigDocumentsHandle, ConfigEditorHandle } from "@pngwasi/config-editor/binding";

/**
 * Props of `<ConfigEditor>`: the document and its callbacks, and every
 * setting (`EditorSettings`: `theme`, `showFooter`, `folding`, …), which
 * apply live.
 */
export interface ConfigEditorProps extends EditorSettings {
  /** The document, in `language` (controlled). */
  value?: string;
  /** The initial document when uncontrolled. */
  defaultValue?: string;
  /** Called with the document, in `language`, after every edit. */
  onChange?: (value: string) => void;
  /** Language of the document. Default `"yaml"`. Changing it re-mounts. */
  language?: LanguageName;
  /** Edit in another language; switches live. */
  editAs?: LanguageName | null;
  /** Called with the language on screen: once on mount, then each time an `editAs` switch completes and when undo or redo steps back across one (a switch waits while the document cannot convert, the host's language staying on screen). */
  onEditAsChange?: (language: LanguageName) => void;
  /** Called with whether the text on screen is written back to the document: once on mount, then each time that changes. `false` while an `editAs` edit cannot convert back (`onChange` waits), whatever brought it there (typing, paste, undo, a switch, a reload, a new value); read after each pause in typing. Always `true` while editing in `language`. */
  onSyncChange?: (inSync: boolean) => void;
  /** Called with how the document differs from `baseline`: on mount, then whenever that changes. */
  onChangeSummary?: (summary: ChangeSummary) => void;
  /** Called with the editor's own problems (syntax, schema, secret placeholders, conversion; never `hostDiagnostics`): on mount, then each time they change. */
  onProblems?: (problems: Diagnostic[]) => void;
  /**
   * Called with the path of what the caret is on each time it changes (and
   * once on mount), for a breadcrumb. The path is computed only while set.
   */
  onPathChange?: (path: EditorPath) => void;
  /**
   * Problems the host found (its own checks: a service, a build step), by path
   * or range, shown next to the editor's own and marked as the host's. A new list replaces
   * the previous one; only the markers update.
   */
  hostDiagnostics?: HostDiagnostic[];
  /**
   * Fields the user cannot change, each with everything inside it, on top of
   * those the schema marks `readOnly`. They hold while editing as another
   * language too. Updates live.
   */
  readOnlyPaths?: FieldPath[];
  /** Stretches of the document (in `language`) the user cannot change. Updates live. */
  readOnlyRanges?: TextRangeInput[];
  /**
   * How secret placeholders are matched, described and completed. Changing
   * `placeholder`, `hint` or `referencePrefixes`, or adding or removing
   * `isPlaceholder`, re-mounts; a new `isPlaceholder` function in its place
   * does not (the latest one is called).
   */
  secrets?: SecretOptions;
  /** JSON Schema as JSON text. Changing it re-mounts. */
  schema?: string;
  /** The format `schema` is written in (default `"json-schema"`, never guessed). Changing it re-mounts. */
  schemaFormat?: SchemaFormatName;
  /** The files `schema` refers to (an include, an import, an external `$ref`), by location. Changing them re-mounts. */
  schemaResources?: Record<string, string>;
  /** Options of `schemaFormat`, such as an XSD's `{ root: "project" }`. Changing them re-mounts. */
  schemaOptions?: Record<string, unknown>;
  /** Called once per thing in `schema` the editor cannot use (an unsupported `patternProperties` regex, what its format cannot express). Default: a console warning. */
  onSchemaWarning?: (message: string) => void;
  /**
   * Text the diff gutter compares against: the version last saved or loaded.
   * A new one while the user has unsaved edits opens a notice (Reload / Keep
   * mine) instead of mixing them; see `onBaselineConflict`.
   */
  baseline?: string;
  /**
   * Called when the user presses `Ctrl+S` / `Cmd+S` (the browser's dialog
   * prevented) or you call `ref.save()`, with the document in `language`.
   * Nothing saves by itself. Without it, `Ctrl+S` is the browser's.
   */
  onSave?: (value: string, language: LanguageName) => void;
  /** Called with whether the document differs from `baseline`: once on mount, then each time that changes. */
  onDirtyChange?: (dirty: boolean) => void;
  /** Called when a new `baseline` arrives under unsaved edits, and when that conflict is settled. */
  onBaselineConflict?: (event: BaselineConflict) => void;
  /** Shown in the toolbar. Changing it re-mounts. */
  fileName?: string;
  /** View mode; switches live. */
  readOnly?: boolean;
  /** Why the editor is read-only, while `readOnly` is on: shown with the "Cannot edit in a read-only editor" notice and in the text area's accessible description. Updates live. */
  readOnlyReason?: string | null;
  /** Class on the editor root (inside the container). Changing it re-mounts. */
  editorClassName?: string;
  /** Class and style of the container element. */
  className?: string;
  style?: CSSProperties;
  /**
   * Called with an error from mounting or from a prop the editor rejects
   * (an unknown language, theme or color token, an invalid schema). Without
   * it errors are logged; they never unmount your React tree.
   */
  onError?: (error: unknown) => void;
}

/**
 * A schema-aware config editor for YAML, JSON, TOML, HCL and XML.
 *
 * ```tsx
 * <ConfigEditor language="yaml" schema={schema} value={text} onChange={setText} />
 * ```
 */
export const ConfigEditor = forwardRef<ConfigEditorHandle, ConfigEditorProps>(function ConfigEditor(props, ref) {
  const host = useRef<HTMLDivElement>(null);
  // Everything an editor needs from a component (live props, re-mounts,
  // the bound value through `ValueSync`), shared with the Vue and Svelte
  // packages: `EditorBinding` from `@pngwasi/config-editor/binding`.
  const binding = useRef<EditorBinding | null>(null);
  binding.current ??= new EditorBinding("config-editor-react", props);
  useEffect(() => {
    binding.current!.attach(host.current!, props);
    return () => binding.current!.detach();
  }, []);
  useEffect(() => binding.current!.update(props));
  useImperativeHandle(ref, () => binding.current!.handle, []);
  return <div ref={host} className={props.className} style={props.style} />;
});

/**
 * Props of `<ConfigDocuments>`: the documents and callbacks, and the settings
 * (`EditorSettings`) of every editor and of the tabs; a document's own
 * `settings` override them. They apply live.
 */
export interface ConfigDocumentsProps extends EditorSettings {
  /** The open documents, in tab order (controlled: keep each `value` up to date from `onChange`). */
  documents: DocumentSpec[];
  /** The document shown first (default: the first one); a new id shows that document, leaving the focus where it is. */
  activeId?: string;
  /** Called with a document's id when it becomes the active one. */
  onSelect?: (id: string) => void;
  /**
   * Called when the user asks to close a document (close button, middle
   * click, `Delete` on its tab). Nothing closes by itself: remove it from
   * `documents`, asking first when `request.dirty`. Without it, tabs have no
   * close button. Only `closable` documents ask. Changing whether it is
   * set re-mounts.
   */
  onClose?: (request: CloseRequest) => void;
  /**
   * Called when the user asks for a new document: the "+" button after the
   * last tab, shown only with it (it stays at the strip's end when the tabs
   * overflow). It only asks: add the new document to `documents`; the next
   * one added is selected and its editor focused. Changing whether it is
   * set re-mounts.
   */
  onAdd?: () => void;
  /** Called with a document's id and text after every edit. */
  onChange?: (id: string, value: string) => void;
  /** Called when the user saves a document (`Ctrl+S` / `Cmd+S`) or you call `ref.save(id)` / `ref.saveAll()`. */
  onSave?: (id: string, value: string, language: LanguageName) => void;
  /** Called with a document's id and whether it differs from its baseline: when it is added, then each time that changes. */
  onDirtyChange?: (id: string, dirty: boolean) => void;
  /** Called with a document's id and its editor's own problems: once its editor mounts (when first shown), then each time they change. */
  onProblems?: (id: string, problems: Diagnostic[]) => void;
  /** Called with a document's id and the language on screen in its editor: once when the editor mounts (the first time the document is shown), then each time that changes (a switch made, one that waited until the text converted, undo or redo across a switch). */
  onEditAsChange?: (id: string, language: LanguageName) => void;
  /** Called with a document's id and whether the text on screen is written back to its `value`: once when its editor mounts, then each time that changes. */
  onSyncChange?: (id: string, inSync: boolean) => void;
  /** The tab list's accessible name. Default `"Open files"`. Changing it re-mounts. */
  label?: string;
  /** Class on the view's root (inside the container). Changing it re-mounts. */
  viewClassName?: string;
  /** Class and style of the container element. */
  className?: string;
  style?: CSSProperties;
  /** Called with an error from mounting or from a prop the view rejects. Without it errors are logged. */
  onError?: (error: unknown) => void;
}

/**
 * Several documents as tabs, one editor per document (each keeps its caret,
 * undo history, scroll and folds across switches).
 *
 * ```tsx
 * <ConfigDocuments documents={files} onChange={(id, value) => update(id, value)} onSave={upload} onClose={close} />
 * ```
 */
export const ConfigDocuments = forwardRef<ConfigDocumentsHandle, ConfigDocumentsProps>(
  function ConfigDocuments(props, ref) {
    const host = useRef<HTMLDivElement>(null);
    const binding = useRef<DocumentsBinding | null>(null);
    binding.current ??= new DocumentsBinding("config-editor-react", props);
    useEffect(() => {
      binding.current!.attach(host.current!, props);
      return () => binding.current!.detach();
    }, []);
    useEffect(() => binding.current!.update(props));
    useImperativeHandle(ref, () => binding.current!.handle, []);
    return <div ref={host} className={props.className} style={props.style} />;
  },
);
