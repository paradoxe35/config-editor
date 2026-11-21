# Engineering rules

Lessons from building a large, test-heavy project. These rules don't depend on
the project: copy this file into any repository and apply what fits. The
general rules come first. The Rust and web sections follow.

Each rule has a short reason. A rule without a reason gets broken the first
time it is in the way.

## Contents

- [1. Design](#1-design)
- [2. When to redesign](#2-when-to-redesign)
- [3. Promises and severity](#3-promises-and-severity)
- [4. Tests](#4-tests)
- [5. Random tests](#5-random-tests)
- [6. Fixing bugs](#6-fixing-bugs)
- [7. Releases](#7-releases)
- [8. Code style](#8-code-style)
- [9. Docs and generated files](#9-docs-and-generated-files)
- [10. Working with agents](#10-working-with-agents)
- [11. Machine, CI and git](#11-machine-ci-and-git)
- [12. Rust](#12-rust)
- [13. Rust in WebAssembly and UI frameworks](#13-rust-in-webassembly-and-ui-frameworks)
- [14. Web UI](#14-web-ui)
- [15. Security](#15-security)

## 1. Design

1. **Know, don't guess.** If code must work out afterwards what happened (by
   diffing two texts, or by reading half-typed input), it will guess wrong in
   some case. Record the fact when it happens instead: where a value came
   from, which range was protected, what an edit replaced.
2. **One source of truth per concern, with one owner.** A second copy "kept in
   sync" goes out of sync sooner or later. Store the fact once and derive the
   rest.
3. **Make wrong states impossible.** Choose types that cannot hold the bug: a
   newtype per unit (bytes, characters, UTF-16 units), an enum instead of two
   booleans, an atomic span instead of two loose ends.
4. **Parse, don't validate.** Turn input into one model at the edge, and have
   every feature query that model. When three features each walk the raw
   input their own way, they disagree.
5. **One door per kind of change.** Every edit goes through one pipeline,
   every write to the outside through one writer, every text cut through one
   safe function, all deferred work through one scheduler. Then a rule (undo,
   locks, safety checks) holds everywhere because nothing goes around it.
   Enforce the doors with lints or tests, not with review alone.
6. **One record of what changed.** Undo, position mapping and syncing should
   all read the same change record that the edit wrote, not re-compute it.
7. **A small core with hooks.** Shared code never names a specific format,
   vendor or plug-in. What differs is a hook with a default, so a new variant
   gets every feature for free. A new hook needs a reason the core can't
   express generically.
8. **A safe fallback beats a clever guess.** When the program can't do
   something exactly, it waits and says why. A visible "can't save yet,
   because…" is fine. A silent wrong result never is.
9. **Simple first.** Aim for a correct, simple design. Less code is a welcome
   result, never the goal.
10. **Optional features are modules with an inert stand-in.** A feature that
    can be turned off lives in one module. When it's off, a stand-in with the
    same hooks does nothing. Shared code calls the hooks and never checks the
    flag itself. Build and test every feature off in CI.
11. **One definition per setting, option or string.** Define it once, with its
    doc and default. Generate the types, tables and docs for other languages
    from that one definition.
12. **Prefer a mature, small library to hand-rolled code** for solved
    problems (diffing, text ranges, parsing). Write down which one, and why.
    Check that it fits your targets (size, platform, no unsupported CPU
    features).

## 2. When to redesign

1. **Group findings by root cause before fixing any of them.** Ask: is it the
   same kind of problem as before, in a new shape?
2. **A cause seen twice is a design flaw.** So is a fix that needs "one more
   special case". Redesign that part, with a tight scope. Otherwise it's a
   normal bug in a good design: fix it with a test.
3. **Redesign early.** Every patch on a bad design widens the surface the next
   bug hides in. Our costliest redesign came six rounds too late.
4. **Have a stop rule.** For example: if three test sweeps in a row find new
   serious problems in the same area, stop patching and redesign that area.
5. **One redesign lands at a time** (the "strangler fig" approach). Each one
   merges behind the full test suite before the next. Two redesigns don't
   change the same module at once unless they agree on an interface first.
6. **The tests are the specification.** A redesign keeps every behaviour
   test. It changes a test only for a listed, deliberate behaviour change.
7. **A redesign brings regressions. Plan for them.** Run the full random
   suites at scale right after it lands, and expect a few findings that come
   from it.
8. **Workarounds for upstream bugs come with a pinning test** that fails once
   upstream fixes the bug. That failure tells you the workaround can go.

## 3. Promises and severity

1. **Write down the product's promises**, numbered: "a locked value never
   changes", "a save never quietly writes something else", "typing is never
   lost", "no hangs or crashes". Tests, QA and release decisions are judged
   against these.
2. **Rank findings by the promise they break**, not by how they look. A
   silent wrong save is severe even if rare. A cosmetic glitch isn't, even if
   common.
3. **Make the safe outcome explicit.** "The save waits with a visible reason"
   is a defined, acceptable result. Document which outcomes are failures and
   which are safe fallbacks.

## 4. Tests

1. **Every fix gets a test that fails before the fix and passes after.** Run
   it both ways. A test that never failed proves nothing.
2. **Fix by class, not by instance.** A bug in one format or one case usually
   exists in its neighbours. Write the test as a matrix (every language ×
   every position × every input kind), and fix what the matrix finds.
3. **Name tests by behaviour**
   (`a_lock_sharing_its_line_lands_on_its_text`), not by ticket or round. The
   doc comment names the promise it guards when that isn't obvious.
4. **A check must be able to fail.** For guard tests (API parity, lint
   counts, memory budgets), add a test that proves the check fails when its
   target is broken.
5. **Use ratchets for slow migrations.** A test counts the old pattern per
   file against an allow-list, and counts may only go down. The migration
   then never goes backwards.
6. **Test speed as a curve, not a number.** A "linear work" test runs the same
   operation at several sizes and fails when the time grows faster than the
   input. A fixed time budget is flaky. A growth check is not.
7. **Test memory with soak tests.** Repeat mount, use and unmount many times,
   and check that memory stays flat.
8. **Run timed tests alone,** after the others, on a quiet machine.
9. **Cap every test run** with a time limit and a memory limit. A runaway
   test can take the whole machine down.
10. **Treat a flaky test as a bug.** Find out whether the product or the test
    is wrong. Don't just retry it.

## 5. Random tests

1. **Use seeded random tests wherever you can** (property-based, or
   model-based against a simple reference): a fixed seed, shrinking to a
   minimal case, and a printed repro that can be pasted as a regression test.
2. **Use a fixed budget in CI, plus scale runs before a release** (for
   example 20× the cases, from new seeds). A suite that only runs its default
   seed slowly stops finding things.
3. **Known classes go behind an allow-list that names the finding** or the
   documented rule. Remove the entry when the class is fixed. Anything not on
   the list fails the run.
4. **A class is fixed only when it's gone at several seeds,** including at
   least one never used before. A fix checked on one seed often covers only
   that shape.
5. **Check every failure for three possible sources:** the product, the
   oracle (the test's checker) or the harness (the test's driver, such as a
   timing race). Decide with evidence (a trace or a log), then fix the one
   that's wrong. In our sweeps about half of the failures were oracle or
   harness bugs.
6. **Keep the oracle independent.** It may ask the product for recorded facts
   (where a value came from), but it judges the result itself against the
   written rules. It must never just ask the product whether it was right.
7. **An oracle that gets a new special case every sweep needs a redesign,**
   just like product code. Make it read recorded facts instead of guessing.
8. **Watch for harness races.** In a browser test, act on the real event
   (`dragstart`, not "the mouse moved 4px"), read state again after an async
   step, and click "outside" at a point you've checked is really outside.

## 6. Fixing bugs

1. **Launch a fix for each finding as soon as it's reported.** Don't batch
   findings and wait.
2. **Look at the cause, not the symptom.** A crash in a display function may
   be a stale position from an edit three steps earlier.
3. **Keep fixes small and local in risky areas.** A fix that touches shared
   core code gets a full random-suite run at scale first.
4. **Check whether a fix caused the bug.** For each new finding, revert the
   recent fix locally and test again. Then you know whether it's a regression
   or an old bug found by deeper testing.
5. **Fix a small gap rather than documenting it as a "limitation".** Document
   a limit only with a real reason, and check that the claimed limit is real.
6. **Review a fix that loosens a test check.** Make sure the new rule is
   narrow and explained. A checker can be "fixed" into one that never fails.

## 7. Releases

1. **Release on promises and trend, not on "zero findings".** Fresh random
   seeds always find some edge case. Example criteria:
   - no open severe finding;
   - the random suites pass at a fixed budget with no allowance left for a
     serious promise;
   - two confirmation sweeps in a row find no new serious class;
   - one human or agent QA round finds nothing new and serious.
2. **Define "clean" before you sweep.** For example: no new class that breaks
   a serious promise. Smaller findings still get fixed, but they don't reset
   the count.
3. **Defer low-severity findings to after the release.** Log them. One-line
   wording or style fixes are the exception.
4. **Notes for users go in the CHANGELOG.** Prefer "behaviour notes" (what
   happens and why) to a "known issues" list of things you could have fixed.
5. **Dry-run the release workflow** before tagging. Check that versions match,
   package names are free, and publishing skips a version that already exists.

## 8. Code style

1. **Comments say why, in the present tense.** They give intent, a rule or a
   constraint. No history ("previously…", "round 37 fixed…"): git and the
   CHANGELOG hold that.
2. **No ticket or QA IDs in product code.** Write the rule the code protects
   instead. IDs are allowed only in allow-lists and open repro tests, which
   are removed when fixed.
3. **Keep modules small and organised by feature.** Split at real seams. Each
   module's doc comment says what it owns. A UI file holds components, not
   long inline handlers.
4. **Keep the public surface small.** Keep modules private by default and
   export the API on purpose. Don't make items public only so tests can reach
   them.
5. **Delete dead code.** No compatibility shims in a project with no users
   yet, no unused flags or hooks, no commented-out code.
6. **Keep lint settings in one place.** Suppressions name their reason and
   report when they're no longer needed.
7. **Format and lint before every commit,** on the full build and on the
   smallest feature build.
8. **Write commit subjects as one line that says what changed and why.** Keep
   one squashed commit per finished piece of agent work. Never rewrite pushed
   history.

## 9. Docs and generated files

1. **Generate what can be generated:** settings tables, type definitions,
   tables of contents, size tables, screenshots. CI checks that they're up to
   date.
2. **Never hand-edit a generated file.** Change the source and regenerate. A
   hand edit is overwritten the next time and fails the up-to-date check.
3. **Regenerate screenshots after every UI change,** from a script, so they
   always show the real product.
4. **Docs link to the repository's own files** (relative links), not to a
   package registry's copy that goes stale.
5. **Every public claim is backed by a test.** If the README says "fast",
   there is a timed test that guards it.

## 10. Working with agents

These rules are for AI coding agents, and they work just as well for human
contributors.

1. **Keep one binding practices document** (principles, promises, release
   criteria, process). Every brief points to it.
2. **Brief an agent like a peer:** the goal, what's already ruled out, the
   files to read, the exact scope, what to stay out of (other agents' areas),
   and what to report.
3. **Agents don't push or publish.** One person or agent merges, pushes and
   watches CI.
4. **Check agent claims before accepting them,** especially "this was only a
   test bug" or "this limit is fine". Read the diff of any loosened check.
5. **Keep long-running branches synced.** When the base moves, tell each
   long-running agent which areas changed, so it merges small and often.
6. **Follow the order the owner stated.** Parallelise only work they haven't
   ordered, and ask before starting a later step early.
7. **Briefs carry the machine rules:** the shared build slots, memory and
   time caps, never killing processes by name, no shell wait loops, and
   cleaning up builds at the end.
8. **Ask for a design-level answer** when an agent fixes the same area twice:
   "is this the same root cause in a new shape, and what fix would stop it?"

## 11. Machine, CI and git

1. **CI is the full check.** Locally, run the tests for what you changed,
   plus lint and format. CI runs everything: every feature build, every
   language, end-to-end tests and size checks.
2. **Keep CI notifications quiet.** Report failures and the final result
   only, not every passing job.
3. **One heavy build per CPU slot.** Share cores on purpose, and keep enough
   disk free (clean build outputs of finished work).
4. **Never kill processes by name or pattern,** only by the process ID you
   started. A name match can kill someone else's work.
5. **Don't write shell wait loops** (`until …; sleep`, `pgrep -f`, `tail -f`).
   They match themselves and never end. Use background jobs with a
   notification.
6. **Temporary folders may be RAM.** Keep anything you need after a restart
   on disk.
7. **Secrets stay in one ignored file.** They never go in commands, URLs,
   logs or commits. Scripts read them from that file.
8. **Shard slow test suites (end-to-end, browser) across machines:**
   - **Build once.** One job builds the app and packages and saves the build.
     Every test machine runs from that same build, so no machine builds its
     own.
   - **Machines from the test count.** A planning step lists the tests and
     starts one machine per N tests, so the run stays fast as the suite
     grows.
   - **Deal tests one by one** (round-robin), not in runs by file. Slow specs
     often sit next to each other, and a run-based split (Playwright's
     `--shard`) puts them all on one machine.
   - **Timed tests get machines of their own,** one worker each, so the other
     tests don't skew their numbers.
   - **The planner fails when a spec fails to load or no test is listed.** A
     spec with a syntax error otherwise lists zero tests, and the run reports
     success while skipping everything.
   - **Merge the shard reports** into one report, so a failure is easy to
     find.
   - **Keep the shared build until the whole run is done.** If a cleanup step
     deletes it early, a single failed shard can't be re-run.
   - **Locally, run only the specs you touched.** CI runs the full suite.

## 12. Rust

1. **Type your text positions.** Use byte offsets as `TextSize` / `TextRange`
   (crate `text-size`). Use UTF-16 or character positions only at the edge
   (browser, editor API), converted once with a line index.
2. **Have one safe door for cutting text.** Every slice goes through a helper
   that never panics on a position inside a character or past the end. Deny
   `clippy::string_slice` and the truncating `as` casts in the modules that
   handle text.
3. **No panics on user input.** Library code returns typed errors. Avoid
   `unwrap` and `expect` outside tests. An `expect` states the invariant it
   relies on.
4. **Use `#[expect(lint, reason = "…")]` instead of `#[allow]`,** so stale
   suppressions are reported. Keep the lint table in `[workspace.lints]`.
5. **Default to `pub(crate)`.** Export the public API with `pub use` from
   private modules. Put test-only helpers behind `#[cfg(test)]`.
6. **Optional features compile out cleanly.** Use
   `#[cfg_attr(not(feature = "x"), path = "off/x.rs")]` to swap in a stand-in
   module. Code used only by some features needs a `cfg` gate, or an
   `#[expect(dead_code, reason)]` scoped to those builds. CI runs clippy on
   every feature combination it ships.
7. **Enforce doors with `clippy.toml`.** List the raw APIs (timers,
   `Closure::forget`, raw schedulers) in `disallowed-methods`, so everything
   goes through your wrapper.
8. **Build tests first, then run them capped.** Use `cargo test --no-run`
   without a memory limit (the compiler needs it), then run the binary under
   `timeout` and a memory limit. A memory cap on the build itself kills
   `rustc`.
9. **Seed random tests from environment variables** (cases and seed), with
   defaults for CI and larger values for scale runs.
10. **Run `cargo fmt` and clippy with `-D warnings`** on the full build and the
    smallest build before each commit.

## 13. Rust in WebAssembly and UI frameworks

1. **Watch the stack in debug wasm.** Big structs copied into every closure
   overflow the stack. Keep component state behind one small `Copy` handle
   into an arena, and add a test that checks the handle's size.
2. **Deferred work must check that its target is still alive.** A timer or
   animation frame can run after its component unmounted. Use one scheduler
   that takes the thing it needs alive and skips the work if it's gone.
3. **Check reactive frameworks for leaks on unmount.** Some versions keep
   disposed subscribers alive (Leptos 0.8 does). Views that come and go read
   long-lived state through a relay that drops dead readers, and lists use
   keyed rows. A memory soak test guards this.
4. **Budget the wasm size per feature.** Measure it in CI, and make heavy
   parts optional features.
5. **Use wasm-safe dependencies** (no SIMD-only paths, no threads you don't
   have).

## 14. Web UI

1. **Compare with a product people already know** (VS Code for an editor).
   Match its spacing and behaviour unless you have a reason not to, and write
   the reason down.
2. **Hit areas are at least 24px.** Markers take no layout space of their
   own, so a marker appearing never moves the text.
3. **Every built-in string comes from one strings setting,** so the UI can be
   translated.
4. **Framework wrappers share one binding.** React, Vue and Svelte components
   only pass their props to one shared binding, so a fix there fixes all
   three. An API-parity test fails when a core feature isn't reachable from
   every wrapper.
5. **A popup keeps its side.** When late content makes it taller, it scrolls.
   It doesn't jump to the other side while the pointer moves toward it.
6. **Any click outside closes transient UI** (suggestions, menus), including
   clicks on the gutter and other chrome.

## 15. Security

1. **Secrets never appear in any message.** Every message to the user (errors,
   warnings, "can't save" reasons, screen-reader text) goes through one
   filter that hides secret values. Each kind of message hiding secrets on
   its own leaks sooner or later.
2. **A placeholder for a secret is atomic.** Edits, drops and pastes either
   take it whole or leave it alone. A part of it never shows as text.
3. **Test the leak promise with random tests** that put secrets in every
   shape and position, and check every output channel.
