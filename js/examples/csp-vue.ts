// The Vue binding under a strict CSP (e2e/csp/vue.html; the options and the
// query are e2e/csp/options.js). Render functions, not templates: compiling
// a template at runtime needs `'unsafe-eval'`.
import { createApp, h } from "vue";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { ConfigDocuments, ConfigEditor } from "@pngwasi/config-editor-vue";
import { documentsOptions, editorOptions, initialValue, view, WASM } from "../../e2e/csp/options.js";

await loadConfigEditor(WASM);
createApp({
  render: () =>
    view === "documents"
      ? h(ConfigDocuments, { class: "csp-page__documents", ...documentsOptions() })
      : h(ConfigEditor, { class: "csp-page__editor", ...editorOptions(), defaultValue: initialValue }),
}).mount("#editor");
document.body.dataset.ready = "true";
