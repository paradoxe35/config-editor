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
// The public types: wasm-bindgen also declares its own internals (the start
// hook, stream adapters) and `[Symbol.dispose]`, which needs `lib: esnext`.
{
  const types = join(here, 'pkg', 'config_editor.d.ts');
  const cleaned = readFileSync(types, 'utf8')
    .replace(/(?:\/\*\*(?:[^*]|\*(?!\/))*\*\/\n)?export class IntoUnderlying\w+ \{[\s\S]*?\n\}\n\n?/g, '')
    .replace(/(?:\/\*\*(?:[^*]|\*(?!\/))*\*\/\n)?export function start\(\): void;\n\n?/g, '')
    .replace(/^ *\[Symbol\.dispose\]\(\): void;\n/gm, '');
  writeFileSync(types, cleaned);
}
// Optional: shrink further with binaryen's wasm-opt when it is installed.
try {
  const wasm = join(here, 'pkg', 'config_editor_bg.wasm');
  execFileSync('wasm-opt', ['-Os', '--enable-bulk-memory', '--enable-nontrapping-float-to-int',
    '--enable-sign-ext', '--enable-mutable-globals', '--enable-reference-types', wasm, '-o', wasm], { stdio: 'inherit' });
} catch {
  console.log('wasm-opt not found: skipping the optional size pass');
}
console.log('built js/config-editor/pkg');
