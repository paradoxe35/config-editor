import init, {
  type ChangeSummary,
  ConfigEditor as Editor,
  type EditorOptions,
  type EditorPath,
  type EditorRange,
  type FieldPath,
  type HostDiagnostic,
  type LanguageName,
  type ThemeColorMap,
  type ThemeName,
  ValueSync,
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
  convert,
  languages,
  registerLanguage,
  schemaWarnings,
  themes,
  unifiedDiff,
  colorTokens,
  validate,
} from "@pngwasi/config-editor";
export type {
  ChangeSummary,
  Conversion,
  CustomLanguage,
  Diagnostic,
  EditorPath,
  EditorRange,
  FieldPath,
  HostDiagnostic,
  LanguageName,
  Severity,
  ThemeColorMap,
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

export interface ConfigEditorProps {
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
  /** Called when an `editAs` switch completes. */
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
  /** JSON Schema as JSON text. Changing it re-mounts. */
  schema?: string;
  /** Called once per thing in `schema` the editor cannot use (an unsupported `patternProperties` regex). Default: a console warning. */
  onSchemaWarning?: (message: string) => void;
  /** Text the diff gutter compares against. */
  baseline?: string;
  fileName?: string;
  theme?: ThemeName;
  colors?: ThemeColorMap;
  readOnly?: boolean;
  showToolbar?: boolean;
  showFooter?: boolean;
  /** Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Default `true`. Changing it re-mounts. */
  search?: boolean;
  /** Go to line (`Ctrl+G`). Default `true`. Changing it re-mounts. */
  goToLine?: boolean;
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
  /** Focuses the editor, keeping its selection. */
  focus(): void;
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
    // (theme, colors, editAs) changes, as a re-mount prop change would.
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
        const options: EditorOptions = {
          value,
          language: current.language,
          editAs: current.editAs ?? undefined,
          schema: current.schema,
          baseline,
          fileName: current.fileName,
          theme: current.theme,
          colors: current.colors,
          readOnly: current.readOnly,
          showToolbar: current.showToolbar,
          showFooter: current.showFooter,
          search: current.search,
          goToLine: current.goToLine,
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
    }, [
      props.language,
      props.schema,
      props.fileName,
      props.showToolbar,
      props.showFooter,
      props.search,
      props.goToLine,
      props.editorClassName,
      retry,
    ]);

    const liveProps = JSON.stringify([props.theme ?? null, props.colors ?? null, props.editAs ?? null]);
    useEffect(() => {
      if (failed.current) setRetry((count) => count + 1);
    }, [liveProps]);

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
      guard((instance) => instance.setTheme(props.theme ?? null));
    }, [mounted, props.theme]);

    const colors = JSON.stringify(props.colors ?? {});
    useEffect(() => {
      guard((instance) => instance.setColors(JSON.parse(colors) as ThemeColorMap));
    }, [mounted, colors]);

    useEffect(() => {
      guard((instance) => instance.setReadOnly(props.readOnly ?? false));
    }, [mounted, props.readOnly]);

    const hostDiagnostics = JSON.stringify(props.hostDiagnostics ?? []);
    useEffect(() => {
      guard((instance) => instance.setHostDiagnostics(JSON.parse(hostDiagnostics) as HostDiagnostic[]));
    }, [mounted, hostDiagnostics]);

    const followsPath = props.onPathChange !== undefined;
    useEffect(() => {
      guard((instance) => instance.setCallbacks({ onPathChange: followsPath ? followPath : null }));
    }, [mounted, followsPath]);

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
        focus: () => guard((instance) => instance.focus()),
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
