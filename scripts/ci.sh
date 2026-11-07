#!/usr/bin/env bash
# The CI checks, one place for CI (.github/workflows/ci.yml runs each step)
# and for local runs:
#
#   scripts/ci.sh                 # every Rust and docs step
#   scripts/ci.sh languages      # one step (see STEPS below)
#   scripts/ci.sh rust browser   # the steps of a CI job
#
# Locally, set CI_MEMORY_LIMIT (bytes, e.g. 4000000000) to run the test
# binaries under `prlimit --as`, and CARGO_BUILD_JOBS to limit the build.
set -euo pipefail
cd "$(dirname "$0")/.."

# Runs a test command, under a memory limit when CI_MEMORY_LIMIT is set:
# compiled first, so the limit applies to the tests and not to rustc.
run_tests() {
  cargo "$@" --no-run --quiet
  if [[ -n "${CI_MEMORY_LIMIT:-}" ]]; then
    prlimit --as="$CI_MEMORY_LIMIT" cargo "$@"
  else
    cargo "$@"
  fi
}

step_fmt() {
  cargo fmt --all --check
  (cd js/config-editor && cargo fmt --check)
}

step_clippy() {
  cargo clippy --all-targets -- -D warnings
  cargo clippy -p config-editor-js --target wasm32-unknown-unknown -- -D warnings
  (cd crates/config-editor-leptos &&
    cargo clippy --all-targets --no-default-features --features yaml,json,toml,hcl,xml -- -D warnings)
}

step_test() {
  run_tests test -p config-editor-leptos
  # Also checks that the settings' generated TypeScript type and README tables
  # match `EditorSettings` (UPDATE_GENERATED=1 rewrites them).
  run_tests test -p config-editor-js
}

# Each language (and schema validation) built and tested on its own, so a
# test or helper that needs another feature is gated by it.
step_languages() {
  (cd crates/config-editor-leptos &&
    for features in yaml json toml hcl xml schema-validation; do
      echo "== --features $features"
      cargo clippy --lib --tests --no-default-features --features "$features" -- -D warnings
      run_tests test --lib --no-default-features --features "$features"
    done)
}

# Every language without schema validation (completions and hover from the
# schema alone).
step_minimal() {
  (cd crates/config-editor-leptos && run_tests test --no-default-features --features yaml,json,toml,hcl,xml)
}

# Generated docs are up to date (no build needed).
step_docs() {
  node scripts/readme-toc.mjs --check
  node e2e/screenshots.mjs --check
}

# The npm packages, their README examples' types, the demo and the e2e suite
# (needs trunk, wasm-bindgen and Playwright's Chromium).
step_browser() {
  (cd js && npm ci && npm run build && npm run typecheck)
  trunk build
  # Workers: playwright.config.mjs (1 in CI, E2E_WORKERS or 4 locally).
  (cd e2e && npm ci && npx playwright test)
}

STEPS=(fmt clippy test languages minimal docs)
case "${1:-all}" in
  all) set -- "${STEPS[@]}" ;;
  rust) set -- fmt clippy test languages minimal ;;
esac
for step in "$@"; do
  echo "### $step"
  "step_$step"
done
