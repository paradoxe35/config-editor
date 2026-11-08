import type {
  BaselineConflict,
  ChangeSummary,
  EditorPath,
  FieldPath,
  HostDiagnostic,
  LanguageName,
  SecretOptions,
  TextRangeInput,
} from "@pngwasi/config-editor";
import { type EditorBindingProps, EditorBinding } from "@pngwasi/config-editor/binding";
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

/** A prop whose absence matters (an event listener, an optional value): `undefined` when not passed. */
const optional = <T>(type: PropType<T>) => ({ type, default: undefined });

/**
 * The props of `<ConfigEditor>`: the document (`v-model`), the live props,
 * the event listeners (`@save`, `@dirty-change`, …: listeners are props, so
 * the editor knows which are set) and every setting (`theme`, `showFooter`,
 * `folding`, …), which apply live.
 */
const editorProps = {
  /** The document, in `language` (`v-model`). */
  modelValue: optional(String),
  /** The initial document when there is no `v-model`. */
  defaultValue: optional(String),
  /** Language of the document. Default `"yaml"`. Changing it re-mounts. */
  language: optional(String as PropType<LanguageName>),
  /** Edit in another language; switches live. */
  editAs: optional(String as PropType<LanguageName | null>),
  /** `@edit-as-change`: the language on screen, when an `editAs` switch completes and when undo or redo steps back across one. */
  onEditAsChange: listener<(language: LanguageName) => void>(),
  /** `@change-summary`: how the document differs from `baseline`, on mount and whenever that changes. */
  onChangeSummary: listener<(summary: ChangeSummary) => void>(),
  /** `@path-change`: the path of what the caret is on, each time it changes (and once on mount). The path is computed only while a listener is set. */
  onPathChange: listener<(path: EditorPath) => void>(),
  /** Problems the host found (a server's validation), by path or range, shown next to the editor's own. Updates live. */
  hostDiagnostics: optional(Array as PropType<HostDiagnostic[]>),
  /** Fields the user cannot change, each with everything inside it, on top of the schema's `readOnly` ones. Updates live. */
  readOnlyPaths: optional(Array as PropType<FieldPath[]>),
  /** Stretches of the document (in `language`) the user cannot change. Updates live. */
  readOnlyRanges: optional(Array as PropType<TextRangeInput[]>),
  /** How secret placeholders are matched, described and completed. Changing `placeholder`, `hint` or `referencePrefixes` re-mounts; `isPlaceholder` may change freely. */
  secrets: optional(Object as PropType<SecretOptions>),
  /** JSON Schema as JSON text. Changing it re-mounts. */
  schema: optional(String),
  /** `@schema-warning`: once per thing in `schema` the editor cannot use. Without a listener, a console warning. */
  onSchemaWarning: listener<(message: string) => void>(),
  /** Text the diff gutter compares against: the version last saved or loaded. A new one under unsaved edits opens a notice (Reload / Keep mine). */
  baseline: optional(String),
  /** `@save`: `Ctrl+S` / `Cmd+S` or `save()`, with the document in `language`. Without a listener, `Ctrl+S` is the browser's. */
  onSave: listener<(value: string, language: LanguageName) => void>(),
  /** `@dirty-change`: whether the document differs from `baseline`, once on mount and then each time that changes. */
  onDirtyChange: listener<(dirty: boolean) => void>(),
  /** `@baseline-conflict`: a new `baseline` arrived under unsaved edits, or that conflict was settled. */
  onBaselineConflict: listener<(event: BaselineConflict) => void>(),
  /** Shown in the toolbar. Changing it re-mounts. */
  fileName: optional(String),
  /** View mode; switches live. */
  readOnly: optional(Boolean),
  /** Class on the editor root (inside the component's `<div>`, which takes `class` and `style`). Changing it re-mounts. */
  editorClass: optional(String),
  /** `@error`: an error from loading, mounting or a prop the editor rejects. Without a listener, logged. */
  onError: listener<(error: unknown) => void>(),
};

/** The runtime props: the editor's own and one per setting. */
export const configEditorProps: typeof editorProps & SettingProps = { ...editorProps, ...settingProps };

/** The props `<ConfigEditor>` accepts (`v-model` is `modelValue` and `@update:model-value`). */
export type ConfigEditorProps = ExtractPublicPropTypes<typeof configEditorProps> & {
  "onUpdate:modelValue"?: (value: string) => void;
};

/**
 * A schema-aware config editor for YAML, JSON, TOML, HCL and XML.
 *
 * ```vue
 * <ConfigEditor v-model="text" language="yaml" :schema="schema" @save="upload" />
 * ```
 *
 * Its template ref is a `ConfigEditorHandle` (`reveal`, `save`, `isDirty`, …).
 */
export const ConfigEditor = defineComponent({
  name: "ConfigEditor",
  props: configEditorProps,
  emits: {
    /** The document after every edit (`v-model`). */
    "update:modelValue": (value: string) => typeof value === "string",
  },
  setup(props, { emit, expose }) {
    const host = ref<HTMLDivElement>();
    // Everything an editor needs from a component (live props, re-mounts,
    // the bound value through `ValueSync`), shared with the React and
    // Svelte packages: `EditorBinding` from `@pngwasi/config-editor`.
    const binding = new EditorBinding("config-editor-vue");
    const onChange = (value: string) => emit("update:modelValue", value);
    // The props as the binding names them (a listener given twice, as an
    // array, made one function); the watcher below follows each one, deeply.
    const current = (): EditorBindingProps => {
      const { modelValue, editorClass, ...rest } = joinListeners(props);
      return { ...rest, value: modelValue, editorClassName: editorClass, onChange };
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
