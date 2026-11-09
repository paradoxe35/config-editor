import type { BaselineConflict, ChangeSummary, CloseRequest, Diagnostic, DocumentSpec, EditorPath, EditorSettings, FieldPath, HostDiagnostic, LanguageName, SecretOptions, TextRangeInput } from "@pngwasi/config-editor";
/**
 * Props of `<ConfigEditor>`: the document (`bind:value`), its callbacks, and
 * every setting (`EditorSettings`: `theme`, `showFooter`, `folding`, …),
 * which apply live.
 */
export interface ConfigEditorProps extends EditorSettings {
    /** The document, in `language` (`bind:value`, or one-way with `onChange`). */
    value?: string;
    /** The initial document when `value` is not given. */
    defaultValue?: string;
    /** Called with the document, in `language`, after every edit. */
    onChange?: (value: string) => void;
    /** Language of the document. Default `"yaml"`. Changing it re-mounts. */
    language?: LanguageName;
    /** Edit in another language; switches live. */
    editAs?: LanguageName | null;
    /** Called with the language on screen when an `editAs` switch completes, and when undo or redo steps back across one. */
    onEditAsChange?: (language: LanguageName) => void;
    /** Called with whether the text on screen is written back to the document: once on mount, then each time that changes. `false` while an `editAs` edit cannot convert back (`onChange` waits), whatever brought it there (typing, paste, undo, a switch, a reload, a new value); read after each pause in typing. Always `true` while editing in `language`. */
    onSyncChange?: (inSync: boolean) => void;
    /** Called with how the document differs from `baseline`: on mount, then whenever that changes. */
    onChangeSummary?: (summary: ChangeSummary) => void;
    /** Called with the editor's own problems (syntax, schema, secret placeholders, conversion; never `hostDiagnostics`): on mount, then each time they change. */
    onProblems?: (problems: Diagnostic[]) => void;
    /** Called with the path of what the caret is on each time it changes (and once on mount). The path is computed only while set. */
    onPathChange?: (path: EditorPath) => void;
    /** Problems the host found (a server's validation), by path or range, shown next to the editor's own. Updates live. */
    hostDiagnostics?: HostDiagnostic[];
    /** Fields the user cannot change, each with everything inside it, on top of the schema's `readOnly` ones. Updates live. */
    readOnlyPaths?: FieldPath[];
    /** Stretches of the document (in `language`) the user cannot change. Updates live. */
    readOnlyRanges?: TextRangeInput[];
    /** How secret placeholders are matched, described and completed. Changing `placeholder`, `hint` or `referencePrefixes` re-mounts; `isPlaceholder` may change freely. */
    secrets?: SecretOptions;
    /** JSON Schema as JSON text. Changing it re-mounts. */
    schema?: string;
    /** Called once per thing in `schema` the editor cannot use. Default: a console warning. */
    onSchemaWarning?: (message: string) => void;
    /** Text the diff gutter compares against: the version last saved or loaded. A new one under unsaved edits opens a notice (Reload / Keep mine). */
    baseline?: string;
    /** Called on `Ctrl+S` / `Cmd+S` or `save()`, with the document in `language`. Without it, `Ctrl+S` is the browser's. */
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
    editorClass?: string;
    /** Class of the container element. */
    class?: string;
    /** Style of the container element. */
    style?: string;
    /** Called with an error from loading, mounting or a prop the editor rejects. Without it, errors are logged. */
    onError?: (error: unknown) => void;
}
/**
 * Props of `<ConfigDocuments>`: the documents and callbacks, and the settings
 * of every editor and of the tabs (a document's own `settings` override
 * them). They apply live.
 */
export interface ConfigDocumentsProps extends EditorSettings {
    /** The open documents, in tab order (keep each `value` up to date from `onChange`). */
    documents: DocumentSpec[];
    /** The document shown first (default: the first one); a new id shows that document, leaving the focus where it is. */
    activeId?: string;
    /** Called with a document's id when it becomes the active one. */
    onSelect?: (id: string) => void;
    /** Called when the user asks to close a document. Nothing closes by itself: remove it from `documents`, asking first when `request.dirty`. Only `closable` documents ask; without it, tabs have no close button. Whether it is set re-mounts. */
    onClose?: (request: CloseRequest) => void;
    /** Called when the user asks for a new document: the "+" after the last tab, shown only with it (it stays at the strip's end when the tabs overflow). It only asks: add the new document to `documents`; the next one added is selected and its editor focused. Whether it is set re-mounts. */
    onAdd?: () => void;
    /** Called with a document's id and text after every edit. */
    onChange?: (id: string, value: string) => void;
    /** Called when the user saves a document (`Ctrl+S` / `Cmd+S`) or you call `save(id)` / `saveAll()`. */
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
    viewClass?: string;
    /** Class of the container element. */
    class?: string;
    /** Style of the container element. */
    style?: string;
    /** Called with an error from loading, mounting or a prop the view rejects. Without it, errors are logged. */
    onError?: (error: unknown) => void;
}
