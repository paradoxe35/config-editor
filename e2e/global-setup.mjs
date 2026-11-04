// Copies the trunk build from ../dist into e2e/.standalone with relative paths,
// so headless Chrome can load the wasm bundle from file:// without a web server.
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, '..', 'dist');
const out = join(here, '.standalone');

export default function globalSetup() {
  if (!existsSync(dist)) {
    throw new Error('dist/ is missing. Run `trunk build` (or `npm run build` in e2e/) first.');
  }
  const files = readdirSync(dist);
  const js = files.find((name) => /^leptos-yaml-editor-.*\.js$/.test(name));
  const wasm = files.find((name) => /^leptos-yaml-editor-.*_bg\.wasm$/.test(name));
  if (!js || !wasm) {
    throw new Error('dist/ does not contain the editor bundle. Run `trunk build` first.');
  }
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  copyFileSync(join(dist, js), join(out, js));
  copyFileSync(join(dist, wasm), join(out, wasm));
  writeFileSync(
    join(out, 'index.html'),
    `<!DOCTYPE html>
<html>
  <head><meta charset="utf-8"><title>YAML editor e2e</title></head>
  <body style="margin:0">
  <script type="module">
  import init, * as bindings from './${js}';
  const wasm = await init({ module_or_path: './${wasm}' });
  window.wasmBindings = bindings;
  dispatchEvent(new CustomEvent("TrunkApplicationStarted", { detail: { wasm } }));
  </script>
  </body>
</html>
`,
  );
}
