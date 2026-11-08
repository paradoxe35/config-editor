// The React binding under a strict CSP (e2e/csp/react.html; the options and
// the query are e2e/csp/options.js).
import { createRoot } from "react-dom/client";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { ConfigDocuments, ConfigEditor } from "@pngwasi/config-editor-react";
import { documentsOptions, editorOptions, initialValue, view, WASM } from "../../e2e/csp/options.js";

await loadConfigEditor(WASM);
createRoot(document.getElementById("editor")!).render(
  view === "documents" ? (
    <ConfigDocuments className="csp-page__documents" {...documentsOptions()} />
  ) : (
    <ConfigEditor className="csp-page__editor" {...editorOptions()} defaultValue={initialValue} />
  ),
);
document.body.dataset.ready = "true";
