<!-- The same page as documents-react.html, for the same tests. The documents
     live in a `$state` array the handlers change in place. -->
<script lang="ts">
  import { untrack } from "svelte";
  import {
    type CloseRequest,
    ConfigDocuments,
    type ConfigDocumentsHandle,
    type DocumentSpec,
    type EditorSettings,
    type LanguageName,
  } from "@pngwasi/config-editor-svelte";

  let { schema }: { schema: string } = $props();

  // The schema is fixed for the page: read once.
  const documentSchema = untrack(() => schema);
  let files: DocumentSpec[] = $state([
    { id: "main", fileName: "burrow.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema: documentSchema, closable: false },
    { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\nmax_pending = 512\n", schema: documentSchema },
    { id: "clients", fileName: "conf.d/clients.json", value: '{\n  "clients": {}\n}\n', schema: documentSchema, readOnly: true },
  ]);
  let log: string[] = $state([]);
  let settings: EditorSettings = $state({});
  let tabs: ReturnType<typeof ConfigDocuments> | undefined = $state();

  const note = (line: string) => log.push(line);
  function update(id: string, change: Partial<DocumentSpec>) {
    const file = files.find((candidate) => candidate.id === id);
    if (file) Object.assign(file, change);
  }
  // The instance API as a `ConfigDocumentsHandle` (Svelte exports
  // functions; the tests read React's ref shape).
  function asHandle(instance: ReturnType<typeof ConfigDocuments>): ConfigDocumentsHandle {
    return {
      get instance() { return instance.getInstance(); },
      get activeId() { return instance.getActiveId(); },
      select: instance.select,
      isDirty: instance.isDirty,
      save: instance.save,
      saveAll: instance.saveAll,
      editAs: instance.editAs,
      focus: instance.focus,
      editor: instance.editor,
    };
  }
  // For tests: the view's settings, a document's own fields and the instance.
  Object.assign(window, {
    setViewSettings: (change: (settings: EditorSettings) => EditorSettings) => (settings = change(settings)),
    updateFile: update,
    documentsHandle: { get current() { return tabs ? asHandle(tabs) : null; } },
  });

  function onChange(id: string, value: string) {
    note(`change ${id}`);
    update(id, { value });
  }
  function onSave(id: string, value: string, language: LanguageName) {
    note(`save ${id} ${language}`);
    update(id, { baseline: value });
  }
  // "+": the host makes the new file; the view selects it.
  let added = 0;
  function onAdd() {
    added += 1;
    note(`add new${added}`);
    files.push({ id: `new${added}`, fileName: `conf.d/new-${added}.yaml`, value: "", schema: documentSchema });
  }
  function onClose({ id, dirty }: CloseRequest) {
    if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
    files = files.filter((file) => file.id !== id);
  }
</script>

<header>
  <strong>ConfigDocuments (Svelte)</strong>
  <button id="save-all" type="button" onclick={() => tabs?.saveAll()}>Save all</button>
</header>
<ConfigDocuments
  bind:this={tabs}
  {...settings}
  documents={files}
  {onChange}
  {onSave}
  onSelect={(id) => note(`select ${id}`)}
  onEditAsChange={(id, language) => note(`language ${id} ${language}`)}
  onSyncChange={(id, inSync) => note(`sync ${id} ${inSync}`)}
  {onAdd}
  {onClose}
/>
<pre id="log">{log.join("\n")}</pre>
