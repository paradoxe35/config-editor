import type { EditorView, LanguageName } from "@pngwasi/config-editor";
import type { ConfigDocumentsProps } from "./types.js";
/**
 * Several documents as tabs, one editor per document (each keeps its caret,
 * undo history, scroll and folds across switches).
 *
 * ```svelte
 * <ConfigDocuments documents={files} onChange={update} onSave={upload} onClose={close} />
 * ```
 *
 * Through `bind:this`: `select`, `save`, `saveAll`, `editor(id)`, … (the
 * instance API).
 */
declare const ConfigDocuments: import("svelte").Component<ConfigDocumentsProps, {
    getInstance: () => import("@pngwasi/config-editor").ConfigDocuments | null;
    getActiveId: () => string | null;
    select: (id: string) => boolean;
    isDirty: (id: string) => boolean;
    save: (id: string) => boolean;
    saveAll: () => string[];
    editAs: (id: string, language: LanguageName) => boolean;
    focus: () => void;
    editor: (id: string) => EditorView | null;
}, "">;
type ConfigDocuments = ReturnType<typeof ConfigDocuments>;
export default ConfigDocuments;
