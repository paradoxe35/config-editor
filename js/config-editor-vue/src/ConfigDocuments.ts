import type { CloseRequest, DocumentSpec, LanguageName } from "@pngwasi/config-editor";
import { DocumentsBinding, type DocumentsBindingProps } from "@pngwasi/config-editor/binding";
import {
  type ExtractPublicPropTypes,
  type PropType,
  defineComponent,
  h,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { joinListeners, listener } from "./listeners.js";
import { type SettingProps, settingProps } from "./settings.js";

const optional = <T>(type: PropType<T>) => ({ type, default: undefined });

/**
 * The props of `<ConfigDocuments>`: the documents, the event listeners and
 * the settings of every editor and of the tabs (a document's own `settings`
 * override them). They apply live.
 */
const documentsProps = {
  /** The open documents, in tab order (keep each `value` up to date from `@change`). */
  documents: { type: Array as PropType<DocumentSpec[]>, required: true as const },
  /** The document shown first (default: the first one); a new id shows that document, leaving the focus where it is. */
  activeId: optional(String),
  /** `@select`: a document's id when it becomes the active one. */
  onSelect: listener<(id: string) => void>(),
  /** `@close`: the user asks to close a document. Nothing closes by itself: remove it from `documents` (asking first when `request.dirty`). Only `closable` documents ask; without a listener, tabs have no close button. Whether one is set re-mounts. */
  onClose: listener<(request: CloseRequest) => void>(),
  /** `@add`: the user asks for a new document (the "+" after the last tab, shown only with a listener; it stays at the strip's end when the tabs overflow). It only asks: add the new document to `documents`; the next one added is selected and its editor focused. Whether one is set re-mounts. */
  onAdd: listener<() => void>(),
  /** `@change`: a document's id and text after every edit. */
  onChange: listener<(id: string, value: string) => void>(),
  /** `@save`: the user saved a document (`Ctrl+S` / `Cmd+S`), or you called `save(id)` / `saveAll()`. */
  onSave: listener<(id: string, value: string, language: LanguageName) => void>(),
  /** `@dirty-change`: a document's id and whether it differs from its baseline, when added and then each time that changes. */
  onDirtyChange: listener<(id: string, dirty: boolean) => void>(),
  /** The tab list's accessible name. Default `"Open files"`. Changing it re-mounts. */
  label: optional(String),
  /** Class on the view's root (inside the component's `<div>`). Changing it re-mounts. */
  viewClass: optional(String),
  /** `@error`: an error from loading, mounting or a prop the view rejects. Without a listener, logged. */
  onError: listener<(error: unknown) => void>(),
};

/** The runtime props: the view's own and one per setting. */
export const configDocumentsProps: typeof documentsProps & SettingProps = { ...documentsProps, ...settingProps };

/** The props `<ConfigDocuments>` accepts. */
export type ConfigDocumentsProps = ExtractPublicPropTypes<typeof configDocumentsProps>;

/**
 * Several documents as tabs, one editor per document (each keeps its caret,
 * undo history, scroll and folds across switches).
 *
 * ```vue
 * <ConfigDocuments :documents="files" @change="update" @save="upload" @close="close" />
 * ```
 *
 * Its template ref is a `ConfigDocumentsHandle` (`select`, `saveAll`, `editor(id)`, …).
 */
export const ConfigDocuments = defineComponent({
  name: "ConfigDocuments",
  props: configDocumentsProps,
  setup(props, { expose }) {
    const host = ref<HTMLDivElement>();
    const binding = new DocumentsBinding("config-editor-vue");
    const current = (): DocumentsBindingProps => {
      // A listener given twice (an array) is made one function.
      const { viewClass, ...rest } = joinListeners(props);
      return { ...rest, viewClassName: viewClass };
    };
    onMounted(() => binding.attach(host.value!, current()));
    // Deep: a list or object prop changed in place (a document's value, a
    // pushed diagnostic) is a change too.
    watch(current, (next) => binding.update(next), { deep: true, flush: "post" });
    onBeforeUnmount(() => binding.detach());
    expose(binding.handle);
    return () => h("div", { ref: host });
  },
});
