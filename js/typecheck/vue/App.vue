<!-- The Vue README examples, type-checked against the built packages (`npm run typecheck`). -->
<script setup lang="ts">
import { reactive, ref } from "vue";
import {
  type BaselineConflict,
  type ChangeSummary,
  type CloseRequest,
  ConfigDocuments,
  type ConfigDocumentsHandle,
  ConfigEditor,
  type ConfigEditorHandle,
  type DocumentSpec,
  type EditorPath,
  type HostDiagnostic,
  type LanguageName,
} from "@pngwasi/config-editor-vue";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { unifiedDiff, validate } from "@pngwasi/config-editor/tools";

const props = defineProps<{ schema: string; wasmUrl: string }>();
loadConfigEditor({ module_or_path: props.wasmUrl });

// Quick start.
const text = ref("listen: 7835\n");

// The instance.
const editor = ref<ConfigEditorHandle | null>(null);
function goToPort(): boolean {
  return editor.value?.reveal(["web", "listen"]) ?? false;
}
const shown: string = editor.value?.text ?? "";
const patch: string = editor.value?.unifiedDiff(0) ?? unifiedDiff("a\n", "b\n");
editor.value?.setValue("listen: 1\n");
editor.value?.fold("/clients");
editor.value?.foldAll();
const where: EditorPath | null = editor.value?.currentPath ?? null;
void [goToPort, shown, patch, where, validate("yaml", text.value)];

// Saving.
declare function putConfig(text: string): Promise<void>;
const saved = ref("listen: 7835\n");
const dirty = ref(false);
async function upload(next: string, language: LanguageName) {
  await putConfig(next);
  saved.value = next;
  void language;
}
const conflicts = ref<BaselineConflict[]>([]);
const summary = ref<ChangeSummary | null>(null);
const breadcrumb = ref("");
const problems = ref<HostDiagnostic[]>([{ path: "/listen", severity: "warning", message: "port in use", source: "server" }]);
const editAs = ref<LanguageName | null>(null);

// Several documents.
const tabs = ref<ConfigDocumentsHandle | null>(null);
const files = reactive<DocumentSpec[]>([
  { id: "main", fileName: "burrow.yaml", value: "listen: 7835\n" },
  { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\n" },
]);
function update(id: string, value: string) {
  const file = files.find((candidate) => candidate.id === id);
  if (file) file.value = value;
}
function markSaved(id: string, value: string) {
  const file = files.find((candidate) => candidate.id === id);
  if (file) file.baseline = value;
}
function close({ id, dirty }: CloseRequest) {
  if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
  files.splice(files.findIndex((file) => file.id === id), 1);
}
const savedIds: string[] = tabs.value?.saveAll() ?? [];
void savedIds;
</script>

<template>
  <ConfigEditor v-model="text" language="yaml" :schema="schema" show-toolbar show-footer />

  <ConfigEditor ref="editor" v-model="text" :edit-as="editAs" @edit-as-change="(language: LanguageName) => void language" />

  <ConfigEditor
    v-model="text"
    :baseline="saved"
    :host-diagnostics="problems"
    :read-only-paths="['/listen']"
    :read-only-ranges="[{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }]"
    :secrets="{ hint: 'Keeps the stored secret' }"
    @save="upload"
    @dirty-change="dirty = $event"
    @baseline-conflict="conflicts.push($event)"
    @change-summary="summary = $event"
    @path-change="breadcrumb = $event.pointer"
    @schema-warning="(message: string) => console.warn(message)"
    @error="(error: unknown) => console.error(error)"
  />

  <ConfigEditor
    :default-value="text"
    read-only
    theme="nord"
    :colors="{ key: '#ffcc66' }"
    :highlighting="false"
    :show-diff="false"
    :completions="false"
    :hover-docs="false"
    :strings="{ find: 'Suchen' }"
    class="editor"
    editor-class="editor-root"
    file-name="burrow.yaml"
  />

  <ConfigDocuments
    ref="tabs"
    :documents="files"
    theme="github-light"
    view-class="tabs"
    @change="update"
    @save="markSaved"
    @close="close"
    @select="(id: string) => void id"
    @dirty-change="(id: string, isDirty: boolean) => void [id, isDirty]"
  />
</template>
