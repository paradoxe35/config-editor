// Builds the npm package: the release wasm through wasm-bindgen (--target web)
// into pkg/. Needs a wasm-bindgen CLI of the version in Cargo.lock: on PATH,
// in WASM_BINDGEN, or the one trunk caches.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const lock = readFileSync(join(root, 'Cargo.lock'), 'utf8');
const version = lock.match(/name = "wasm-bindgen"\nversion = "([^"]+)"/)[1];

function wasmBindgen() {
  const candidates = [
    process.env.WASM_BINDGEN,
    join(homedir(), '.cache', 'trunk', `wasm-bindgen-${version}`, 'wasm-bindgen'),
  ].filter(Boolean);
  for (const candidate of candidates) if (existsSync(candidate)) return candidate;
  try {
    const found = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' });
    if (found.includes(version)) return 'wasm-bindgen';
  } catch {}
  throw new Error(`wasm-bindgen ${version} not found: cargo install wasm-bindgen-cli --version ${version}`);
}

execFileSync(
  'cargo',
  ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'config-editor-js'],
  { cwd: root, stdio: 'inherit' },
);
execFileSync(
  wasmBindgen(),
  [
    '--target', 'web',
    '--out-dir', join(here, 'pkg'),
    '--out-name', 'config_editor',
    join(root, 'target', 'wasm32-unknown-unknown', 'release', 'config_editor_js.wasm'),
  ],
  { stdio: 'inherit' },
);
// Used before the module loads, the glue would fail on its first use of the
// module's exports with an internal message (reading
// `__wbindgen_add_to_stack_pointer` of undefined): until the load, they are
// a stand-in that says what to do.
{
  const glue = join(here, 'pkg', 'config_editor.js');
  let code = readFileSync(glue, 'utf8');
  const patch = (from, to) => {
    if (!code.includes(from)) throw new Error(`wasm-bindgen glue changed: ${JSON.stringify(from)} not found`);
    code = code.split(from).join(to);
  };
  patch(
    'let wasmModule, wasmInstance, wasm;',
    'const NOT_LOADED = new Proxy({}, { get() { throw new Error("config-editor: the WebAssembly module is not loaded: await loadConfigEditor() before using the editor or the tools"); } });\n' +
      'let wasmModule, wasmInstance, wasm = NOT_LOADED;',
  );
  patch('if (wasm !== undefined) return wasm;', 'if (wasm !== NOT_LOADED) return wasm;');
  writeFileSync(glue, code);
}
// The public types: wasm-bindgen also declares its own internals (the start
// hook, stream adapters) and `[Symbol.dispose]`, which needs `lib: esnext`.
{
  const types = join(here, 'pkg', 'config_editor.d.ts');
  const cleaned = readFileSync(types, 'utf8')
    .replace(/(?:\/\*\*(?:[^*]|\*(?!\/))*\*\/\n)?export class IntoUnderlying\w+ \{[\s\S]*?\n\}\n\n?/g, '')
    .replace(/(?:\/\*\*(?:[^*]|\*(?!\/))*\*\/\n)?export function start\(\): void;\n\n?/g, '')
    .replace(/^ *\[Symbol\.dispose\]\(\): void;\n/gm, '')
    .replace(/(?:\/\*\*(?:[^*]|\*(?!\/))*\*\/\n\n?)?export type ReadableStreamType = "bytes";\n\n?/g, '')
    // The module's exports are internals; only its memory is public.
    .replace(/export interface InitOutput \{[\s\S]*?\n\}/, 'export interface InitOutput {\n    readonly memory: WebAssembly.Memory;\n}');
  writeFileSync(types, cleaned);
}
// Optional: shrink further with binaryen's wasm-opt when it is installed. Its
// features are listed, without SIMD (the wasm loads in browsers without it);
// a wasm-opt that is installed but fails fails the build.
let hasWasmOpt = true;
try {
  execFileSync('wasm-opt', ['--version'], { stdio: 'ignore' });
} catch {
  hasWasmOpt = false;
  console.log('wasm-opt not found: skipping the optional size pass');
}
if (hasWasmOpt) {
  const wasm = join(here, 'pkg', 'config_editor_bg.wasm');
  // The features Rust's wasm32 target turns on by default (multivalue
  // included), and no others: without SIMD the wasm loads in every browser.
  // CI and the release use the same pinned wasm-opt (scripts/install-binaryen.sh).
  execFileSync('wasm-opt', ['-Os', '--enable-bulk-memory', '--enable-nontrapping-float-to-int',
    '--enable-sign-ext', '--enable-mutable-globals', '--enable-reference-types', '--enable-multivalue',
    wasm, '-o', wasm], { stdio: 'inherit' });
}
console.log('built js/config-editor/pkg');
