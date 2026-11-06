// For the e2e tests: a load that fails (a wrong .wasm URL) can be retried.
import { loadConfigEditor } from "@pngwasi/config-editor-react";

const outcome: string[] = [];
try {
  await loadConfigEditor({ module_or_path: "./does-not-exist.wasm" });
  outcome.push("loaded");
} catch {
  outcome.push("failed");
}
try {
  await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
  outcome.push("loaded");
} catch {
  outcome.push("failed");
}
document.body.dataset.result = outcome.join(",");
