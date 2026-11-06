import init, {
  ConfigEditor as Editor,
  type EditorOptions,
  type LanguageName,
  type ThemeColorMap,
  type ThemeName,
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
  themes,
  colorTokens,
  validate,
} from "@pngwasi/config-editor";
export type {
  Conversion,
  CustomLanguage,
  Diagnostic,
  LanguageName,
  ThemeColorMap,
  ThemeName,
} from "@pngwasi/config-editor";

type InitInput = Parameters<typeof init>[0];

/** `onChange` values remembered as still on their way back. */
const MAX_PENDING = 1000;
/** How long a sent value can come back as a late echo. */
const ECHO_WINDOW_MS = 10_000;

/** A short stand-in for a document: its length and a 32-bit FNV-1a hash. */
function fingerprint(text: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `${text.length}:${hash >>> 0}`;
}
let loading: Promise<unknown> | undefined;

/**
 * Loads the WebAssembly module once. The component calls it itself; call it
 * earlier to start the download sooner, or to pass the `.wasm` URL when a
 * bundler does not resolve it next to the package's JavaScript.
 */
export function loadConfigEditor(input?: InitInput): Promise<unknown> {
  // A failed load is not kept: calling again (with the right URL) retries.
  loading ??= init(input).catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
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
  /** JSON Schema as JSON text. Changing it re-mounts. */
  schema?: string;
  /** Text the diff gutter compares against. */
  baseline?: string;
  fileName?: string;
  theme?: ThemeName;
  colors?: ThemeColorMap;
  readOnly?: boolean;
  showToolbar?: boolean;
  showFooter?: boolean;
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
    // Values sent through onChange that the parent has not passed back yet,
    // oldest first. A parent applying them late must not roll the document
    // back: an echo with newer values still pending is stale. Any other
    // `value` (a revert, a reset) is the parent's own and is applied.
    // Kept as fingerprints with the time sent: a fast typist against a slow
    // parent has many in flight, and an echo older than `ECHO_WINDOW_MS`
    // counts as the parent's own value again (a parent that skipped a
    // render for an unchanged value never sends it).
    const pending = useRef<{ print: string; sent: number }[]>([]);
    // Values still on their way when `ref.setValue` replaced the document.
    const superseded = useRef<{ print: string; sent: number }[]>([]);
    // The `value` prop as last handled: a re-mount re-runs the value effect,
    // and an unchanged prop is not a new value from the parent.
    const handledValue = useRef<string | undefined>(undefined);
    // What a re-mount (language, schema, … changed) starts from, so an
    // uncontrolled document and its diff baseline survive it.
    const carried = useRef<{ value: string; baseline: string } | null>(null);
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

    // Construction-time options re-mount; everything else is set live below.
    useEffect(() => {
      let cancelled = false;
      void loadConfigEditor().then(() => {
        if (cancelled || !host.current) return;
        const current = latest.current;
        // A re-mount starts from the newest document: the editor's own, since
        // a late parent's `value` may not have caught up. Values still
        // pending stay pending, so their late echoes are recognized.
        const value = carried.current?.value ?? current.value ?? current.defaultValue ?? "";
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
          className: current.editorClassName,
          onChange: (value) => {
            if (carried.current) carried.current.value = value;
            pending.current.push({ print: fingerprint(value), sent: Date.now() });
            if (pending.current.length > MAX_PENDING) pending.current.shift();
            latest.current.onChange?.(value);
          },
          onEditAsChange: (language) => latest.current.onEditAsChange?.(language),
        };
        try {
          editor.current = new Editor(host.current, options);
          failed.current = false;
          setMounted((count) => count + 1);
        } catch (error) {
          failed.current = true;
          report(error);
        }
      }, report);
      return () => {
        cancelled = true;
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
        const now = Date.now();
        pending.current = pending.current.filter((echo) => now - echo.sent < ECHO_WINDOW_MS);
        const print = fingerprint(value);
        // A value sent before a `ref.setValue` reset is a late echo of what
        // the reset replaced: skipped, so the reset sticks.
        const replaced = superseded.current.findIndex((echo) => echo.print === print && now - echo.sent < ECHO_WINDOW_MS);
        if (replaced >= 0) {
          superseded.current.splice(0, replaced + 1);
          // The parent's state took that old value: tell it the document
          // again, so both end on the reset.
          if (superseded.current.length === 0 && instance.value !== value) {
            const current = instance.value;
            pending.current.push({ print: fingerprint(current), sent: now });
            latest.current.onChange?.(current);
          }
          return;
        }
        // Echoes come back in the order they were sent, so this is the oldest
        // pending copy (a typo fixed sends the same text twice: `a`, `ab`,
        // `a`). A parent applying only the latest value leaves older copies
        // pending until the echo window ends: a reset to such a text in the
        // meantime is taken for its echo (`ref.setValue` always applies).
        const sent = pending.current.findIndex((echo) => echo.print === print);
        if (sent >= 0) {
          // The parent caught up to this emission; newer ones may follow.
          pending.current.splice(0, sent + 1);
          if (pending.current.length > 0) return;
        } else {
          pending.current = [];
        }
        if (instance.value !== value) instance.setValue(value);
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

    useImperativeHandle(
      ref,
      () => ({
        get value() {
          // While a re-mount has failed, the document kept for it.
          return (
            editor.current?.value ??
            carried.current?.value ??
            latest.current.value ??
            latest.current.defaultValue ??
            ""
          );
        },
        get editor() {
          return editor.current;
        },
        setValue(value: string) {
          superseded.current = [...superseded.current, ...pending.current];
          pending.current = [];
          if (carried.current) carried.current.value = value;
          guard((instance) => {
            if (instance.value === value) return;
            instance.setValue(value);
            // The parent's state follows the reset: it is reported like an
            // edit, and its echo is recognised like one.
            pending.current.push({ print: fingerprint(value), sent: Date.now() });
            latest.current.onChange?.(value);
          });
        },
      }),
      [],
    );

    return <div ref={host} className={props.className} style={props.style} />;
  },
);
