import { createApp, defineComponent, ref, shallowRef } from "vue";
import {
  type CloseRequest,
  ConfigDocuments,
  type ConfigDocumentsHandle,
  type DocumentSpec,
  type EditorSettings,
  type LanguageName,
} from "@pngwasi/config-editor-vue";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { later } from "./echo-delay";

// The same page as documents-react.html, for the same tests. The documents
// live in a reactive array the handlers change in place, as Vue apps do.
await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
const schema = await (await fetch("../../schema.json")).text();


// `?revisions`: each file keeps the revision `@change` gave its value
// (`valueRevision`), as the README shows: the newest revision wins (a late,
// older one is ignored).
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

const App = defineComponent({
  components: { ConfigDocuments },
  setup() {
    const files = ref<DocumentSpec[]>([
      { id: "main", fileName: "tunnel.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema, closable: false },
      { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\nmax_pending = 512\n", schema },
      { id: "clients", fileName: "conf.d/clients.json", value: '{\n  "clients": {}\n}\n', schema, readOnly: true },
    ]);
    const log = ref<string[]>([]);
    const tabs = shallowRef<ConfigDocumentsHandle | null>(null);
    // For tests: the view's settings (`setViewSettings`), a document's own
    // fields (`updateFile`) and the template ref (`documentsHandle`).
    const settings = ref<EditorSettings>({});
    const note = (line: string) => log.value.push(line);
    // "+": the host makes the new file; the view selects it.
    let added = 0;
    const update = (id: string, change: Partial<DocumentSpec>) => {
      const file = files.value.find((candidate) => candidate.id === id);
      if (file) Object.assign(file, change);
    };
    Object.assign(window, {
      setViewSettings: (change: (settings: EditorSettings) => EditorSettings) => (settings.value = change(settings.value)),
      updateFile: (id: string, change: Partial<DocumentSpec>) => {
        if (forgetNewest && "value" in change && change.valueRevision === undefined) newest.delete(id);
        update(id, change);
      },
      currentFiles: () => files.value,
      documentsHandle: {
        get current() {
          return tabs.value;
        },
      },
    });
    return {
      files,
      log,
      tabs,
      settings,
      onChange: (id: string, value: string, revision: number) => {
        note(`change ${id}`);
        later(() => {
          if (!revisions) return update(id, { value });
          if (revision < (newest.get(id) ?? revision)) return;
          newest.set(id, revision);
          update(id, { value, valueRevision: revision });
        });
      },
      onSave: (id: string, value: string, language: LanguageName) => {
        note(`save ${id} ${language}`);
        update(id, { baseline: value });
      },
      onSelect: (id: string) => note(`select ${id}`),
      onEditAsChange: (id: string, language: LanguageName) => note(`language ${id} ${language}`),
      onSyncChange: (id: string, inSync: boolean) => note(`sync ${id} ${inSync}`),
      onAdd: () => {
        added += 1;
        note(`add new${added}`);
        files.value.push({ id: `new${added}`, fileName: `conf.d/new-${added}.yaml`, value: "", schema });
      },
      onClose: ({ id, dirty }: CloseRequest) => {
        if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
        files.value = files.value.filter((file) => file.id !== id);
      },
    };
  },
  template: `
    <header>
      <h1>ConfigDocuments (Vue)</h1>
      <button id="save-all" type="button" @click="tabs?.saveAll()">Save all</button>
    </header>
    <ConfigDocuments
      ref="tabs"
      v-bind="settings"
      :documents="files"
      @change="onChange"
      @save="onSave"
      @select="onSelect"
      @edit-as-change="onEditAsChange"
      @sync-change="onSyncChange"
      @add="onAdd"
      @close="onClose"
    />
    <pre id="log">{{ log.join("\\n") }}</pre>
  `,
});

createApp(App).mount("#root");
document.body.dataset.ready = "true";
