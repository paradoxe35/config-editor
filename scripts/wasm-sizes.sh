#!/usr/bin/env bash
# The wasm size of a host using the editor, per Cargo feature set: builds the
# crate's `wasm_size` example (a small host mounting one editor, so code a
# host does not use is dropped as in a real app) for wasm32 in release, runs
# wasm-bindgen as a bundler would, then wasm-opt -Os when it is installed
# (as js/config-editor/build.mjs does), and gzips it (-9).
#
#   scripts/wasm-sizes.sh           # print the table
#   scripts/wasm-sizes.sh --write   # also rewrite the crate README's table
#
# Not a CI check: sizes move with the Rust toolchain and the dependencies.
# Needs the wasm32-unknown-unknown target and the wasm-bindgen CLI of the
# version in Cargo.lock (on PATH, in WASM_BINDGEN, or the one trunk caches).
# Takes a few minutes: one release build per row.
set -euo pipefail
cd "$(dirname "$0")/.."

readme=crates/config-editor-leptos/README.md
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

version=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"$/\1/p')
bindgen=${WASM_BINDGEN:-$HOME/.cache/trunk/wasm-bindgen-$version/wasm-bindgen}
if [[ ! -x "$bindgen" ]]; then
  bindgen=wasm-bindgen
  if ! "$bindgen" --version 2>/dev/null | grep -q "$version"; then
    echo "wasm-bindgen $version not found: cargo install wasm-bindgen-cli --version $version" >&2
    exit 1
  fi
fi
optimized="without wasm-opt (not installed)"
if command -v wasm-opt >/dev/null; then optimized="after wasm-opt -Os"; fi

# Optional features (all on by default), each priced by building without it.
optional=(search folding diff protect)
languages_all=(yaml json toml hcl xml)

# Prints the gzip -9 size in bytes of the example built with `$1` (features,
# comma-separated, on top of none).
size_of() {
  cargo build --quiet --release --target wasm32-unknown-unknown \
    -p config-editor-leptos --example wasm_size --no-default-features --features "$1"
  rm -rf "$work/out"
  "$bindgen" --target web --out-dir "$work/out" 2>/dev/null \
    target/wasm32-unknown-unknown/release/examples/wasm_size.wasm
  local wasm="$work/out/wasm_size_bg.wasm"
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
      --enable-mutable-globals --enable-reference-types "$wasm" -o "$wasm"
  fi
  gzip -9 -c "$wasm" | wc -c
}

join() { local IFS=,; echo "$*"; }
kb() { awk -v bytes="$1" 'BEGIN { printf "%.0f KB", bytes / 1000 }'; }
signed_kb() { awk -v bytes="$1" 'BEGIN { printf "%+.0f KB", bytes / 1000 }'; }

all=$(join schema-validation "${languages_all[@]}" "${optional[@]}")
rows=()
row() { rows+=("| $1 | $2 |"); echo "  $1: $2" >&2; }

echo "Measuring (gzip -9, $optimized)…" >&2
full=$(size_of "$all")
row "Everything (the default features)" "$(kb "$full")"
row "YAML + JSON, every optional feature, no \`schema-validation\`" \
  "$(kb "$(size_of "$(join yaml json "${optional[@]}")")")"
row "YAML + JSON, every optional feature, with \`schema-validation\`" \
  "$(kb "$(size_of "$(join schema-validation yaml json "${optional[@]}")")")"
row "Minimal: YAML only, no optional feature, no \`schema-validation\`" "$(kb "$(size_of yaml)")"
for feature in schema-validation "${optional[@]}"; do
  without=$(join $(tr ',' '\n' <<<"$all" | grep -vx "$feature"))
  row "Saved by leaving out \`$feature\` (from everything)" \
    "$(signed_kb $(( $(size_of "$without") - full )))"
done

table=$(
  echo "| Build | wasm, gzip -9 |"
  echo "| ----- | ------------- |"
  printf '%s\n' "${rows[@]}"
)
note="Measured $(date +%Y-%m-%d) with $(rustc --version | cut -d' ' -f1-2), $optimized, by \`scripts/wasm-sizes.sh\`."
echo
echo "$table"
echo
echo "$note"

if [[ "${1:-}" == "--write" ]]; then
  block=$(printf '%s\n\n%s' "$table" "$note")
  BLOCK="$block" perl -0pi -e \
    's/(<!-- wasm-sizes:start -->\n).*?(\n<!-- wasm-sizes:end -->)/$1$ENV{BLOCK}$2/s' "$readme"
  echo "rewrote the table in $readme" >&2
fi
