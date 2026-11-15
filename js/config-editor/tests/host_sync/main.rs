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

    /// A reset to the text the editor holds (`false`: nothing replaced),
    /// then a reset and typing: the host lists no text of its own for it.
    #[test]
    fn repro_editor_reverted_by_same_listing() {
        assert_ok(
            Setup {
                view: Editor,
                base: 1,
                other: 0,
                form: Lines,
            },
            &[
                Act::MoveLine { line: 18613 },
                Act::Reset {
                    doc: 0,
                    kind: Same,
                    same_task: false,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: true,
                },
                Act::Type { at: 19343, ch: 'a' },
                Act::Deliver {
                    doc: 1,
                    pick: 18008,
                    dup: false,
                    ranges: true,
                    same_task: false,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                },
            ],
        );
    }

    /// A lock on a line joined to the one above, an echo with its ranges,
    /// then a reset to the original: the lock lands on its text.
    #[test]
    fn repro_editor_lock_off_at_reset() {
        assert_ok(
            Setup {
                view: Editor,
                base: 2,
                other: 3,
                form: Index,
            },
            &[
                Act::Reset {
                    doc: 1,
                    kind: NewVersion(3400),
                    same_task: false,
                },
                Act::Delete { at: 8993, len: 2 },
                Act::Deliver {
                    doc: 0,
                    pick: 617,
                    dup: false,
                    ranges: true,
                    same_task: true,
                },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: false,
                },
            ],
        );
    }

    /// A reset to the held text keeps the host's store: its echo, given
    /// twice in a task after an older one, is no revert.
    #[test]
    fn repro_documents_reverted_by_same_listing() {
        assert_ok(
            Setup {
                view: Documents,
                base: 3,
                other: 1,
                form: Lines,
            },
            &[
                Act::Type { at: 51612, ch: ' ' },
                Act::Type { at: 29740, ch: 'a' },
                Act::Deliver {
                    doc: 0,
                    pick: 7705,
                    dup: false,
                    ranges: true,
                    same_task: false,
                },
                Act::Type { at: 48349, ch: ':' },
                Act::Delete { at: 8282, len: 1 },
                Act::Reset {
                    doc: 0,
                    kind: Same,
                    same_task: false,
                },
                Act::Drop {
                    doc: 0,
                    pick: 19620,
                },
                Act::Type { at: 32855, ch: ':' },
                Act::Own {
                    doc: 1,
                    edit: 21814,
                    ranges: true,
                    same_task: false,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: true,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: true,
                    ranges: false,
                    same_task: true,
                },
            ],
        );
    }

    /// Blank lines typed around the lock, then a newer version: a line
    /// diff aligns a blank line across the locked text; the lock stays on it.
    #[test]
    fn repro_documents_lock_off_at_new_version() {
        assert_ok(
            Setup {
                view: Documents,
                base: 1,
                other: 0,
                form: Lines,
            },
            &[
                Act::Type { at: 36100, ch: ' ' },
                Act::Enter { at: 48884 },
                Act::Type { at: 3357, ch: 'b' },
                Act::Enter { at: 29002 },
                Act::Type { at: 25353, ch: 'a' },
                Act::Type { at: 33386, ch: 'a' },
                Act::Enter { at: 56736 },
                Act::Type { at: 31499, ch: 'b' },
                Act::Type { at: 32644, ch: 'x' },
                Act::Type { at: 39831, ch: 'a' },
                Act::Enter { at: 40736 },
                Act::Type { at: 24962, ch: 'a' },
                Act::Deliver {
                    doc: 0,
                    pick: 55128,
                    dup: false,
                    ranges: true,
                    same_task: false,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 27286,
                    dup: false,
                    ranges: true,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(3529),
                    same_task: false,
                },
            ],
        );
    }
}
