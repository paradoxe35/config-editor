import type { EditorPath, EditorRange, FieldPath, LanguageName, LineRange } from "@pngwasi/config-editor";
import type { ConfigEditorProps } from "./types.js";
/**
 * A schema-aware config editor for YAML, JSON, TOML, HCL and XML.
 *
 * ```svelte
 * <ConfigEditor bind:value={text} language="yaml" {schema} onSave={upload} />
 * ```
 *
 * Through `bind:this`: `reveal`, `save`, `isDirty`, `getValue`, … (the
 * instance API).
 */
declare const ConfigEditor: import("svelte").Component<ConfigEditorProps, {
    getValue: () => string;
    getEditor: () => import("@pngwasi/config-editor").ConfigEditor | null;
    setValue: (text: string) => void;
    unifiedDiff: (context?: number) => string;
    getCurrentPath: () => EditorPath | null;
    rangeOf: (path: FieldPath) => EditorRange | null;
    pathAt: (offset: number) => EditorPath | null;
    reveal: (path: FieldPath) => boolean;
    revealRange: (range: {
        start: number;
        end: number;
    }) => boolean;
    fold: (path: FieldPath) => boolean;
    unfold: (path: FieldPath) => boolean;
    getText: () => string;
    getFoldedLines: () => LineRange[];
    foldAll: () => void;
    unfoldAll: () => void;
    focus: () => void;
    save: () => boolean;
    editAs: (language: LanguageName) => boolean;
    isDirty: () => boolean;
}, "value">;
type ConfigEditor = ReturnType<typeof ConfigEditor>;
export default ConfigEditor;
