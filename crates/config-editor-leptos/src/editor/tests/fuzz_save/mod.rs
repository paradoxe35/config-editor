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
//! The standing test runs 40 sessions per host and starting view;
//! `FUZZ_CASES=5000 FUZZ_SEED=7 cargo test -p leptos-config-editor
//! fuzz_save -- --nocapture` runs more (`FUZZ_PAIR=TOML>JSON` one pair
//! only, `FUZZ_ALL=1` shrinks and prints the allowed failures too).
//!
//! A second test runs the sessions of an XML view under a schema typing
//! each value (`typing`): the view reads its text by the schema, and the
//! save keeps each value in the host's type or as the view reads it by the
//! editor's record of the edits; the host then has a problem exactly when
//! the view shows one.

mod checks;
mod docs;
mod dotted_keys;
mod known;
mod ops;
mod repros;
mod shrink;
mod typing;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use leptos::prelude::*;

use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;
use crate::editor::save_properties::Random;
use crate::editor::schema::Schema;
use crate::editor::test_state::TestEditor;
use checks::{Broken, Failure};
// `Put` and `Take` are named by the printed regression tests.
#[allow(unused_imports, reason = "named by the printed regression tests")]
use ops::{Op, Put, Take};

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// One session: a host's text, the language it is first shown in, the
/// steps, and the seed of the schema typing the text (see `typing`), if
/// the session has one.
#[derive(Clone, Debug)]
struct Case {
    host: Language,
    view: Language,
    text: String,
    ops: Vec<Op>,
    schema: Option<u64>,
}

impl Case {
    /// The schema typing the session's text, compiled; `Err` when it has
    /// one that does not compile (its text does not read).
    fn schema(&self) -> Result<Option<Schema>, ()> {
        let Some(seed) = self.schema else {
            return Ok(None);
        };
        let value = self.host.parse(&self.text).map_err(|_| ())?;
        Schema::compile(&typing::schema(&value, seed).to_string())
            .map(Some)
            .map_err(|_| ())
    }
}

/// Where a session broke the promise: the step (its index) and how.
#[derive(Clone, Debug)]
struct Broke {
    step: usize,
    failure: Box<Failure>,
}

/// Runs `case`; `Ok(false)` when it cannot start (its text does not read,
/// or no view of it can be shown).
fn run(case: &Case) -> Result<bool, Broke> {
    let Ok(schema) = case.schema() else {
        return Ok(false);
    };
    let bundle = schema.as_ref().map(|schema| schema.bundle.clone());
    let shown = match Bridge::new(case.host, case.view, bundle.clone()) {
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
        editor.schema = bundle;
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
        for (step, op) in case.ops.iter().enumerate() {
            let before = state.doc().text.get_untracked();
            let language_before = state.sync().edit_language.get_untracked();
            if op.saves() {
                start.note(language_before, &before);
            }
            ops::apply(state, case.host, op, &mut clipboard, &mut time);
            if trace {
                println!(
                    "-- {op:?} ({}, in sync {}):\n{}--- host:\n{}--- problems: {:?}",
                    state.sync().edit_language.get_untracked().label(),
                    state.sync().in_sync.get_untracked(),
                    state.doc().text.get_untracked(),
                    state.sync().host_text.get_value(),
                    state.sync().conversion_problems.get_untracked()
                );
            }
            if op.saves() {
                let switched_from =
                    matches!(op, Op::Switch { .. }).then_some((language_before, before.as_str()));
                checks::check(state, &start, &held, switched_from)
                    .and_then(|()| checks::problems_agree(state, &held, schema.as_ref()))
                    .map_err(|failure| Broke { step, failure })?;
            }
            let language = state.sync().edit_language.get_untracked();
            let text = state.doc().text.get_untracked();
            if language == case.host {
                start.rebase(&text);
            } else if matches!(op, Op::Switch { .. }) {
                let in_sync = state.sync().in_sync.get_untracked();
                let host_text = held.lock().unwrap().clone();
                start.seen(language, &text, in_sync.then_some(host_text.as_str()));
            }
        }
        ops::pause(state);
        checks::check(state, &start, &held, None)
            .and_then(|()| checks::problems_agree(state, &held, schema.as_ref()))
            .map_err(|failure| Broke {
                step: case.ops.len(),
                failure,
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
            })
        }
    }
}

/// The host's text once `ops` are made in a `view` of `text` (in `host`)
/// and a pause saves them.
fn saved_after(host: Language, view: Language, text: &str, ops: &[Op]) -> String {
    let held: checks::HostCopy = std::sync::Arc::new(std::sync::Mutex::new(text.to_string()));
    let mut editor = TestEditor::new(host, text);
    let told = held.clone();
    editor.on_change = Some(Callback::new(move |text: String| {
        *told.lock().unwrap() = text;
    }));
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let (mut clipboard, mut time) = (None, 0.0);
    for op in ops {
        ops::apply(state, host, op, &mut clipboard, &mut time);
    }
    ops::pause(state);
    held.lock().unwrap().clone()
}

/// Runs a printed case (see [`shrink::repro`]) and fails on a broken save.
#[allow(dead_code, reason = "called by the printed regression tests")]
fn replay(host: Language, view: Language, text: &str, ops: &[Op]) {
    replay_case(Case {
        host,
        view,
        text: text.to_string(),
        ops: ops.to_vec(),
        schema: None,
    });
}

/// [`replay`] under the schema `typing` makes of `text` with `seed`.
#[allow(dead_code, reason = "called by the printed regression tests")]
fn replay_typed(host: Language, view: Language, seed: u64, text: &str, ops: &[Op]) {
    replay_case(Case {
        host,
        view,
        text: text.to_string(),
        ops: ops.to_vec(),
        schema: Some(seed),
    });
}

#[allow(dead_code, reason = "called by the printed regression tests")]
fn replay_case(case: Case) {
    if let Err(broke) = run(&case) {
        if setting("FUZZ_TRACE").is_some() {
            println!(
                "== broke: view {:?}:\n{}--- saved:\n{}",
                broke.failure.view.map(|view| view.label()),
                broke.failure.view_text,
                broke.failure.saved
            );
        }
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
        schema: None,
    }
}

/// [`case`] typed by a schema (its seed drawn after the session's).
fn typed_case(seed: u64, host: usize, view: usize, index: usize) -> Case {
    let mut case = case(seed, host, view, index);
    let mut random = Random(seed ^ (index as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
    case.schema = Some(random.below(1 << 30) as u64);
    case
}

/// The sessions per pair: `FUZZ_CASES`, else the standing test's
/// `standing`.
fn cases(standing: usize) -> usize {
    setting("FUZZ_CASES").map_or(standing, |value| value.parse().unwrap())
}

/// A class of failures: what broke in which pair, and the finding allowing
/// it.
type Class = (String, Option<&'static str>);

#[test]
fn random_sessions_keep_the_save_promise() {
    let pairs: Vec<(usize, usize)> = (0..ALL.len())
        .flat_map(|host| (0..ALL.len()).map(move |view| (host, view)))
        .collect();
    explore(cases(40), &pairs, case);
}

#[test]
fn random_sessions_under_a_schema_keep_the_save_promise() {
    // An XML view of each other host: the view whose scalars the schema
    // reads from text.
    let xml = ALL.len() - 1;
    let pairs: Vec<(usize, usize)> = (0..xml).map(|host| (host, xml)).collect();
    explore(cases(40), &pairs, typed_case);
}

/// Runs `cases` sessions (`FUZZ_CASES`) of each of `pairs` (host and
/// view, by their index in [`ALL`]) as `make` draws them, and fails on a
/// broken save no known finding allows, printing each class's first one
/// shrunk.
fn explore(cases: usize, pairs: &[(usize, usize)], make: fn(u64, usize, usize, usize) -> Case) {
    let seed: u64 = setting("FUZZ_SEED").map_or(0x5a7e, |value| value.parse().unwrap());
    let only = setting("FUZZ_PAIR");
    let all = setting("FUZZ_ALL").is_some();
    let repros: usize = setting("FUZZ_REPROS").map_or(1, |value| value.parse().unwrap());
    let started = std::time::Instant::now();
    let (mut ran, mut skipped) = (0, 0);
    // Failures by class (what broke, host, view, allowance): how many, and
    // the first one shrunk.
    let mut classes: BTreeMap<Class, (usize, Vec<String>)> = BTreeMap::new();
    for &(host, view) in pairs {
        let pair = format!("{}>{}", ALL[host].label(), ALL[view].label());
        if only.as_ref().is_some_and(|only| *only != pair) {
            continue;
        }
        for index in 0..cases {
            let case = make(seed, host, view, index);
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
