#!/usr/bin/env bash
# Installs a pinned Binaryen (wasm-opt) for the npm package's size pass, the
# same in CI and in the release: the distributions' binaryen (Ubuntu's 108) is
# too old for wasm-bindgen's output and writes a wasm that no longer loads.
#
#   scripts/install-binaryen.sh [dir]   # default: $RUNNER_TEMP or /tmp
#
# On GitHub Actions it adds wasm-opt to the PATH of the next steps.
set -euo pipefail

VERSION=version_123
SHA256=e959f2170af4c20c552e9de3a0253704d6a9d2766e8fdb88e4d6ac4bae9388fe

dir=${1:-${RUNNER_TEMP:-/tmp}}/binaryen
mkdir -p "$dir"
archive="$dir/binaryen.tar.gz"
curl -fsSL --retry 5 --retry-delay 5 --retry-connrefused -o "$archive" \
  "https://github.com/WebAssembly/binaryen/releases/download/$VERSION/binaryen-$VERSION-x86_64-linux.tar.gz"
echo "$SHA256  $archive" | sha256sum -c -
tar -xzf "$archive" -C "$dir"
bin="$dir/binaryen-$VERSION/bin"
"$bin/wasm-opt" --version
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$bin" >> "$GITHUB_PATH"
fi
