//! Random host sync: a host with `echoes: true` and read-only ranges, its
//! texts handed back late, out of order, twice or never, its own texts
//! and resets (`replaceValue`, twice in a task, to the same text, to the
//! last one listed), its ranges by lines or by index worked out in its own
//! lagging copy, the user typing, deleting, pressing Enter and moving
//! lines meanwhile (and switching tabs in a `ConfigDocuments`). After
//! every step the locked text is unchanged and locked, and nothing beside
//! it is; a text handed back never replaces newer typing; once every text
//! came back the host and the editor agree; nothing panics or places
//! ranges past a text's end.
//!
//! The library's echo, remap and reset code (wasm32-only there) runs here
//! natively, loaded by path (`host_locks` with it); `editor_sim` and `documents_sim` mirror the
//! two classes' `update` and `replaceValue` over it, and `model::Core`
//! the editor's locks. Runs are seeded: `HOST_SYNC_CASES` (runs per view),
//! `HOST_SYNC_STEPS` (actions per run) and `HOST_SYNC_SEED` (the first
//! seed) run more locally. Each class of failure prints its smallest run
//! as a test to paste below; classes an open finding explains are listed
//! in `known.rs`.

#[path = "../../src/text_lines.rs"]
mod text_lines;

#[allow(dead_code)]
#[path = "../../src/remap.rs"]
mod remap;

#[allow(dead_code)]
#[path = "../../src/host_locks.rs"]
mod host_locks;

#[allow(dead_code)]
#[path = "../../src/echoes.rs"]
mod echoes;

#[path = "../../src/documents_view/echo_ranges.rs"]
mod echo_ranges;

mod documents_sim;
mod editor_sim;
mod host;
mod known;
mod model;
mod rng;
mod run;

use model::Failure;
use rng::env_or;
use run::{View, explore, repro};

fn check_view(view: View) {
    let cases = env_or("HOST_SYNC_CASES", 300);
    let steps = env_or("HOST_SYNC_STEPS", 40) as usize;
    let first = env_or("HOST_SYNC_SEED", 1);
    let found = explore(view, cases, steps, first);
    let mut new = Vec::new();
    for (key, found) in &found {
        let (class, step) = key;
        match known::excused(view, *class, step) {
            Some(finding) => eprintln!(
                "{view:?} {class:?} at {step}: {} of {cases} runs, known ({finding})\n{}",
                found.hits,
                repro(key, found)
            ),
            None => new.push(format!(
                "{view:?} {class:?} at {step}: {} of {cases} runs\n{}",
                found.hits,
                repro(key, found)
            )),
        }
    }
    assert!(new.is_empty(), "new classes:\n{}", new.join("\n\n"));
}

#[test]
fn random_host_sync_keeps_a_configeditors_locks() {
    check_view(View::Editor);
}

#[test]
fn random_host_sync_keeps_a_documents_views_locks() {
    check_view(View::Documents);
}

/// A run that must pass (the repros below).
#[allow(dead_code)]
fn assert_ok(setup: run::Setup, acts: &[model::Act]) {
    if let Err(Failure { class, detail, .. }) = run::replay(setup, acts) {
        panic!("{class:?}: {detail}");
    }
}

/// Smallest runs of fixed classes, pasted from a failure.
#[allow(unused_imports)]
mod repros {
    use super::assert_ok;
    use crate::model::Act;
    use crate::model::Form::*;
    use crate::model::ResetKind::*;
    use crate::run::Setup;
    use crate::run::View::*;
}
