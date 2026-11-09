// `@pngwasi/config-editor/binding`: what the React, Vue and Svelte packages
// build on, for a package for another framework.
import {
  type BaselineConflict,
  type ChangeSummary,
  type CloseRequest,
  type Diagnostic,
  ConfigDocuments,
  ConfigEditor,
  type DocumentSpec,
  type EditorPath,
  type EditorRange,
  type EditorSettings,
  type EditorView,
  type FieldPath,
  type HostDiagnostic,
  type LanguageName,
  type LineRange,
  type SecretOptions,
  type TextRangeInput,
} from "./pkg/config_editor.js";

/**
 * Each key of `EditorSettings` and the JavaScript type of its values
 * (generated from the Rust struct): the settings among a component's props,
 * and how a framework declares them (Vue's runtime prop types). Plain data,
 * usable before the WebAssembly module loads.
 */
export const SETTING_TYPES: Readonly<Record<keyof EditorSettings, "boolean" | "string" | "object">>;

/**
 * The props an `EditorBinding` reads: the document and its callbacks, the
 * live props and every setting. React's names; each framework package maps
 * its own (Vue's `modelValue`, …) onto them.
 */
export interface EditorBindingProps extends EditorSettings {
  value?: string;
  defaultValue?: string;
  onChange?: (value: string) => void;
  language?: LanguageName;
  editAs?: LanguageName | null;
  onEditAsChange?: (language: LanguageName) => void;
  onSyncChange?: (inSync: boolean) => void;
  onChangeSummary?: (summary: ChangeSummary) => void;
  onProblems?: (problems: Diagnostic[]) => void;
  onPathChange?: (path: EditorPath) => void;
  hostDiagnostics?: HostDiagnostic[];
  readOnlyPaths?: FieldPath[];
  readOnlyRanges?: TextRangeInput[];
  secrets?: SecretOptions;
  schema?: string;
  onSchemaWarning?: (message: string) => void;
  baseline?: string;
  onSave?: (value: string, language: LanguageName) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onBaselineConflict?: (event: BaselineConflict) => void;
  fileName?: string;
  readOnly?: boolean;
  readOnlyReason?: string | null;
  editorClassName?: string;
  onError?: (error: unknown) => void;
}

/** A framework component's imperative access to its editor (React's ref, Vue's exposed instance). */
export interface ConfigEditorHandle {
  /** The current document, in `language`. */
  readonly value: string;
  /** The underlying `ConfigEditor` instance, once mounted. */
  readonly editor: ConfigEditor | null;
  /**
   * Replaces the document, always, and reports it through the change
   * callback so the host's state follows. A bound value equal to one still
   * on its way through a late host is taken as that late echo; a reset to
   * such a text goes through here.
   */
  setValue(value: string): void;
  /**
   * The unified diff (`diff -u`) of the baseline against the document, with
   * `context` unchanged lines around each change (default 3); empty when
   * they are the same.
   */
  unifiedDiff(context?: number): string;
  /** The path of what the caret is on, or `null` before the editor mounts. */
  readonly currentPath: EditorPath | null;
  /** Where the field at `path` is written in the text on screen, or `null`. */
  rangeOf(path: FieldPath): EditorRange | null;
  /** The path at a string index of the text on screen, or `null`. */
  pathAt(offset: number): EditorPath | null;
  /** Scrolls to the field at `path`, selects it and focuses the editor; `false` when there is no such field. */
  reveal(path: FieldPath): boolean;
  /** Selects `range` (string indices into the text on screen, as `rangeOf` gives them), scrolls it into view inside the editor and focuses the editor without moving the page; any stretch, comments and blank lines included. A range past the end is clamped. `false` before the editor mounts and for an empty document. */
  revealRange(range: { start: number; end: number }): boolean;
  /** Folds the block the field at `path` opens; `false` when there is none to fold. */
  fold(path: FieldPath): boolean;
  /** Opens the folds around the field at `path`; `false` when none was closed. */
  unfold(path: FieldPath): boolean;
  /** The text on screen (in the `editAs` language while editing as another), which `rangeOf` and `pathAt` index into; `""` before the editor mounts. */
  readonly text: string;
  /** The folded blocks, one-based `{ startLine, endLine }` (`startLine` stays visible). */
  readonly foldedLines: LineRange[];
  /** Folds every block. */
  foldAll(): void;
  /** Opens every fold. */
  unfoldAll(): void;
  /** Focuses the editor, keeping its selection. */
  focus(): void;
  /** Saves as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent (no `onSave`, not mounted, or an `editAs` edit that cannot convert back yet). */
  save(): boolean;
  /**
   * Edits as `language` (`language` itself to go back), also when the
   * `editAs` prop already says so: a prop that did not change is not sent
   * again, so after an undo stepped back across the switch this asks for it
   * again. Give the prop the same language. `false` before the editor mounts.
   */
  editAs(language: LanguageName): boolean;
  /** Whether the document differs from `baseline`. */
  readonly isDirty: boolean;
}

/** The props a `DocumentsBinding` reads (React's names). */
export interface DocumentsBindingProps extends EditorSettings {
  documents: DocumentSpec[];
  activeId?: string;
  onSelect?: (id: string) => void;
  onClose?: (request: CloseRequest) => void;
  onAdd?: () => void;
  onChange?: (id: string, value: string) => void;
  onSave?: (id: string, value: string, language: LanguageName) => void;
  onDirtyChange?: (id: string, dirty: boolean) => void;
  onProblems?: (id: string, problems: Diagnostic[]) => void;
  onEditAsChange?: (id: string, language: LanguageName) => void;
  onSyncChange?: (id: string, inSync: boolean) => void;
  label?: string;
  viewClassName?: string;
  onError?: (error: unknown) => void;
}

/** A framework component's imperative access to its documents view. */
export interface ConfigDocumentsHandle {
  /** The underlying `ConfigDocuments` instance, once mounted. */
  readonly instance: ConfigDocuments | null;
  /** The active document's id, or `null`. */
  readonly activeId: string | null;
  /** Shows the document `id` and focuses its editor; `false` when there is no such document. */
  select(id: string): boolean;
  /** Whether the document `id` differs from its baseline. */
  isDirty(id: string): boolean;
  /** Saves the document `id` as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent. */
  save(id: string): boolean;
  /** Saves every document with unsaved edits; returns the ids sent. */
  saveAll(): string[];
  /**
   * Edits the document `id` as `language` (its own language to go back), also
   * when its `editAs` already says so: a prop that did not change is not
   * sent again, so after an undo stepped back across the switch this asks
   * for it again. Give the document the same `editAs`. `false` when there is
   * no such document.
   */
  editAs(id: string, language: LanguageName): boolean;
  /** Focuses the active document's editor. */
  focus(): void;
  /**
   * The editor of the document `id` (paths, folds, focus, saving), or `null`
   * when there is no such document or the view has not mounted. Its queries
   * answer `null` / `false` until the document has been shown once.
   */
  editor(id: string): EditorView | null;
}

/**
 * What a framework component does with a `ConfigEditor`, written once: feed
 * it the props (`attach` when the component's element exists, `update`
 * after every change, `detach` on unmount) and expose `handle`. Settings and
 * most props apply live; `language`, `schema`, `fileName`, `editorClassName`
 * and the `secrets` patterns re-mount (the document, baseline and caret
 * carried over); a late echo of the bound value never rolls the document back; a mount that
 * failed on a bad prop is retried when a prop changes.
 */
export class EditorBinding {
  /** `name` prefixes the errors logged without `onError` (the package's name). */
  constructor(name: string, props?: EditorBindingProps);
  /** The props as last given. */
  readonly props: EditorBindingProps;
  /** The editor, while mounted. */
  readonly instance: ConfigEditor | null;
  /** Whether the last mount threw (a bad prop). */
  readonly failed: boolean;
  /** The component's imperative API. */
  readonly handle: ConfigEditorHandle;
  attach(element: HTMLElement, props?: EditorBindingProps): void;
  update(props: EditorBindingProps): void;
  detach(): void;
}

/** What a framework component does with a `ConfigDocuments` view (see `EditorBinding`); `label`, `viewClassName` and whether `onClose` and `onAdd` are set re-mount. */
export class DocumentsBinding {
  constructor(name: string, props?: DocumentsBindingProps);
  readonly props: DocumentsBindingProps;
  readonly instance: ConfigDocuments | null;
  readonly failed: boolean;
  readonly handle: ConfigDocumentsHandle;
  attach(element: HTMLElement, props?: DocumentsBindingProps): void;
  update(props: DocumentsBindingProps): void;
  detach(): void;
}
