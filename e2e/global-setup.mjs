// Copies the trunk build (bundle and demo styles) from ../dist into e2e/.standalone with relative paths,
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
  const js = files.find((name) => /^config-editor-demo-.*\.js$/.test(name));
  const wasm = files.find((name) => /^config-editor-demo-.*_bg\.wasm$/.test(name));
  const css = files.filter((name) => name.endsWith('.css'));
  if (!js || !wasm) {
    throw new Error('dist/ does not contain the editor bundle. Run `trunk build` first.');
  }
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  copyFileSync(join(dist, js), join(out, js));
  copyFileSync(join(dist, wasm), join(out, wasm));
  for (const name of css) copyFileSync(join(dist, name), join(out, name));
  const links = css.map((name) => `<link rel="stylesheet" href="./${name}">`).join('');
  writeFileSync(
    join(out, 'index.html'),
    `<!DOCTYPE html>
<html>
  <head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>YAML editor e2e</title>${links}</head>
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
