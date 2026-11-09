import { createApp, defineComponent, h, nextTick, onMounted, ref, shallowRef } from "vue";
import * as core from "@pngwasi/config-editor";
import * as tools from "@pngwasi/config-editor/tools";
import { loadConfigEditor, themes } from "@pngwasi/config-editor";
import * as api from "@pngwasi/config-editor-vue";
import {
  type BaselineConflict,
  type ChangeSummary,
  ConfigEditor,
  type ConfigEditorHandle,
  type EditorPath,
  type HostDiagnostic,
} from "@pngwasi/config-editor-vue";

// The same page as react.html, for the same tests: the same controls and ids,
// query flags and `window` hooks. The app is a template, as in an SFC.
const query = new URLSearchParams(location.search);
// The bundle does not sit next to the package's wasm, so say where it is.
// `?late-load` loads it only after the component's own load (from its
// default URL, which fails here) has failed; `?double-load` passes a wrong
// URL, then the right one, while the first still loads.
const wasm = { module_or_path: "../config-editor/pkg/config_editor_bg.wasm" };
if (query.has("double-load")) void loadConfigEditor({ module_or_path: "../config-editor/pkg/missing.wasm" }).catch(() => {});
const ready = query.has("late-load")
  ? new Promise((resolve) => setTimeout(resolve, 800)).then(() => loadConfigEditor(wasm))
  : loadConfigEditor(wasm);
// `?uncontrolled`: `default-value` instead of `v-model`. `?late`: edits reach
// the parent 400 ms late, only the latest. `?slow`: every edit 1.5 s late.
// `?reset-early`: `setValue` before the editor mounts. `?props={...}`: extra
// props (`setExtraProps` changes them later).
const uncontrolled = query.has("uncontrolled");
const late = query.has("late");
const slow = query.has("slow");
const resetEarly = query.has("reset-early");
const initialExtra = JSON.parse(query.get("props") ?? "{}") as Record<string, unknown>;
const initialText = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";

type Hooks = {
  packageApi: typeof api;
  coreApi: typeof core;
  setExtraProps: (change: Record<string, unknown> | ((props: Record<string, unknown>) => Record<string, unknown>)) => void;
  editorHandle: { readonly current: ConfigEditorHandle | null };
  renameFile: () => void;
  setParentValue: (value: string) => void;
  setShown: (shown: boolean) => void;
};
const hooks = window as unknown as Hooks;
hooks.packageApi = api;
hooks.coreApi = core;
Object.assign(window, { toolsApi: tools });
// For tests that mount components of their own.
Object.assign(window, { Vue: { createApp, h, nextTick, ref } });

const App = defineComponent({
  components: { ConfigEditor },
  props: { schema: { type: String, required: true } },
  setup() {
    const value = ref(initialText);
    // The version the host last saved; saves and other clients move it.
    const baseline = ref(initialText);
    const dirty = ref(false);
    const saves = ref<string[]>([]);
    const conflicts = ref<BaselineConflict[]>([]);
    // The value before the latest edit, for the parent-side revert button.
    let previous = value.value;
    let lateTimer: ReturnType<typeof setTimeout> | undefined;
    const fileName = ref("tunnel.yaml");
    const editAs = ref<string | null>(null);
    const theme = ref("vscode-dark");
    const readOnly = ref(false);
    const editing = ref("yaml");
    const changes = ref("");
    const path = ref("");
    const hostProblems = ref<HostDiagnostic[]>([]);
    const lockAddress = ref(false);
    const lockFirstLine = ref(false);
    const themeNames = ref<string[]>([]);
    const shown = ref(true);
    const editor = shallowRef<ConfigEditorHandle | null>(null);
    const extra = ref<Record<string, unknown>>(initialExtra);

    hooks.setExtraProps = (change) => {
      extra.value = typeof change === "function" ? change(extra.value) : change;
    };
    hooks.editorHandle = {
      get current() {
        return editor.value;
      },
    };
    hooks.renameFile = () => (fileName.value = `renamed-${fileName.value}`);
    hooks.setParentValue = (text) => (value.value = text);
    hooks.setShown = (next) => (shown.value = next);
    void ready.then(() => (themeNames.value = themes().map((theme) => theme.name)));
    onMounted(() => resetEarly && editor.value?.setValue("from-effect: 1\n"));

    const apply = (next: string) => {
      previous = value.value;
      value.value = next;
    };
    const onModel = (next: string) => {
      if (slow) return void setTimeout(() => apply(next), 1500);
      if (!late) return apply(next);
      clearTimeout(lateTimer);
      lateTimer = setTimeout(() => apply(next), 400);
    };
    // A discard that also drops an edit the late parent has not applied yet.
    const discard = (text: string) => {
      clearTimeout(lateTimer);
      value.value = text;
      editor.value?.setValue(text);
    };
    const toggleHostProblems = () => {
      hostProblems.value = hostProblems.value.length
        ? []
        : [{ path: "/listen", severity: "warning", message: "port in use", source: "server" }];
    };
    const onSave = (text: string) => {
      saves.value = [...saves.value, text];
      baseline.value = text;
    };
    const onChangeSummary = (summary: ChangeSummary) => {
      changes.value = `${summary.hunks} changes: +${summary.added} ~${summary.modified} -${summary.removed}`;
    };
    const onPathChange = (next: EditorPath) => (path.value = next.pointer || "/");

    return {
      value, baseline, dirty, saves, conflicts, fileName, editAs, theme, readOnly, editing, changes, path,
      hostProblems, lockAddress, lockFirstLine, themeNames, shown, editor, extra, uncontrolled, initialText,
      onModel, discard, toggleHostProblems, onSave, onChangeSummary, onPathChange,
      revert: () => (value.value = previous),
      secrets: { hint: "Keeps the stored secret" },
      colors: { key: "#ffcc66" },
    };
  },
  template: `
    <header>
      <strong>config-editor-vue</strong>
      <label>Edit as
        <select id="edit-as" :value="editAs ?? ''" @change="editAs = $event.target.value || null">
          <option value="">Same</option><option>json</option><option>toml</option><option>hcl</option><option>xml</option>
        </select>
      </label>
      <label>Theme
        <select id="theme" v-model="theme"><option v-for="name in themeNames" :key="name">{{ name }}</option></select>
      </label>
      <label><input id="read-only" type="checkbox" v-model="readOnly" /> Read-only</label>
      <button id="replace" type="button" @click="value = 'listen: 9000\\n'">Replace value</button>
      <button id="revert" type="button" @click="revert">Revert last edit</button>
      <button id="reset" type="button" @click="value = initialText">Reset</button>
      <button id="force-reset" type="button" @click="discard(initialText)">Reset (always)</button>
      <button id="bad-theme" type="button" @click="theme = 'not-a-theme'">Invalid theme</button>
      <button id="rename" type="button" @click="fileName = 'renamed-' + fileName">Rename (re-mounts)</button>
      <button id="reveal" type="button" @click="editor?.reveal(['public_address'])">Go to public_address</button>
      <button id="host-problems" type="button" @click="toggleHostProblems">Host problems</button>
      <label><input id="lock-address" type="checkbox" v-model="lockAddress" /> Lock public_address</label>
      <label><input id="lock-first-line" type="checkbox" v-model="lockFirstLine" /> Lock line 1</label>
      <button id="load-secret" type="button" @click="value = 'clients:\\n  acme:\\n    secret: «secret 3f2a»\\n'">Load a secret</button>
      <button id="external-change" type="button" @click="baseline = baseline.replace('7835', '7836')">External change</button>
      <button id="save" type="button" @click="editor?.save()">Save</button>
      <span id="dirty">{{ dirty ? "unsaved" : "saved" }}</span>
      <span id="saves" :data-count="saves.length">{{ saves.at(-1) ?? "" }}</span>
      <span id="conflicts">{{ conflicts.join(",") }}</span>
      <span id="editing">{{ editing }}</span>
      <span id="changes">{{ changes }}</span>
      <code id="breadcrumb">{{ path }}</code>
    </header>
    <ConfigEditor
      v-if="shown"
      ref="editor"
      language="yaml"
      :schema="schema"
      :model-value="uncontrolled ? undefined : value"
      :default-value="uncontrolled ? initialText : undefined"
      @update:model-value="onModel"
      :edit-as="editAs"
      @edit-as-change="editing = $event"
      @change-summary="onChangeSummary"
      @path-change="onPathChange"
      :host-diagnostics="hostProblems"
      :secrets="secrets"
      :read-only-paths="lockAddress ? ['/public_address'] : []"
      :read-only-ranges="lockFirstLine ? [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }] : []"
      :baseline="baseline"
      @save="onSave"
      @dirty-change="dirty = $event"
      @baseline-conflict="conflicts = [...conflicts, $event]"
      :theme="theme"
      :read-only="readOnly"
      :colors="colors"
      :file-name="fileName"
      show-toolbar
      show-footer
      v-bind="extra"
    />
    <pre id="saved">{{ value }}</pre>
  `,
});

const schema = await (await fetch("../../schema.json")).text();
createApp(App, { schema }).mount("#root");
document.body.dataset.ready = "true";
