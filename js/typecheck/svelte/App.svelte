<!-- The Svelte README examples, type-checked against the built packages (`npm run typecheck`). -->
<script lang="ts" module>
  declare function putConfig(text: string): Promise<void>;
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import {
    type BaselineConflict,
    type ChangeSummary,
    type CloseRequest,
    ConfigDocuments,
    ConfigEditor,
    type DocumentSpec,
    type EditorPath,
    type HostDiagnostic,
    type LanguageName,
  } from "@pngwasi/config-editor-svelte";
  import { loadConfigEditor } from "@pngwasi/config-editor";
  import { unifiedDiff, validate } from "@pngwasi/config-editor/tools";

  let { schema, wasmUrl }: { schema: string; wasmUrl: string } = $props();
  // Start the download early, with the URL your bundler serves the `.wasm` at.
  onMount(() => void loadConfigEditor({ module_or_path: wasmUrl }));

  // Quick start.
  let text = $state("listen: 7835\n");

  // The instance.
  let editor: ReturnType<typeof ConfigEditor> | undefined = $state();
  function goToPort(): boolean {
    return editor?.reveal(["web", "listen"]) ?? false;
  }
  function inspect() {
    const shown: string = editor?.getText() ?? "";
    const value: string = editor?.getValue() ?? "";
    const patch: string = editor?.unifiedDiff(0) ?? unifiedDiff("a\n", "b\n");
    const where: EditorPath | null = editor?.getCurrentPath() ?? null;
    const dirty: boolean = editor?.isDirty() ?? false;
    editor?.setValue("listen: 1\n");
    editor?.fold("/clients");
    editor?.foldAll();
    return [shown, value, patch, where, dirty, validate("yaml", text), goToPort()];
  }

  // Saving.

  let saved = $state("listen: 7835\n");
  let dirty = $state(false);
  async function upload(next: string, language: LanguageName) {
    await putConfig(next);
    saved = next;
    void language;
  }
  let conflicts: BaselineConflict[] = $state([]);
  let summary: ChangeSummary | null = $state(null);
  let breadcrumb = $state("");
  let problems: HostDiagnostic[] = $state([{ path: "/listen", severity: "warning", message: "port in use", source: "server" }]);
  let editAs: LanguageName | null = $state(null);

  // Several documents.
  let tabs: ReturnType<typeof ConfigDocuments> | undefined = $state();
  let files: DocumentSpec[] = $state([
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
    files = files.filter((file) => file.id !== id);
  }
  function saveAll(): string[] {
    return tabs?.saveAll() ?? [];
  }
</script>

<ConfigEditor bind:value={text} language="yaml" {schema} showToolbar showFooter />

<ConfigEditor bind:this={editor} bind:value={text} {editAs} onEditAsChange={(language) => void language} />

<ConfigEditor
  value={text}
  onChange={(next) => (text = next)}
  baseline={saved}
  hostDiagnostics={problems}
  readOnlyPaths={["/listen"]}
  readOnlyRanges={[{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }]}
  secrets={{ hint: "Keeps the stored secret" }}
  onSave={upload}
  onDirtyChange={(next) => (dirty = next)}
  onBaselineConflict={(event) => conflicts.push(event)}
  onChangeSummary={(next) => (summary = next)}
  onPathChange={(path) => (breadcrumb = path.pointer)}
  onSchemaWarning={(message) => console.warn(message)}
  onError={(error) => console.error(error)}
/>

<ConfigEditor
  defaultValue={text}
  readOnly
  theme="nord"
  colors={{ key: "#ffcc66" }}
  highlighting={false}
  showDiff={false}
  completions={false}
  hoverDocs={false}
  strings={{ find: "Suchen" }}
  class="editor"
  style="height: 20rem"
  editorClass="editor-root"
  fileName="burrow.yaml"
/>

<ConfigDocuments
  bind:this={tabs}
  documents={files}
  theme="github-light"
  viewClass="tabs"
  onChange={update}
  onSave={markSaved}
  onClose={close}
  onSelect={(id) => void id}
  onDirtyChange={(id, isDirty) => void [id, isDirty]}
/>

<button onclick={inspect}>{dirty} {summary?.hunks} {breadcrumb} {conflicts.length}</button>
<button onclick={saveAll}>Save all</button>
