// The options every CSP page mounts with: `styled: false` (the page links
// preset.css), custom colors, the fixture's schema, host problems and
// secrets. Query: `view=documents` for ConfigDocuments, `plain` for an empty
// document (no locked field in the way), `edit=json` to edit as JSON.
import { COLORS, HOST_DIAGNOSTICS, SCHEMA, SECRETS, VALUE } from './fixture.js';

const query = new URLSearchParams(location.search);

export const view = query.get('view') ?? 'editor';

export const WASM = { module_or_path: '/js/config-editor/pkg/config_editor_bg.wasm' };

const settings = { styled: false, colors: { ...COLORS, 'tab-active-bg': '#203040' }, showFooter: true };

export const initialValue = query.has('plain') ? '' : VALUE;

export function editorOptions() {
  return {
    ...settings,
    language: 'yaml',
    schema: SCHEMA,
    fileName: 'burrow.yaml',
    showToolbar: true,
    hostDiagnostics: HOST_DIAGNOSTICS,
    secrets: SECRETS,
    ...(query.get('edit') ? { editAs: query.get('edit') } : {}),
  };
}

export function documentsOptions() {
  return {
    ...settings,
    documents: [
      { id: 'main', fileName: 'burrow.yaml', value: VALUE, schema: SCHEMA, hostDiagnostics: HOST_DIAGNOSTICS, secrets: SECRETS, closable: false },
      { id: 'limits', fileName: 'limits.toml', value: '[limits]\nmax_pending = 512\n' },
    ],
    // The "+" and the close buttons are drawn; the page keeps its two files.
    onAdd: () => {},
    onClose: () => {},
  };
}
