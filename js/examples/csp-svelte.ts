// The Svelte binding under a strict CSP (e2e/csp/svelte.html; the options
// and the query are e2e/csp/options.js).
import { mount } from "svelte";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { ConfigDocuments, ConfigEditor } from "@pngwasi/config-editor-svelte";
import { documentsOptions, editorOptions, initialValue, view, WASM } from "../../e2e/csp/options.js";

await loadConfigEditor(WASM);
const target = document.getElementById("editor")!;
if (view === "documents") {
  mount(ConfigDocuments, { target, props: { class: "csp-page__documents", ...documentsOptions() } });
} else {
  mount(ConfigEditor, { target, props: { class: "csp-page__editor", ...editorOptions(), defaultValue: initialValue } });
}
document.body.dataset.ready = "true";
