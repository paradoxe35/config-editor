import { createApp, defineComponent, ref, shallowRef } from "vue";
import {
  type CloseRequest,
  ConfigDocuments,
  type ConfigDocumentsHandle,
  type DocumentSpec,
  type EditorSettings,
  type LanguageName,
  loadConfigEditor,
} from "@pngwasi/config-editor-vue";

// The same page as documents-react.html, for the same tests. The documents
// live in a reactive array the handlers change in place, as Vue apps do.
await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
const schema = await (await fetch("../../schema.json")).text();

const App = defineComponent({
  components: { ConfigDocuments },
  setup() {
    const files = ref<DocumentSpec[]>([
      { id: "main", fileName: "burrow.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema },
      { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\nmax_pending = 512\n", schema },
      { id: "clients", fileName: "conf.d/clients.json", value: '{\n  "clients": {}\n}\n', schema, readOnly: true },
    ]);
    const log = ref<string[]>([]);
    const tabs = shallowRef<ConfigDocumentsHandle | null>(null);
    // For tests: the view's settings (`setViewSettings`), a document's own
    // fields (`updateFile`) and the template ref (`documentsHandle`).
    const settings = ref<EditorSettings>({});
    const note = (line: string) => log.value.push(line);
    const update = (id: string, change: Partial<DocumentSpec>) => {
      const file = files.value.find((candidate) => candidate.id === id);
      if (file) Object.assign(file, change);
    };
    Object.assign(window, {
      setViewSettings: (change: (settings: EditorSettings) => EditorSettings) => (settings.value = change(settings.value)),
      updateFile: update,
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
      onChange: (id: string, value: string) => {
        note(`change ${id}`);
        update(id, { value });
      },
      onSave: (id: string, value: string, language: LanguageName) => {
        note(`save ${id} ${language}`);
        update(id, { baseline: value });
      },
      onSelect: (id: string) => note(`select ${id}`),
      onClose: ({ id, dirty }: CloseRequest) => {
        if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
        files.value = files.value.filter((file) => file.id !== id);
      },
    };
  },
  template: `
    <header>
      <strong>ConfigDocuments (Vue)</strong>
      <button id="save-all" type="button" @click="tabs?.saveAll()">Save all</button>
    </header>
    <ConfigDocuments
      ref="tabs"
      v-bind="settings"
      :documents="files"
      @change="onChange"
      @save="onSave"
      @select="onSelect"
      @close="onClose"
    />
    <pre id="log">{{ log.join("\\n") }}</pre>
  `,
});

createApp(App).mount("#root");
document.body.dataset.ready = "true";
