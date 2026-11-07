import init, {
  type BaselineConflict,
  type ChangeSummary,
  type CloseRequest,
  ConfigDocuments as Documents,
  ConfigEditor as Editor,
  type DocumentEditor,
  type DocumentSpec,
  type DocumentsOptions,
  type EditorOptions,
  type EditorPath,
  type EditorRange,
  type EditorSettings,
  type FieldPath,
  type HostDiagnostic,
  type LanguageName,
  type LineRange,
  type SecretOptions,
  type TextRangeInput,
  ValueSync,
  settingKeys,
  unifiedDiff as diffTexts,
} from "@pngwasi/config-editor";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type CSSProperties,
} from "react";

export {
  changeSummary,
  colorTokens,
  convert,
  foldRanges,
  languageForFile,
  languageInfo,
  languages,
  parse,
  pathAt,
  presetCss,
  rangeOf,
  registerLanguage,
  schemaWarnings,
  serialize,
  settingKeys,
  themeInfo,
  themes,
  unifiedDiff,
  validate,
} from "@pngwasi/config-editor";
export type {
  BaselineConflict,
  ChangeSummary,
  CloseRequest,
  DocumentEditor,
  DocumentsCallbackUpdate,
  DocumentsOptions,
  DocumentSpec,
  Conversion,
  CustomLanguage,
  Diagnostic,
  EditorPath,
  EditorRange,
  EditorSettings,
  EditorStrings,
  FieldPath,
  HostDiagnostic,
  LanguageInfo,
  LanguageName,
  LineRange,
  SecretOptions,
  Severity,
  TextRangeInput,
  ThemeColorMap,
  ThemeInfo,
  ThemeName,
} from "@pngwasi/config-editor";

type InitInput = Parameters<typeof init>[0];

let loading: Promise<unknown> | undefined;
// Editors whose own load failed, retried when a later load succeeds.
const onLoaded = new Set<() => void>();

/**
 * Loads the WebAssembly module once. The component calls it itself; call it
 * earlier to start the download sooner, or to pass the `.wasm` URL when a
 * bundler does not resolve it next to the package's JavaScript.
 */
export function loadConfigEditor(input?: InitInput): Promise<unknown> {
  // A failed load is not kept: calling again (with the right URL) retries.
  // A URL given while a load runs: tried when that one fails (a wrong URL,
  // or the default one, followed by the right one).
  if (loading && input !== undefined) {
    const previous = loading;
    loading = previous.catch(() => init(input)).then(
      (value) => {
        for (const retry of [...onLoaded]) retry();
        return value;
      },
      (error: unknown) => {
        loading = undefined;
        throw error;
      },
    );
    return loading;
  }
  loading ??= init(input).then(
    (value) => {
      for (const retry of [...onLoaded]) retry();
      return value;
    },
    (error: unknown) => {
      loading = undefined;
      throw error;
    },
  );
  return loading;
}

/**
 * The settings among `props`: the keys of `EditorSettings` it sets (call
 * once the module has loaded).
 */
function settingsOf(props: object): EditorSettings {
  const settings: Record<string, unknown> = {};
  for (const key of settingKeys()) {
    const value = (props as Record<string, unknown>)[key];
    if (value !== undefined) settings[key] = value;
  }
  return settings as EditorSettings;
}

/**
 * Applies the settings among the props live, on each render where they
 * differ from those applied last (a JSON comparison). Before the first
 * mount it does nothing; after a mount failed (a bad theme or color),
 * changed settings retry it.
 */
function useLiveSettings(
  latest: { current: object },
  apply: (settings: EditorSettings) => void,
  failed: { current: boolean },
  retry: () => void,
): { current: string | null } {
  const applied = useRef<string | null>(null);
  useEffect(() => {
    if (applied.current === null) return;
    const settings = settingsOf(latest.current);
    const key = JSON.stringify(settings);
    if (key === applied.current) return;
    applied.current = key;
    if (failed.current) retry();
    else apply(settings);
  });
  return applied;
}

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
  /** Called with the language on screen when an `editAs` switch completes, and when undo or redo steps back across one (a switch waits while the document cannot convert, the host's language staying on screen). */
  onEditAsChange?: (language: LanguageName) => void;
  /** Called with how the document differs from `baseline`: on mount, then whenever that changes. */
  onChangeSummary?: (summary: ChangeSummary) => void;
  /**
   * Called with the path of what the caret is on each time it changes (and
   * once on mount), for a breadcrumb. The path is computed only while set.
   */
  onPathChange?: (path: EditorPath) => void;
  /**
   * Problems the host found (a server's validation), by path or range, shown
   * next to the editor's own and marked as the host's. A new list replaces
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
   * `placeholder`, `hint` or `referencePrefixes` re-mounts; `isPlaceholder`
   * may change freely (the latest one is called).
   */
  secrets?: SecretOptions;
  /** JSON Schema as JSON text. Changing it re-mounts. */
  schema?: string;
  /** Called once per thing in `schema` the editor cannot use (an unsupported `patternProperties` regex). Default: a console warning. */
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
  fileName?: string;
  /** View mode; switches live. */
  readOnly?: boolean;
  /** Class on the editor root (inside the container). */
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

/** Imperative access, through `ref`. */
export interface ConfigEditorHandle {
  /** The current document, in `language`. */
  readonly value: string;
  /** The underlying `@pngwasi/config-editor` instance, once mounted. */
  readonly editor: Editor | null;
  /**
   * Replaces the document, always, and reports it through `onChange` so the
   * parent's state follows. A `value` prop equal to one still on its way
   * through a late parent is taken as that late echo; a reset to such a text
   * goes through here.
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
  /** Whether the document differs from `baseline`. */
  readonly isDirty: boolean;
}

/**
 * A schema-aware config editor for YAML, JSON, TOML, HCL and XML.
 *
 * ```tsx
 * <ConfigEditor language="yaml" schema={schema} value={text} onChange={setText} />
 * ```
 */
export const ConfigEditor = forwardRef<ConfigEditorHandle, ConfigEditorProps>(
  function ConfigEditor(props, ref) {
    const host = useRef<HTMLDivElement>(null);
    const editor = useRef<Editor | null>(null);
    const latest = useRef(props);
    latest.current = props;
    const [mounted, setMounted] = useState(0);
    // Bumped to retry a mount that failed on a bad prop once a live prop
    // (a setting, editAs) changes, as a re-mount prop change would.
    const [retry, setRetry] = useState(0);
    const failed = useRef(false);
    // Tells the parent's own values (applied) from late echoes of what the
    // editor sent (skipped, so the document never rolls back); shared with
    // every framework wrapper (`ValueSync` from `@pngwasi/config-editor`).
    const sync = useRef(new ValueSync());
    // The `value` prop as last handled: a re-mount re-runs the value effect,
    // and an unchanged prop is not a new value from the parent.
    const handledValue = useRef<string | undefined>(undefined);
    // What a re-mount (language, schema, … changed) starts from, so an
    // uncontrolled document and its diff baseline survive it.
    const carried = useRef<{ value: string; baseline: string } | null>(null);
    // Where the caret was when a re-mount took the editor away while it had
    // focus: the new one takes focus and the caret back, so typing goes on.
    const carriedFocus = useRef<{ start: number; end: number; scrollTop: number } | null>(null);
    // A `ref.setValue` made while there is no editor (before the first
    // mount, or while a re-mount has failed): applied and reported on mount.
    const reset = useRef<string | null>(null);
    const report = (error: unknown) => {
      if (latest.current.onError) latest.current.onError(error);
      else console.error("config-editor-react:", error);
    };
    const guard = (apply: (instance: Editor) => void) => {
      const instance = editor.current;
      if (!instance) return;
      try {
        apply(instance);
      } catch (error) {
        report(error);
      }
    };

    const followPath = (path: EditorPath) => latest.current.onPathChange?.(path);
    const save = (value: string, language: LanguageName) => latest.current.onSave?.(value, language);

    // The secrets options that re-mount (the check function does not).
    const secretsKey = JSON.stringify(
      props.secrets
        ? [props.secrets.placeholder, props.secrets.hint, props.secrets.referencePrefixes, !!props.secrets.isPlaceholder]
        : null,
    );
    // Construction-time options re-mount; everything else is set live below.
    useEffect(() => {
      let cancelled = false;
      // When this load fails, the app's own `loadConfigEditor(url)` succeeding
      // later mounts the editor.
      const retryOnLoad = () => {
        onLoaded.delete(retryOnLoad);
        if (!cancelled) setRetry((count) => count + 1);
      };
      void loadConfigEditor().then(() => {
        if (cancelled || !host.current) return;
        const current = latest.current;
        // A re-mount starts from the newest document: the editor's own, since
        // a late parent's `value` may not have caught up. Values still
        // pending stay pending, so their late echoes are recognized.
        const value = reset.current ?? carried.current?.value ?? current.value ?? current.defaultValue ?? "";
        const baseline = current.baseline ?? carried.current?.baseline ?? value;
        carried.current = { value, baseline };
        const settings = settingsOf(current);
        appliedSettings.current = JSON.stringify(settings);
        const options: EditorOptions = {
          ...settings,
          value,
          language: current.language,
          editAs: current.editAs ?? undefined,
          schema: current.schema,
          baseline,
          fileName: current.fileName,
          readOnly: current.readOnly,
          className: current.editorClassName,
          onChange: (value) => {
            if (carried.current) carried.current.value = value;
            sync.current.sent(value);
            latest.current.onChange?.(value);
          },
          onEditAsChange: (language) => latest.current.onEditAsChange?.(language),
          onChangeSummary: (summary) => latest.current.onChangeSummary?.(summary),
          onPathChange: current.onPathChange ? followPath : undefined,
          hostDiagnostics: current.hostDiagnostics,
          readOnlyPaths: current.readOnlyPaths,
          readOnlyRanges: current.readOnlyRanges,
          secrets: current.secrets && {
            ...current.secrets,
            isPlaceholder: current.secrets.isPlaceholder
              ? (token: string) => latest.current.secrets?.isPlaceholder?.(token) ?? false
              : undefined,
          },
          onSave: current.onSave ? save : undefined,
          onDirtyChange: (dirty) => latest.current.onDirtyChange?.(dirty),
          onBaselineConflict: (event) => latest.current.onBaselineConflict?.(event),
          onSchemaWarning: (message) => {
            const report = latest.current.onSchemaWarning;
            if (report) report(message);
            else console.warn(`config-editor: ${message}`);
          },
        };
        try {
          editor.current = new Editor(host.current, options);
          failed.current = false;
          const focus = carriedFocus.current;
          carriedFocus.current = null;
          const textarea = host.current.querySelector("textarea");
          if (focus && textarea) {
            textarea.focus();
            textarea.setSelectionRange(focus.start, focus.end);
            textarea.scrollTop = focus.scrollTop;
          }
          // A reset made before there was an editor reaches the parent now.
          if (reset.current !== null) {
            const value = reset.current;
            reset.current = null;
            // The `value` prop still holds what the parent had before the
            // reset reaches it: not a new value of its own.
            handledValue.current = latest.current.value;
            sync.current.sent(value);
            latest.current.onChange?.(value);
          }
          setMounted((count) => count + 1);
        } catch (error) {
          failed.current = true;
          report(error);
        }
      }, (error: unknown) => {
        onLoaded.add(retryOnLoad);
        report(error);
      });
      return () => {
        cancelled = true;
        onLoaded.delete(retryOnLoad);
        const textarea = host.current?.querySelector("textarea");
        if (editor.current && textarea && document.activeElement === textarea) {
          carriedFocus.current = {
            start: textarea.selectionStart,
            end: textarea.selectionEnd,
            scrollTop: textarea.scrollTop,
          };
        }
        // destroy() first: it saves an edit still waiting to convert.
        editor.current?.destroy();
        if (editor.current && carried.current) carried.current.value = editor.current.value;
        editor.current?.free();
        editor.current = null;
      };
    }, [props.language, props.schema, props.fileName, props.editorClassName, secretsKey, retry]);

    // Settings apply live, keeping the caret, undo history and folds.
    const appliedSettings = useLiveSettings(
      latest,
      (settings) => guard((instance) => instance.setSettings(settings)),
      failed,
      () => setRetry((count) => count + 1),
    );

    useEffect(() => {
      if (failed.current) setRetry((count) => count + 1);
    }, [props.editAs ?? null]);

    useEffect(() => {
      guard((instance) => {
        const value = props.value;
        if (value === undefined) return;
        if (value === handledValue.current) return;
        handledValue.current = value;
        // First: an Edit-as edit still waiting out the typing pause is sent
        // now (reading `value` does), so its echo is told apart like any.
        const current = instance.value;
        const { apply, report } = sync.current.received(value, current);
        if (apply !== null) instance.setValue(apply);
        if (report !== null) latest.current.onChange?.(report);
      });
    }, [mounted, props.value]);

    useEffect(() => {
      if (props.baseline === undefined) return;
      if (carried.current) carried.current.baseline = props.baseline;
      guard((instance) => instance.setBaseline(props.baseline!));
    }, [mounted, props.baseline]);

    useEffect(() => {
      guard((instance) => instance.setEditAs(props.editAs ?? undefined));
    }, [mounted, props.editAs]);

    useEffect(() => {
      guard((instance) => instance.setReadOnly(props.readOnly ?? false));
    }, [mounted, props.readOnly]);

    const hostDiagnostics = JSON.stringify(props.hostDiagnostics ?? []);
    useEffect(() => {
      guard((instance) => instance.setHostDiagnostics(JSON.parse(hostDiagnostics) as HostDiagnostic[]));
    }, [mounted, hostDiagnostics]);

    const readOnlyPaths = JSON.stringify(props.readOnlyPaths ?? []);
    useEffect(() => {
      guard((instance) => instance.setReadOnlyPaths(JSON.parse(readOnlyPaths) as FieldPath[]));
    }, [mounted, readOnlyPaths]);

    const readOnlyRanges = JSON.stringify(props.readOnlyRanges ?? []);
    useEffect(() => {
      guard((instance) => instance.setReadOnlyRanges(JSON.parse(readOnlyRanges) as TextRangeInput[]));
    }, [mounted, readOnlyRanges]);

    const followsPath = props.onPathChange !== undefined;
    useEffect(() => {
      guard((instance) => instance.setCallbacks({ onPathChange: followsPath ? followPath : null }));
    }, [mounted, followsPath]);

    // `Ctrl+S` is the editor's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    useEffect(() => {
      guard((instance) => instance.setCallbacks({ onSave: saves ? save : null }));
    }, [mounted, saves]);

    // Runs `query` on the editor; `fallback` before it mounts or when it throws.
    const ask = <T,>(query: (instance: Editor) => T, fallback: T): T => {
      const instance = editor.current;
      if (!instance) return fallback;
      try {
        return query(instance);
      } catch (error) {
        report(error);
        return fallback;
      }
    };

    useImperativeHandle(
      ref,
      () => ({
        get value() {
          // While a re-mount has failed, the document kept for it.
          return (
            editor.current?.value ??
            reset.current ??
            carried.current?.value ??
            latest.current.value ??
            latest.current.defaultValue ??
            ""
          );
        },
        get editor() {
          return editor.current;
        },
        get currentPath() {
          return ask((instance) => instance.currentPath, null);
        },
        rangeOf: (path: FieldPath) => ask((instance) => instance.rangeOf(path), null),
        pathAt: (offset: number) => ask((instance) => instance.pathAt(offset), null),
        reveal: (path: FieldPath) => ask((instance) => instance.reveal(path), false),
        fold: (path: FieldPath) => ask((instance) => instance.fold(path), false),
        unfold: (path: FieldPath) => ask((instance) => instance.unfold(path), false),
        get text() {
          return ask((instance) => instance.text, "");
        },
        get foldedLines() {
          return ask((instance) => instance.foldedLines, [] as LineRange[]);
        },
        foldAll: () => guard((instance) => instance.foldAll()),
        unfoldAll: () => guard((instance) => instance.unfoldAll()),
        focus: () => guard((instance) => instance.focus()),
        save: () => ask((instance) => instance.save(), false),
        get isDirty() {
          return ask((instance) => instance.isDirty, false);
        },
        setValue(value: string) {
          if (typeof value !== "string") {
            report(new TypeError("setValue needs a string"));
            return;
          }
          // An Edit-as edit still waiting is sent first, so it is superseded
          // with the others below.
          void editor.current?.value;
          sync.current.supersede();
          if (carried.current) carried.current.value = value;
          if (!editor.current) {
            reset.current = value;
            return;
          }
          guard((instance) => {
            if (instance.value === value) return;
            instance.setValue(value);
            // The parent's state follows the reset: it is reported like an
            // edit, and its echo is recognised like one.
            sync.current.sent(value);
            latest.current.onChange?.(value);
          });
        },
        unifiedDiff(context?: number) {
          if (editor.current) return editor.current.unifiedDiff(context);
          // While a re-mount has failed: the document and baseline kept for
          // it (no diff while the module has not loaded).
          const value =
            reset.current ?? carried.current?.value ?? latest.current.value ?? latest.current.defaultValue ?? "";
          try {
            return diffTexts(carried.current?.baseline ?? latest.current.baseline ?? value, value, context);
          } catch (error) {
            report(error);
            return "";
          }
        },
      }),
      [],
    );

    return <div ref={host} className={props.className} style={props.style} />;
  },
);

/**
 * Props of `<ConfigDocuments>`: the documents and callbacks, and the settings
 * (`EditorSettings`) of every editor and of the tabs; a document's own
 * `settings` override them. They apply live.
 */
export interface ConfigDocumentsProps extends EditorSettings {
  /** The open documents, in tab order (controlled: keep each `value` up to date from `onChange`). */
  documents: DocumentSpec[];
  /** The document shown first. Default: the first one. */
  activeId?: string;
  /** Called with a document's id when it becomes the active one. */
  onSelect?: (id: string) => void;
  /**
   * Called when the user asks to close a document (close button, middle
   * click, `Delete` on its tab). Nothing closes by itself: remove it from
   * `documents`, asking first when `request.dirty`. Without it, tabs have no
   * close button. Changing whether it is set re-mounts.
   */
  onClose?: (request: CloseRequest) => void;
  /** Called with a document's id and text after every edit. */
  onChange?: (id: string, value: string) => void;
  /** Called when the user saves a document (`Ctrl+S` / `Cmd+S`) or you call `ref.save(id)` / `ref.saveAll()`. */
  onSave?: (id: string, value: string, language: LanguageName) => void;
  /** Called with a document's id and whether it differs from its baseline: when it is added, then each time that changes. */
  onDirtyChange?: (id: string, dirty: boolean) => void;
  /** The tab list's accessible name. Default `"Open files"`. Changing it re-mounts. */
  label?: string;
  /** Class on the view's root (inside the container). */
  viewClassName?: string;
  /** Class and style of the container element. */
  className?: string;
  style?: CSSProperties;
  /** Called with an error from mounting or from a prop the view rejects. Without it errors are logged. */
  onError?: (error: unknown) => void;
}

/** Imperative access to a `ConfigDocuments`, through `ref`. */
export interface ConfigDocumentsHandle {
  /** The underlying `@pngwasi/config-editor` instance, once mounted. */
  readonly instance: Documents | null;
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
  /** Focuses the active document's editor. */
  focus(): void;
  /**
   * The editor of the document `id` (paths, folds, focus, saving), or `null`
   * when there is no such document or the view has not mounted. Its queries
   * answer `null` / `false` until the document has been shown once.
   */
  editor(id: string): DocumentEditor | null;
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
    const instance = useRef<Documents | null>(null);
    const latest = useRef(props);
    latest.current = props;
    const [mounted, setMounted] = useState(0);
    const [retry, setRetry] = useState(0);
    const failed = useRef(false);
    // Per document: what its editor holds now, and the echoes of its edits
    // still on their way through the parent (see `ValueSync`).
    const texts = useRef(new Map<string, string>());
    const syncs = useRef(new Map<string, ValueSync>());
    const report = (error: unknown) => {
      if (latest.current.onError) latest.current.onError(error);
      else console.error("config-editor-react:", error);
    };
    const guard = (apply: (view: Documents) => void) => {
      const view = instance.current;
      if (!view) return;
      try {
        apply(view);
      } catch (error) {
        report(error);
      }
    };
    // The parent's list with late echoes of the user's own edits replaced by
    // what the editors hold, so a slow parent never rolls a document back.
    const reconcile = (documents: DocumentSpec[]): DocumentSpec[] => {
      const listed = new Set(documents.map((document) => document.id));
      for (const id of [...texts.current.keys()]) {
        if (!listed.has(id)) {
          texts.current.delete(id);
          syncs.current.delete(id);
        }
      }
      return documents.map((document) => {
        const current = texts.current.get(document.id);
        if (current === undefined) {
          texts.current.set(document.id, document.value);
          return document;
        }
        let sync = syncs.current.get(document.id);
        if (!sync) syncs.current.set(document.id, (sync = new ValueSync()));
        const { apply, report: resend } = sync.received(document.value, current);
        if (resend !== null) latest.current.onChange?.(document.id, resend);
        if (apply !== null) {
          texts.current.set(document.id, apply);
          return document;
        }
        return current === document.value ? document : { ...document, value: current };
      });
    };
    const closable = props.onClose !== undefined;

    useEffect(() => {
      let cancelled = false;
      const retryOnLoad = () => {
        onLoaded.delete(retryOnLoad);
        if (!cancelled) setRetry((count) => count + 1);
      };
      void loadConfigEditor().then(() => {
        if (cancelled || !host.current) return;
        const current = latest.current;
        texts.current.clear();
        syncs.current.clear();
        const settings = settingsOf(current);
        appliedSettings.current = JSON.stringify(settings);
        const options: DocumentsOptions = {
          ...settings,
          documents: reconcile(current.documents),
          activeId: current.activeId,
          className: current.viewClassName,
          label: current.label,
          onSelect: (id) => latest.current.onSelect?.(id),
          onClose: closable ? (request) => latest.current.onClose?.(request) : undefined,
          onChange: (id, value) => {
            texts.current.set(id, value);
            let sync = syncs.current.get(id);
            if (!sync) syncs.current.set(id, (sync = new ValueSync()));
            sync.sent(value);
            latest.current.onChange?.(id, value);
          },
          onSave: (id, value, language) => latest.current.onSave?.(id, value, language),
          onDirtyChange: (id, dirty) => latest.current.onDirtyChange?.(id, dirty),
        };
        try {
          instance.current = new Documents(host.current, options);
          failed.current = false;
          setMounted((count) => count + 1);
        } catch (error) {
          failed.current = true;
          report(error);
        }
      }, (error: unknown) => {
        onLoaded.add(retryOnLoad);
        report(error);
      });
      return () => {
        cancelled = true;
        onLoaded.delete(retryOnLoad);
        instance.current?.destroy();
        instance.current?.free();
        instance.current = null;
      };
    }, [props.label, props.viewClassName, closable, retry]);

    // Settings apply live: every editor keeps its caret, undo history and folds.
    const appliedSettings = useLiveSettings(
      latest,
      (settings) => guard((view) => view.setSettings(settings)),
      failed,
      () => setRetry((count) => count + 1),
    );

    useEffect(() => {
      guard((view) => view.setDocuments(reconcile(props.documents)));
    }, [mounted, props.documents]);

    // `Ctrl+S` is the view's only while `onSave` is set.
    const saves = props.onSave !== undefined;
    useEffect(() => {
      guard((view) =>
        view.setCallbacks({
          onSave: saves ? (id, value, language) => latest.current.onSave?.(id, value, language) : null,
        }),
      );
    }, [mounted, saves]);

    const ask = <T,>(query: (view: Documents) => T, fallback: T): T => {
      const view = instance.current;
      if (!view) return fallback;
      try {
        return query(view);
      } catch (error) {
        report(error);
        return fallback;
      }
    };

    useImperativeHandle(
      ref,
      () => ({
        get instance() {
          return instance.current;
        },
        get activeId() {
          return ask((view) => view.activeId ?? null, null);
        },
        select: (id: string) => ask((view) => view.select(id), false),
        isDirty: (id: string) => ask((view) => view.isDirty(id), false),
        save: (id: string) => ask((view) => view.save(id), false),
        saveAll: () => ask((view) => view.saveAll(), [] as string[]),
        focus: () => guard((view) => view.focus()),
        editor: (id: string) => ask((view) => view.editor(id) ?? null, null),
      }),
      [],
    );

    return <div ref={host} className={props.className} style={props.style} />;
  },
);
