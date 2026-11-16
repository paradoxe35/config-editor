#!/usr/bin/env bash
# The random (seeded, shrinking) suites at release size: the one list of
# every random property, and the runner CI (.github/workflows/sweep.yml) and
# local sweeps share.
#
#   scripts/sweep.sh --list                       # the properties, with release cases
#   scripts/sweep.sh --check                      # every seeded test is listed (scripts/ci.sh docs)
#   scripts/sweep.sh --build [DIR]                # build the test binaries into DIR (default target/sweep)
#   scripts/sweep.sh --seed 7 [--scale 20] [--only edit,SCHEMA_FUZZ] [--group edit]
#                                                 # run properties from the built binaries
#   scripts/sweep.sh --browser vue --seed 7 [--runs 100]
#                                                 # the browser fuzz on one page (needs the npm build and dist/)
#   scripts/sweep.sh --matrix 7,8 [--only LIST]   # CI's matrix: one job per group x seed (JSON)
#   scripts/sweep.sh --pages [LIST]               # the browser fuzz pages (JSON; LIST checked against them)
#   scripts/sweep.sh --seeds 7,8                  # the seeds, checked (JSON)
#   scripts/sweep.sh --summary DIR                # a Markdown table of the results under DIR
#
# A property runs ci_cases x scale cases (scale 20 is the release size) from
# the given seed. `--only` takes a comma list of property names, groups and
# env prefixes (empty or `all`: everything). Results go to --out DIR (default
# target/sweep/out): logs/<name>.<seed>.log and results/<name>.<seed>.tsv.
# SWEEP_TIMEOUT caps each property (seconds, default 3600); SWEEP_MEMORY its
# address space (bytes, default 8000000000).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD

# The properties. Columns: name, group (the CI job it runs in, so small ones
# share a runner), binary (lib: the config-editor-leptos lib tests;
# host_sync: config-editor-js's host_sync test), the cases and seed env
# variables, the CI case count (the test's own default), the test path (its
# last segments), and `ignored` for a test that runs only on request.
# Groups are sized to a few minutes each on a CI runner at scale 20.
PROPERTIES='
edit-commands          edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    800  commands::comment_and_line_commands_never_panic_and_keep_the_selection_in_the_text
edit-commands-custom   edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    800  commands::comment_and_line_commands_keep_the_selection_in_the_text_in_custom_languages
edit-folds             edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  folds::folds_find_and_go_to_line_follow_random_edits
edit-folds-custom      edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  folds::folds_find_and_go_to_line_follow_random_edits_in_custom_languages
edit-convert           edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    800  convert::conversions_keep_values_and_numbers_or_list_the_loss
edit-valid             edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    600  valid::edit_commands_keep_documents_valid_under_random_selections
edit-hooks             edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    60   hooks::language_hooks_never_panic_on_half_typed_text_of_wide_characters
edit-hooks-custom      edit     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    60   hooks::language_hooks_never_panic_on_half_typed_text_in_custom_languages
edit-replace           undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    800  replace::replace_all_equals_the_model_under_random_patterns
edit-replace-custom    undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    800  replace::replace_all_equals_the_model_in_custom_languages
edit-undo              undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  undo::undo_all_and_redo_all_give_back_the_texts_under_random_edits
edit-undo-steps        undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  undo::undo_steps_follow_the_documented_grouping_under_random_edits
edit-undo-custom       undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  undo::undo_all_and_redo_all_give_back_the_texts_in_custom_languages
edit-undo-steps-custom undo     lib       FUZZ_EDIT_CASES    FUZZ_EDIT_SEED    300  undo::undo_steps_follow_the_documented_grouping_in_custom_languages
edit-leaks             undo     lib       FUZZ_LEAK_CASES    FUZZ_EDIT_SEED    100  leaks::a_random_edit_sequence_repeated_keeps_no_memory ignored
hostile-linear         hostile  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 24   linear::work_on_hostile_text_grows_linearly
hostile-endings        hostile  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 150  endings::line_endings_and_the_byte_order_mark_survive_editing
hostile-live           hostile  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 120  live::props_changed_live_agree_with_a_fresh_editor
hostile-changes        hostile  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 300  changes::changes_between_random_texts_apply_and_revert
hostile-diff-cost      hostile  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 16   changes::the_diff_of_a_large_text_with_few_changes_grows_with_the_text
hostile-reading        reading  lib       FUZZ_HOSTILE_CASES FUZZ_HOSTILE_SEED 200  reading::every_reading_of_hostile_text_stays_inside_it
schema-edit-as         schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  250  edit_as::values_typed_in_an_edit_as_view_save_as_the_view_reads_them
schema-fill            schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  250  fill::accepted_keys_fill_what_their_schema_requires_with_a_stop_per_value
schema-text            schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  300  schema_text::every_spelling_of_a_schema_compiles_alike_and_keys_written_twice_are_refused
schema-secrets         schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  600  secrets::no_message_hover_or_suggestion_shows_a_secret
schema-suggestions     schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  200  suggestions::accepted_suggestions_add_no_problem_and_hovers_name_types_once
schema-validation      schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  400  validation::random_documents_get_problems_exactly_when_the_reference_finds_them_invalid
schema-work            schema   lib       SCHEMA_FUZZ_CASES  SCHEMA_FUZZ_SEED  12   work::work_on_random_recursive_schemas_grows_with_the_document
host-sync-editor       schema   host_sync HOST_SYNC_CASES    HOST_SYNC_SEED    300  random_host_sync_keeps_a_configeditors_locks
host-sync-documents    schema   host_sync HOST_SYNC_CASES    HOST_SYNC_SEED    300  random_host_sync_keeps_a_documents_views_locks
protect                protect  lib       PROTECT_FUZZ_CASES PROTECT_FUZZ_SEED 400  protect_fuzz::random_edits_keep_every_protection_and_take_ordinary_edits
protect-custom         protect  lib       PROTECT_FUZZ_CASES PROTECT_FUZZ_SEED 2000 protect_fuzz::random_edits_keep_every_protection_and_take_ordinary_edits_in_custom_languages
change-inverse         edit     lib       CHANGE_CASES       CHANGE_SEED       2000 change_tests::a_change_and_its_inverse_give_the_text_back
change-compose         edit     lib       CHANGE_CASES       CHANGE_SEED       2000 change_tests::a_composed_change_makes_what_the_two_make_in_turn
change-rebase          edit     lib       CHANGE_CASES       CHANGE_SEED       2000 change_tests::two_changes_rebased_over_each_other_meet
change-map             edit     lib       CHANGE_CASES       CHANGE_SEED       2000 change_tests::positions_map_in_order_and_kept_text_stays_on_its_text
change-diffed          edit     lib       CHANGE_CASES       CHANGE_SEED       2000 change_tests::a_change_read_off_two_texts_makes_the_second
protect-reach          protect  lib       REACH_CASES        REACH_SEED        400  reach_tests::the_fast_path_answers_as_the_full_rule_on_random_edits
save                   save     lib       FUZZ_CASES         FUZZ_SEED         40   fuzz_save::random_sessions_keep_the_save_promise
'
# The browser fuzz's seed variable (its runs and pages come from the spec).
BROWSER_SEED_VAR=FUZZ_BROWSER_SEED
# Where the seeded tests live (searched by --check).
SOURCES=(crates/config-editor-leptos/src js/config-editor/tests)

properties() { grep -v '^[[:space:]]*$' <<<"$PROPERTIES"; }

# Whether property `$1` (its name, group and seed variable's prefix in $2 $3)
# is selected by the comma list `$4`.
selected() {
  local name=$1 group=$2 prefix=$3 list=$4
  [[ -z "$list" || "$list" == all ]] && return 0
  local item
  local -a items
  IFS=, read -ra items <<<"$list"
  for item in "${items[@]}"; do
    item=${item// /}
    [[ "$item" == "$name" || "$item" == "$group" || "$item" == "$prefix" ]] && return 0
  done
  return 1
}

prefix_of() { echo "${1%_SEED}"; }

cmd_list() {
  local scale=${1:-20}
  printf '%-24s %-8s %-10s %8s  %s\n' name group binary cases test
  properties | while read -r name group bin cases_var seed_var ci test _; do
    printf '%-24s %-8s %-10s %8s  %s (%s, %s)\n' "$name" "$group" "$bin" $((ci * scale)) "$test" "$cases_var" "$seed_var"
  done
}

# The seeded tests in the sources: every #[test] whose first lines run a
# random property (a case count from the env or a runner). Prints
# `file fn_name` lines.
seeded_tests() {
  # shellcheck disable=SC2016 # an awk program, not shell
  find "${SOURCES[@]}" -name '*.rs' -print0 | xargs -0 awk '
    FNR == 1 { pending = 0; left = 0 }
    /^[ \t]*#\[test\]/ { pending = 1; next }
    pending && /^[ \t]*#\[/ { next }
    pending && /fn [a-z0-9_]+\(/ {
      name = $0; sub(/.*fn /, "", name); sub(/\(.*/, "", name)
      pending = 0; left = 15; found = 0; next
    }
    { pending = 0 }
    left > 0 {
      left--
      if (!found && ($0 ~ /cases\([0-9]/ || $0 ~ /cases: [1-9]/ || $0 ~ /explore_and_report\(/ || $0 ~ /check_view\(/ || $0 ~ /_CASES"/)) {
        print FILENAME, name; found = 1
      }
    }'
}

cmd_check() {
  local failed=0 listed seeds
  listed=$(properties | awk '{ n = split($7, parts, "::"); print parts[n] }')
  seeds=$( (properties | awk '{ print $5 }'; echo "$BROWSER_SEED_VAR") | sort -u)
  # Every seeded test in the sources is a listed property.
  while read -r file name; do
    if ! grep -qx "$name" <<<"$listed"; then
      echo "$file: the random test $name is not listed in scripts/sweep.sh (PROPERTIES)" >&2
      failed=1
    fi
  done < <(seeded_tests)
  # Every seed variable the sources read belongs to a listed suite (a new
  # suite has a new one, even when its tests run through a new helper).
  while read -r var; do
    if ! grep -qx "$var" <<<"$seeds"; then
      echo "the seed variable $var belongs to no suite listed in scripts/sweep.sh (PROPERTIES)" >&2
      failed=1
    fi
  done < <(grep -rhoE '\b[A-Z][A-Z0-9_]*_SEED\b' "${SOURCES[@]}" e2e/tests --include='*.rs' --include='*.mjs' | sort -u)
  # Every listed test still exists.
  local found
  found=$(seeded_tests | awk '{ print $2 }')
  while read -r name; do
    if ! grep -qx "$name" <<<"$found"; then
      echo "scripts/sweep.sh lists $name, which is no seeded test in the sources (renamed?)" >&2
      failed=1
    fi
  done <<<"$listed"
  [[ "$failed" == 0 ]] && echo "sweep list: $(wc -l <<<"$listed") random properties, all listed"
  return "$failed"
}

# Builds the lib tests and the host_sync test and copies them to DIR.
cmd_build() {
  local dir=${1:-target/sweep}
  mkdir -p "$dir"
  local spec bin
  for spec in "lib:-p config-editor-leptos --lib" "host_sync:-p config-editor-js --test host_sync"; do
    bin=${spec%%:*}
    local path
    # shellcheck disable=SC2086 # the cargo arguments, split on spaces
    path=$(cargo test ${spec#*:} --no-run --message-format=json |
      sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -1)
    [[ -n "$path" ]] || { echo "no test binary for $bin" >&2; return 1; }
    cp "$path" "$dir/$bin"
    echo "$dir/$bin <- $path"
  done
}

json_array() {
  local first=1 item
  printf '['
  for item in "$@"; do
    [[ $first == 1 ]] || printf ','
    printf '%s' "$item"
    first=0
  done
  printf ']\n'
}

# The comma list of seeds `$1`, one per line, each checked.
seed_list() {
  local seed
  local -a parts
  IFS=, read -ra parts <<<"$1"
  [[ ${#parts[@]} -gt 0 ]] || { echo "no seed given" >&2; return 1; }
  for seed in "${parts[@]}"; do
    seed=${seed// /}
    [[ "$seed" =~ ^[0-9]+$ ]] || { echo "seed '$seed' is not a whole number" >&2; return 1; }
    echo "$seed"
  done
}

cmd_seeds() {
  local list items=() seed
  list=$(seed_list "$1")
  for seed in $list; do items+=("\"$seed\""); done
  json_array "${items[@]}"
}

cmd_matrix() {
  local seeds=$1 only=$2 items=() groups seed group list
  list=$(seed_list "$seeds")
  groups=$(properties | while read -r name group _ _ seed_var _; do
    if selected "$name" "$group" "$(prefix_of "$seed_var")" "$only"; then echo "$group"; fi
  done | awk '!seen[$0]++')
  [[ -n "$groups" ]] || { echo "--only $only selects no property" >&2; return 1; }
  for seed in $list; do
    for group in $groups; do
      items+=("{\"group\":\"$group\",\"seed\":\"$seed\"}")
    done
  done
  json_array "${items[@]}"
}

all_pages() {
  node --input-type=module -e "const { PAGES } = await import('$ROOT/e2e/tests/fuzz/pages.mjs'); console.log(Object.keys(PAGES).join(' '));"
}

cmd_pages() {
  local list=${1:-all} pages items=() page
  pages=$(all_pages)
  if [[ -n "$list" && "$list" != all ]]; then
    IFS=, read -ra wanted <<<"$list"
    for page in "${wanted[@]}"; do
      page=${page// /}
      grep -qw -- "$page" <<<"$pages" || { echo "no browser fuzz page '$page' (pages: $pages)" >&2; return 1; }
      items+=("\"$page\"")
    done
  else
    for page in $pages; do items+=("\"$page\""); done
  fi
  json_array "${items[@]}"
}

# Writes one result line: name group seed status seconds cases.
record() {
  local out=$1 name=$2 group=$3 seed=$4 code=$5 seconds=$6 cases=$7 status
  case "$code" in
    0) status=pass ;;
    124) status=timeout ;;
    *) status=fail ;;
  esac
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$group" "$seed" "$status" "$seconds" "$cases" >"$out/results/$name.$seed.tsv"
  echo "$name seed $seed: $status in ${seconds}s ($cases cases)"
}

cmd_run() {
  local seed=$1 scale=$2 only=$3 group_wanted=$4 bins=$5 out=$6 failed=0 ran=0
  mkdir -p "$out/logs" "$out/results"
  bins=$(cd "$bins" && pwd)
  local timeout_s=${SWEEP_TIMEOUT:-3600} memory=${SWEEP_MEMORY:-8000000000}
  while read -r name group bin cases_var seed_var ci test flags; do
    [[ -n "$group_wanted" && "$group" != "$group_wanted" ]] && continue
    selected "$name" "$group" "$(prefix_of "$seed_var")" "$only" || continue
    local exe="$bins/$bin" cases=$((ci * scale)) log="$out/logs/$name.$seed.log"
    [[ -x "$exe" ]] || { echo "no test binary $exe: run scripts/sweep.sh --build first" >&2; return 1; }
    local full
    full=$("$exe" --list 2>/dev/null | sed -n "s/^\(\(.*::\)\{0,1\}$test\): test$/\1/p" | head -1)
    if [[ -z "$full" ]]; then
      echo "no test $test in $exe" | tee "$log" >&2
      record "$out" "$name" "$group" "$seed" 1 0 "$cases"
      failed=1
      continue
    fi
    local extra=()
    [[ "$flags" == *ignored* ]] && extra=(--include-ignored)
    local start code=0
    start=$(date +%s)
    # The crate's directory, as cargo test runs it (tests read fixtures
    # relative to it).
    (cd "$(crate_dir "$bin")" &&
      env "$cases_var=$cases" "$seed_var=$seed" \
        timeout "$timeout_s" prlimit --as="$memory" \
        "$exe" "$full" --exact "${extra[@]}" --test-threads=1 --nocapture) >"$log" 2>&1 || code=$?
    record "$out" "$name" "$group" "$seed" "$code" $(($(date +%s) - start)) "$cases"
    [[ "$code" == 0 ]] || failed=1
    ran=$((ran + 1))
  done < <(properties)
  [[ "$ran" -gt 0 || "$failed" == 1 ]] || { echo "no property selected" >&2; return 1; }
  return "$failed"
}

crate_dir() {
  case "$1" in
    lib) echo "$ROOT/crates/config-editor-leptos" ;;
    host_sync) echo "$ROOT/js/config-editor" ;;
  esac
}

cmd_browser() {
  local page=$1 seed=$2 runs=$3 out=$4
  mkdir -p "$out/logs" "$out/results"
  local log="$out/logs/browser-$page.$seed.log" start code=0
  start=$(date +%s)
  (cd e2e && env FUZZ_BROWSER_RUNS="$runs" "$BROWSER_SEED_VAR=$seed" FUZZ_BROWSER_PAGES="$page" \
    npx playwright test --project=editor --no-deps --reporter=line tests/fuzz/browser-fuzz.spec.mjs) >"$log" 2>&1 || code=$?
  # The spec skips itself when the packages are not built: a run that drove
  # no sequence is a failure, not a pass.
  if [[ "$code" == 0 ]] && ! grep -q "\[fuzz\] $page: $runs sequences" "$log"; then
    echo "sweep: the browser fuzz ran no sequences on $page (packages not built?)" >>"$log"
    code=1
  fi
  record "$out" "browser-$page" browser "$seed" "$code" $(($(date +%s) - start)) "$runs"
  return "$code"
}

# The lines of a failed log that show the failure: a Rust test's panic
# message (its classes and shrunk repros), the browser fuzz's new classes
# and shrunk sequences, else the log's end.
failure_excerpt() {
  local log=$1
  [[ -f "$log" ]] || { echo "(no log)"; return; }
  local excerpt
  excerpt=$(grep -E '\[fuzz\].*(NEW|actions after|repro:)' "$log" | head -40 || true)
  [[ -n "$excerpt" ]] || excerpt=$(sed -n '/panicked at/,/^note: run with\|^failures:$/p' "$log" | head -80)
  # A timeout, a crash or a failure outside the properties: the log's end.
  [[ -n "$excerpt" ]] || excerpt=$(tail -30 "$log")
  echo "$excerpt"
}

cmd_summary() {
  local dir=$1
  local results
  results=$(find "$dir" -path '*/results/*.tsv' -exec cat {} + 2>/dev/null | sort || true)
  if [[ -z "$results" ]]; then
    echo "## Random suite sweep"
    echo
    echo "No results (every job failed before running a property)."
    return 1
  fi
  local seeds names
  seeds=$(cut -f3 <<<"$results" | awk '!seen[$0]++')
  # Rows in list order, then the browser pages.
  names=$( (properties | awk '{ print $1 }'; cut -f1 <<<"$results" | sed -n '/^browser-/p' | sort -u) |
    while read -r name; do if grep -q "^$name	" <<<"$results"; then echo "$name"; fi; done)
  local header="| suite |" rule="| --- |" seed
  for seed in $seeds; do header+=" seed $seed |"; rule+=" --- |"; done
  echo "## Random suite sweep"
  echo
  echo "$header"
  echo "$rule"
  local name row line status seconds cases failures=()
  for name in $names; do
    row="| $name |"
    for seed in $seeds; do
      line=$(awk -F'\t' -v n="$name" -v s="$seed" '$1 == n && $3 == s' <<<"$results")
      if [[ -z "$line" ]]; then
        row+=" - |"
        continue
      fi
      IFS=$'\t' read -r _ _ _ status seconds cases <<<"$line"
      case "$status" in
        pass) row+=" pass (${cases}, ${seconds}s) |" ;;
        *) row+=" **${status^^}** (${cases}, ${seconds}s) |"; failures+=("$name.$seed") ;;
      esac
    done
    echo "$row"
  done
  echo
  local total failed
  total=$(wc -l <<<"$results")
  failed=${#failures[@]}
  echo "$((total - failed)) of $total runs passed."
  local failure log
  for failure in "${failures[@]}"; do
    log=$(find "$dir" -path "*/logs/$failure.log" | head -1)
    echo
    echo "<details><summary>${failure%.*} at seed ${failure##*.}</summary>"
    echo
    echo '```'
    failure_excerpt "$log"
    echo '```'
    echo
    echo "</details>"
  done
  [[ "$failed" == 0 ]]
}

mode=run seed='' scale=20 only='' group='' bins=target/sweep out=target/sweep/out runs=100 page='' arg=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    --list | --check) mode=${1#--} ;;
    --build | --matrix | --pages | --seeds | --summary)
      mode=${1#--}
      if [[ $# -gt 1 && "$2" != --* ]]; then arg=$2; shift; fi
      ;;
    --browser) mode=browser page=$2; shift ;;
    --seed) seed=$2; shift ;;
    --scale) scale=$2; shift ;;
    --only) only=$2; shift ;;
    --group) group=$2; shift ;;
    --bins) bins=$2; shift ;;
    --out) out=$2; shift ;;
    --runs) runs=$2; shift ;;
    -h | --help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done
[[ "$scale" =~ ^[0-9]+$ && "$scale" -gt 0 ]] || { echo "--scale must be a positive whole number" >&2; exit 2; }

case "$mode" in
  list) cmd_list "$scale" ;;
  check) cmd_check ;;
  build) cmd_build "${arg:-target/sweep}" ;;
  matrix) cmd_matrix "${arg:?--matrix needs a comma list of seeds}" "$only" ;;
  pages) cmd_pages "${arg:-all}" ;;
  seeds) cmd_seeds "${arg:?--seeds needs a comma list of seeds}" ;;
  summary) cmd_summary "${arg:?--summary needs the results directory}" ;;
  browser)
    [[ -n "$seed" ]] || { echo "--browser needs --seed" >&2; exit 2; }
    cmd_browser "$page" "$seed" "$runs" "$out"
    ;;
  run)
    [[ -n "$seed" ]] || { echo "--seed is required (see --help)" >&2; exit 2; }
    cmd_run "$seed" "$scale" "$only" "$group" "$bins" "$out"
    ;;
esac
