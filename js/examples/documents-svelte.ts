import { mount } from "svelte";
import { loadConfigEditor } from "@pngwasi/config-editor";
import Documents from "./svelte/Documents.svelte";

// The bundle does not sit next to the package's wasm, so say where it is.
await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
const schema = await (await fetch("../../schema.json")).text();
mount(Documents, { target: document.getElementById("root")!, props: { schema } });
document.body.dataset.ready = "true";
