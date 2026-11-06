import init, {
  ConfigEditor as Editor,
  type EditorOptions,
  type LanguageName,
  type ThemeColorMap,
  type ThemeName,
} from "config-editor";
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
} from "config-editor";
export type {
  Conversion,
  CustomLanguage,
  Diagnostic,
  LanguageName,
  ThemeColorMap,
  ThemeName,
} from "config-editor";

type InitInput = Parameters<typeof init>[0];
let loading: Promise<unknown> | undefined;

/**
 * Loads the WebAssembly module once. The component calls it itself; call it
 * earlier to start the download sooner, or to pass the `.wasm` URL when a
 * bundler does not resolve it next to the package's JavaScript.
 */
export function loadConfigEditor(input?: InitInput): Promise<unknown> {
  loading ??= init(input);
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
}

/** Imperative access, through `ref`. */
export interface ConfigEditorHandle {
  /** The current document, in `language`. */
  readonly value: string;
  /** The underlying `config-editor` instance, once mounted. */
  readonly editor: Editor | null;
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

    // Construction-time options re-mount; everything else is set live below.
    useEffect(() => {
      let cancelled = false;
      void loadConfigEditor().then(() => {
        if (cancelled || !host.current) return;
        const current = latest.current;
        const options: EditorOptions = {
          value: current.value ?? current.defaultValue ?? "",
          language: current.language,
          editAs: current.editAs ?? undefined,
          schema: current.schema,
          baseline: current.baseline,
          fileName: current.fileName,
          theme: current.theme,
          colors: current.colors,
          readOnly: current.readOnly,
          showToolbar: current.showToolbar,
          showFooter: current.showFooter,
          className: current.editorClassName,
          onChange: (value) => latest.current.onChange?.(value),
          onEditAsChange: (language) => latest.current.onEditAsChange?.(language),
        };
        editor.current = new Editor(host.current, options);
        setMounted((count) => count + 1);
      });
      return () => {
        cancelled = true;
        editor.current?.destroy();
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
    ]);

    useEffect(() => {
      const instance = editor.current;
      if (instance && props.value !== undefined && instance.value !== props.value) {
        instance.setValue(props.value);
      }
    }, [mounted, props.value]);

    useEffect(() => {
      if (props.baseline !== undefined) editor.current?.setBaseline(props.baseline);
    }, [mounted, props.baseline]);

    useEffect(() => {
      editor.current?.setEditAs(props.editAs ?? undefined);
    }, [mounted, props.editAs]);

    useEffect(() => {
      if (props.theme !== undefined) editor.current?.setTheme(props.theme);
    }, [mounted, props.theme]);

    const colors = JSON.stringify(props.colors ?? {});
    useEffect(() => {
      editor.current?.setColors(JSON.parse(colors) as ThemeColorMap);
    }, [mounted, colors]);

    useEffect(() => {
      editor.current?.setReadOnly(props.readOnly ?? false);
    }, [mounted, props.readOnly]);

    useImperativeHandle(
      ref,
      () => ({
        get value() {
          return editor.current?.value ?? latest.current.value ?? latest.current.defaultValue ?? "";
        },
        get editor() {
          return editor.current;
        },
      }),
      [],
    );

    return <div ref={host} className={props.className} style={props.style} />;
  },
);
