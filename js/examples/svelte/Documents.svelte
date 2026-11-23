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
    type GoToDefinition,
    type GoToUse,
    type LanguageName,
  } from "@pngwasi/config-editor-svelte";
  import { later } from "../echo-delay";
  import { closedFile, findUses, keys, keyFiles, keyNames, reveal } from "../key-names.js";

  let { schema }: { schema: string } = $props();
  // `?revisions`: each file keeps the revision `onChange` gave its value
  // (`valueRevision`), as the README shows: the newest revision wins (a
  // late, older one is ignored).
  const revisions = new URLSearchParams(location.search).has("revisions");
  // The newest revision heard for each file, which a text of the host's own
  // (a `valueRevision` left out) does not forget: an older echo still on its
  // way after one is dropped too.
  const newest = new Map<string, number>();
  // For tests: `?forget-newest` brings back a mistake a host can make: a
  // text of the host's own forgets the newest revision heard for the file, so
  // an older echo still on its way after it takes the host back (the editor
  // then tells it the document again).
  const forgetNewest = new URLSearchParams(location.search).has("forget-newest");

  // The schema is fixed for the page: read once.
  const documentSchema = untrack(() => schema);
  let files: DocumentSpec[] = $state(keys ? keyFiles : [
    { id: "main", fileName: "tunnel.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema: documentSchema, closable: false },
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
      setValue: instance.setValue,
      applyChanges: instance.applyChanges,
    };
  }
  // For tests: the view's settings, a document's own fields and the instance.
  Object.assign(window, {
    setViewSettings: (change: (settings: EditorSettings) => EditorSettings) => (settings = change(settings)),
    updateFile: (id: string, change: Partial<DocumentSpec>) => {
      if (forgetNewest && "value" in change && change.valueRevision === undefined) newest.delete(id);
      update(id, change);
    },
    currentFiles: () => files,
    documentsHandle: { get current() { return tabs ? asHandle(tabs) : null; } },
  });

  function onChange(id: string, value: string, revision: number) {
    note(`change ${id}`);
    later(() => {
      if (!revisions) return update(id, { value });
      if (revision < (newest.get(id) ?? revision)) return;
      newest.set(id, revision);
      update(id, { value, valueRevision: revision });
    });
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
  // `?keys`: a name no open file defines goes to the host, which opens its
  // file there.
  function goTo(target: GoToDefinition | GoToUse) {
    const file = closedFile(target.source, files);
    if (file) files.push(file);
    reveal(() => (tabs ? asHandle(tabs) : null), target);
  }
  function onGoToDefinition(target: GoToDefinition) {
    note(`goto ${target.source} ${target.name}`);
    goTo(target);
  }
  function onGoToUse(target: GoToUse) {
    note(`use ${target.source} ${target.path}`);
    goTo(target);
  }
  function onClose({ id, dirty }: CloseRequest) {
    if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
    files = files.filter((file) => file.id !== id);
  }
</script>

<header>
  <h1>ConfigDocuments (Svelte)</h1>
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
  keyNames={keys ? keyNames : undefined}
  onGoToDefinition={keys ? onGoToDefinition : undefined}
  onFindUses={keys ? findUses : undefined}
  onGoToUse={keys ? onGoToUse : undefined}
/>
<pre id="log">{log.join("\n")}</pre>
