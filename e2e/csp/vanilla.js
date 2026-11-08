// The JS package under a strict CSP (see options.js for the query).
import { ConfigDocuments, ConfigEditor, loadConfigEditor, themes } from '/js/config-editor/index.js';
import { documentsOptions, editorOptions, initialValue, view, WASM } from './options.js';

await loadConfigEditor(WASM);
window.themeNames = themes().map(({ name }) => name);
const host = document.getElementById('editor');

if (view === 'documents') {
  host.className = 'csp-page__documents';
  const options = documentsOptions();
  let files = options.documents;
  window.documentsView = new ConfigDocuments(host, {
    ...options,
    onChange: (id, value) => {
      files = files.map((file) => (file.id === id ? { ...file, value } : file));
      window.documentsView.update({ documents: files });
    },
  });
} else {
  window.editor = new ConfigEditor(host, { ...editorOptions(), value: initialValue });
}
document.body.dataset.ready = 'true';
