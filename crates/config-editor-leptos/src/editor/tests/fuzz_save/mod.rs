//! Random save-promise testing: seeded random editing sessions, the way a
//! user edits (typing, Backspace, Enter, line commands, cut, copy and
//! paste of entries, lines and line texts, Replace all, whole-text
//! pastes, undo and redo, typing pauses, view switches), on documents rich
//! in the forms only their host writes, in every host natively and seen
//! as every other language. After every save (a pause or a switch) the
//! save promise must hold (`checks`): the host's text reads, means what
//! the view shows, keeps every value the session left alone — and every
//! value it moved or copied — in its written form, and keeps the comments
//! of what was left alone; or the save waits with a problem shown.
//!
//! A failure is shrunk (`shrink`) to a short session on a short document
//! and printed as a ready-to-paste regression test. Failures of an open
//! finding are allowed by name in `known.rs` (remove the entry with the
//! fix).
//!
//! The standing test runs [`CASES`] sessions per host and starting view;
//! `FUZZ_CASES=5000 FUZZ_SEED=7 cargo test -p config-editor-leptos
//! fuzz_save -- --nocapture` runs more (`FUZZ_PAIR=TOML>JSON` one pair
//! only, `FUZZ_ALL=1` shrinks and prints the allowed failures too).

mod checks;
mod docs;
mod known;
mod ops;
mod repros;
mod shrink;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use leptos::prelude::*;

use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;
use crate::editor::save_properties::Random;
use crate::editor::test_state::TestEditor;
use checks::{Broken, Failure};
// `Put` and `Take` are named by the printed regression tests.
#[allow(unused_imports, reason = "named by the printed regression tests")]
use ops::{Op, Put, Take};

/// Sessions per host and starting view in the standing test.
const CASES: usize = 40;

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// One session: a host's text, the language it is first shown in, the
/// steps.
#[derive(Clone, Debug)]
struct Case {
    host: Language,
    view: Language,
    text: String,
    ops: Vec<Op>,
}

/// Where a session broke the promise: the step (its index) and how.
#[derive(Clone, Debug)]
struct Broke {
    step: usize,
    failure: Box<Failure>,
    /// Whether the text was edited in the host's own language before.
    edited_natively: bool,
}

/// Runs `case`; `Ok(false)` when it cannot start (its text does not read,
/// or no view of it can be shown).
fn run(case: &Case) -> Result<bool, Broke> {
    let shown = match Bridge::new(case.host, case.view, None) {
        Some(bridge) => match bridge.to_edit(&case.text) {
            Ok(conversion) => conversion.text,
            Err(_) => return Ok(false),
        },
        None => case.text.clone(),
    };
    let Some(mut start) = checks::Start::new(case.host, &case.text, &shown, case.view) else {
        return Ok(false);
    };
    let trace = setting("FUZZ_TRACE").is_some();
    if trace {
        println!(
            "== {} as {}:\n{}",
            case.host.label(),
            case.view.label(),
            case.text
        );
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<(), Broke> {
        let held: checks::HostCopy = std::sync::Arc::new(std::sync::Mutex::new(case.text.clone()));
        let mut editor = TestEditor::new(case.host, &case.text);
        let told = held.clone();
        editor.on_change = Some(Callback::new(move |text: String| {
            *told.lock().unwrap() = text;
        }));
        if case.view != case.host {
            editor.edit_as = Some(case.view);
        }
        let (_owner, state) = editor.mount();
        let mut clipboard = None;
        let mut time = 0.0;
        let mut edited_natively = false;
        for (step, op) in case.ops.iter().enumerate() {
            let native = state.sync().edit_language.get_untracked() == case.host;
            let before = state.doc().text.get_untracked();
            if op.saves() {
                start.note(&before);
            }
            ops::apply(state, case.host, op, &mut clipboard, &mut time);
            if native
                && !matches!(op, Op::Switch { .. })
                && before != state.doc().text.get_untracked()
            {
                edited_natively = true;
            }
            if trace {
                println!(
                    "-- {op:?} ({}, in sync {}):\n{}",
                    state.sync().edit_language.get_untracked().label(),
                    state.sync().in_sync.get_untracked(),
                    state.doc().text.get_untracked()
                );
            }
            if op.saves() {
                checks::check(state, &start, &held).map_err(|failure| Broke {
                    step,
                    failure,
                    edited_natively,
                })?;
            }
            let language = state.sync().edit_language.get_untracked();
            let text = state.doc().text.get_untracked();
            if language == case.host {
                start.rebase(&text);
            } else if matches!(op, Op::Switch { .. }) {
                start.seen(language, &text);
            }
        }
        ops::pause(state);
        checks::check(state, &start, &held).map_err(|failure| Broke {
            step: case.ops.len(),
            failure,
            edited_natively,
        })
    }));
    match outcome {
        Ok(result) => result.map(|()| true),
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
                .unwrap_or_default();
            Err(Broke {
                step: case.ops.len(),
                failure: Failure::new(Broken::Panic, "", message),
                edited_natively: false,
            })
        }
    }
}

/// Runs a printed case (see [`shrink::repro`]) and fails on a broken save.
#[allow(dead_code, reason = "called by the printed regression tests")]
fn replay(host: Language, view: Language, text: &str, ops: &[Op]) {
    let case = Case {
        host,
        view,
        text: text.to_string(),
        ops: ops.to_vec(),
    };
    if let Err(broke) = run(&case) {
        panic!(
            "step {}: {:?} {}\n{}",
            broke.step, broke.failure.broken, broke.failure.was, broke.failure.detail
        );
    }
}

fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// The session `index` of the pair (`host`, `view`) from `seed`.
fn case(seed: u64, host: usize, view: usize, index: usize) -> Case {
    let mixed = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((host * 5 + view) as u64)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9)
        .wrapping_add(index as u64);
    let mut random = Random(mixed);
    random.below(2);
    let host_language = ALL[host];
    let text = docs::document(host_language, &mut random);
    let length = 3 + random.below(12);
    Case {
        host: host_language,
        view: ALL[view],
        text,
        ops: ops::session(&mut random, length),
    }
}

/// A class of failures: what broke in which pair, and the finding allowing
/// it.
type Class = (String, Option<&'static str>);

#[test]
fn random_sessions_keep_the_save_promise() {
    let cases: usize = setting("FUZZ_CASES").map_or(CASES, |value| value.parse().unwrap());
    let seed: u64 = setting("FUZZ_SEED").map_or(0x5a7e, |value| value.parse().unwrap());
    let only = setting("FUZZ_PAIR");
    let all = setting("FUZZ_ALL").is_some();
    let repros: usize = setting("FUZZ_REPROS").map_or(1, |value| value.parse().unwrap());
    let started = std::time::Instant::now();
    let (mut ran, mut skipped) = (0, 0);
    // Failures by class (what broke, host, view, allowance): how many, and
    // the first one shrunk.
    let mut classes: BTreeMap<Class, (usize, Vec<String>)> = BTreeMap::new();
    for (host, host_language) in ALL.iter().enumerate() {
        for (view, view_language) in ALL.iter().enumerate() {
            let pair = format!("{}>{}", host_language.label(), view_language.label());
            if only.as_ref().is_some_and(|only| *only != pair) {
                continue;
            }
            for index in 0..cases {
                let case = case(seed, host, view, index);
                match run(&case) {
                    Ok(true) => ran += 1,
                    Ok(false) => skipped += 1,
                    Err(broke) => {
                        ran += 1;
                        let allowed = known::allowed(&case, &broke);
                        let class = format!("{:?} {pair}", broke.failure.broken);
                        let entry = classes.entry((class, allowed)).or_default();
                        entry.0 += 1;
                        if entry.1.len() < repros && (allowed.is_none() || all) {
                            entry.1.push(shrink::shrunk_repro(&case, &broke));
                        }
                    }
                }
            }
        }
    }
    let mut unexpected = Vec::new();
    for ((class, allowed), (count, repros)) in &classes {
        println!("{class}: {count} ({})", allowed.unwrap_or("not allowed"));
        for repro in repros {
            let line = format!("{class}, {count} sessions:\n{repro}");
            if allowed.is_none() {
                unexpected.push(line);
            } else {
                println!("{line}");
            }
        }
    }
    println!(
        "{ran} sessions ({skipped} could not start) in {:.1} s; saves in a view: {} written, {} waiting",
        started.elapsed().as_secs_f64(),
        checks::WRITTEN.load(std::sync::atomic::Ordering::Relaxed),
        checks::WAITING.load(std::sync::atomic::Ordering::Relaxed),
    );
    assert!(
        unexpected.is_empty(),
        "{} classes of broken saves:\n\n{}",
        unexpected.len(),
        unexpected.join("\n\n")
    );
}
