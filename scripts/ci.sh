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
#
# CI_LANES=N (default 1) runs the heavy steps in N lanes at once, each with
# its own target directory (target/lane-2, … beside the usual one), so their
# builds don't wait on one Cargo lock: faster on a machine with idle cores,
# at the cost of disk (several GB per extra lane). Each lane's output is
# printed, prefixed, when it ends; the run fails if any lane fails.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

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

# Each language (and schema validation) built and tested on its own, with no
# optional feature, so a test or helper that needs another feature is gated
# by it.
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

# The optional features (search, diff, folding, protect): each one off with
# everything else on, then all of them off together (YAML and JSON), so
# shared code never needs one and a test that does is gated by it. The
# end-to-end suite runs only with the defaults (`browser`). Without debug
# info: five configurations of test binaries are otherwise several GB.
OPTIONAL=(search diff folding protect)
step_optional() {
  (cd crates/config-editor-leptos &&
    export CARGO_PROFILE_DEV_DEBUG=0 &&
    for off in "${OPTIONAL[@]}"; do
      local on=(schema-validation yaml json toml hcl xml)
      for feature in "${OPTIONAL[@]}"; do
        [[ "$feature" != "$off" ]] && on+=("$feature")
      done
      local features
      features=$(IFS=,; echo "${on[*]}")
      echo "== without $off"
      cargo clippy --all-targets --no-default-features --features "$features" -- -D warnings
      run_tests test --no-default-features --features "$features"
    done
    echo "== without any (yaml, json)"
    cargo clippy --all-targets --no-default-features --features yaml,json -- -D warnings
    run_tests test --no-default-features --features yaml,json)
}

# Generated docs are up to date (no build needed).
step_docs() {
  node scripts/readme-toc.mjs --check
  node scripts/markdown-tables.mjs
  # Markdown is formatted by Prettier (the version pinned in js/package.json),
  # as an editor's format-on-save does; the generators write its output.
  if [[ ! -x js/node_modules/.bin/prettier ]]; then
    echo "run 'cd js && npm ci' first (Prettier checks the Markdown)" >&2
    return 1
  fi
  git ls-files '*.md' | xargs js/node_modules/.bin/prettier --check --log-level warn
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

# The steps of one lane, in order, with that lane's output prefixed.
run_lane() {
  local lane=$1
  shift
  if [[ "$lane" -gt 1 ]]; then
    export CARGO_TARGET_DIR="$ROOT/target/lane-$lane"
  fi
  for step in "$@"; do
    echo "### $step"
    "step_$step"
  done
}

# Runs `steps` in CI_LANES lanes: the quick ones first, then the heavy ones
# dealt to lanes heaviest first, each lane in the background.
run_in_lanes() {
  local lanes=$1
  shift
  local quick=() heavy=()
  for step in "$@"; do
    case "$step" in
      fmt | docs) quick+=("$step") ;;
      *) heavy+=("$step") ;;
    esac
  done
  for step in "${quick[@]}"; do
    echo "### $step"
    "step_$step"
  done
  # Heaviest first (roughly: what each step builds and runs).
  local order=(optional languages test clippy minimal browser)
  local sorted=()
  for step in "${order[@]}"; do
    for wanted in "${heavy[@]}"; do
      [[ "$wanted" == "$step" ]] && sorted+=("$step")
    done
  done
  local -a lane_steps
  local index=0
  for step in "${sorted[@]}"; do
    local lane=$((index % lanes + 1))
    lane_steps[lane]="${lane_steps[lane]:-} $step"
    index=$((index + 1))
  done
  local logs pids=() failed=0
  logs=$(mktemp -d)
  for lane in "${!lane_steps[@]}"; do
    # shellcheck disable=SC2086 # the lane's steps, split on spaces
    (run_lane "$lane" ${lane_steps[lane]}) >"$logs/$lane" 2>&1 &
    pids[lane]=$!
  done
  for lane in "${!pids[@]}"; do
    if wait "${pids[lane]}"; then status=ok; else status=FAILED; failed=1; fi
    sed "s/^/[lane $lane] /" "$logs/$lane"
    echo "[lane $lane] ${status}:${lane_steps[lane]}"
  done
  rm -rf "$logs"
  return "$failed"
}

STEPS=(fmt clippy test languages minimal optional docs)
case "${1:-all}" in
  all) set -- "${STEPS[@]}" ;;
  rust) set -- fmt clippy test languages minimal optional ;;
esac
lanes=${CI_LANES:-1}
if [[ "$lanes" =~ ^[0-9]+$ && "$lanes" -gt 1 ]]; then
  run_in_lanes "$lanes" "$@"
else
  run_lane 1 "$@"
fi
