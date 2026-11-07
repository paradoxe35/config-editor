import { flushSync, mount, unmount } from "svelte";
import { loadConfigEditor } from "@pngwasi/config-editor-svelte";
import App from "./svelte/App.svelte";

// The bundle does not sit next to the package's wasm, so say where it is.
// `?late-load` loads it only after the component's own load (from its
// default URL, which fails here) has failed; `?double-load` passes a wrong
// URL, then the right one, while the first still loads.
const query = new URLSearchParams(location.search);
const wasm = { module_or_path: "../config-editor/pkg/config_editor_bg.wasm" };
if (query.has("double-load")) void loadConfigEditor({ module_or_path: "../config-editor/pkg/missing.wasm" }).catch(() => {});
const ready = query.has("late-load")
  ? new Promise((resolve) => setTimeout(resolve, 800)).then(() => loadConfigEditor(wasm))
  : loadConfigEditor(wasm);

// For tests that mount components of their own.
Object.assign(window, { Svelte: { flushSync, mount, unmount } });
const schema = await (await fetch("../../schema.json")).text();
mount(App, { target: document.getElementById("root")!, props: { schema, ready } });
document.body.dataset.ready = "true";
