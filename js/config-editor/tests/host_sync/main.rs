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
//! The core's record of each document's host (`js_support::HostChannel`:
//! the echo engine, the host log and the lists given) runs here as it is;
//! `sim` drives it as the two classes' `update` and `replaceValue` do, and
//! `model::Core` stands for the editor around it (the guard, the edits it
//! records). Half the runs have a host keeping revisions and naming the
//! texts its lists were worked out on (`rangesFor`), the other half a
//! v0.1.0 host. The main property: a lock never moves to text it was not
//! given for, in any order of echoes, re-mounts, hide/shows (in the same
//! element, and by a conditional render with an `echoKey`, the host's store
//! taking echoes while no editor shows the document) and resets. Runs are seeded:
//! `HOST_SYNC_CASES` (runs per view), `HOST_SYNC_STEPS` (actions per run)
//! and `HOST_SYNC_SEED` (the first seed) run more locally. Each class of
//! failure prints its smallest run as a test to paste below; classes an
//! open finding or the design explains are listed in `known.rs`.

mod host;
mod known;
mod model;
mod rng;
mod run;
mod sim;

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

/// A run that ends in `class` (the repros of a class by design).
fn assert_class(setup: run::Setup, acts: &[model::Act], class: model::Class) {
    match run::replay(setup, acts) {
        Err(failure) => assert_eq!(failure.class, class, "{}", failure.detail),
        Ok(()) => panic!("passed, not {class:?}"),
    }
}

/// Smallest runs of fixed classes, and of the classes by design in
/// `known.rs`, pasted from a failure.
#[allow(unused_imports)]
mod repros {
    use super::{assert_class, assert_ok};
    use crate::model::Class;
    use crate::model::Form::*;
    use crate::model::ResetKind::*;
    use crate::model::{Act, Anchor, Via};
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
                revisions: false,
            },
            &[
                Act::MoveLine { line: 18613 },
                Act::Reset {
                    doc: 0,
                    kind: Same,
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: true,
                    via: Via::ReplaceValue,
                },
                Act::Type { at: 19343, ch: 'a' },
                Act::Deliver {
                    doc: 1,
                    pick: 18008,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
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
                revisions: false,
            },
            &[
                Act::Reset {
                    doc: 1,
                    kind: NewVersion(3400),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Delete { at: 8993, len: 2 },
                Act::Deliver {
                    doc: 0,
                    pick: 617,
                    dup: false,
                    ranges: true,
                    same_task: true,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: false,
                    via: Via::ReplaceValue,
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
                revisions: false,
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
                    anchor: Anchor::None,
                },
                Act::Type { at: 48349, ch: ':' },
                Act::Delete { at: 8282, len: 1 },
                Act::Reset {
                    doc: 0,
                    kind: Same,
                    same_task: false,
                    via: Via::ReplaceValue,
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
                    anchor: Anchor::None,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: true,
                    ranges: false,
                    same_task: true,
                    anchor: Anchor::None,
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
                revisions: false,
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
                    anchor: Anchor::None,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 27286,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(3529),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
            ],
        );
    }

    /// By the echo contract: a reset adds a line, then the host's own text
    /// removes it again before the reset is listed: the text it gave last,
    /// given again, no news. The editor keeps the reset's text. The class
    /// is the narrow one the allowance names, not `NotConverged`.
    #[test]
    fn repro_editor_own_text_given_again_after_a_reset() {
        assert_class(
            Setup {
                view: Editor,
                base: 3,
                other: 2,
                form: Lines,
                revisions: false,
            },
            &[
                Act::Reset {
                    doc: 0,
                    kind: Twice(24951),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: true,
                    anchor: Anchor::None,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 1786,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Type { at: 58081, ch: ':' },
                Act::Enter { at: 37481 },
                Act::Deliver {
                    doc: 1,
                    pick: 36209,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Settle,
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(38468),
                    same_task: true,
                    via: Via::ReplaceValue,
                },
                Act::Own {
                    doc: 0,
                    edit: 28009,
                    ranges: true,
                    same_task: false,
                },
            ],
            Class::NotConvergedOnGivenAgain,
        );
    }

    /// The same in a documents view: a newer version, then the host's own
    /// text without its added line, the one it listed last. Each document
    /// goes through its own record as an editor does, and the view's listing
    /// of the other document no longer gives this one's text again: it ends
    /// on the host's text.
    #[test]
    fn repro_documents_own_text_given_again_after_a_reset() {
        assert_ok(
            Setup {
                view: Documents,
                base: 1,
                other: 1,
                form: Index,
                revisions: false,
            },
            &[
                Act::Own {
                    doc: 0,
                    edit: 22969,
                    ranges: true,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: Twice(60416),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Type { at: 7497, ch: 'b' },
                Act::MoveLine { line: 35032 },
                Act::Type { at: 61644, ch: 'a' },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(12078),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Own {
                    doc: 1,
                    edit: 62701,
                    ranges: true,
                    same_task: false,
                },
                Act::Type { at: 21737, ch: 'a' },
                Act::Type { at: 2108, ch: ' ' },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: true,
                    anchor: Anchor::None,
                },
                Act::Delete { at: 40421, len: 0 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: true,
                    anchor: Anchor::None,
                },
                Act::Delete { at: 63843, len: 2 },
                Act::Enter { at: 38994 },
                Act::Deliver {
                    doc: 0,
                    pick: 13762,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(60439),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Type { at: 56831, ch: ':' },
                Act::Settle,
                Act::Own {
                    doc: 0,
                    edit: 43117,
                    ranges: true,
                    same_task: false,
                },
            ],
        );
    }

    /// P4: a text sent twice, a reset, that text listed twice, a second
    /// reset, then the last copy of the text comes back: the host stores it
    /// over the reset and its listing drops the reset's, so the editor
    /// waits out the pause and tells the host the document again.
    #[test]
    fn repro_documents_late_echo_after_a_second_reset_hears_the_document() {
        assert_ok(
            Setup {
                view: Documents,
                base: 0,
                other: 2,
                form: Lines,
                revisions: false,
            },
            &[
                Act::Delete { at: 25242, len: 2 },
                Act::Deliver {
                    doc: 0,
                    pick: 1503,
                    dup: false,
                    ranges: false,
                    same_task: true,
                    anchor: Anchor::None,
                },
                Act::Type { at: 50182, ch: 'a' },
                Act::Type { at: 8875, ch: 'x' },
                Act::Delete { at: 3239, len: 1 },
                Act::Select { doc: 1 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Settle,
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: Twice(65224),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Own {
                    doc: 1,
                    edit: 62647,
                    ranges: true,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: Twice(42024),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
            ],
        );
    }

    /// A host keeping revisions: typing above the lock, a late server check
    /// of the text before (`rangesFor` its revision), a re-mount and a hide
    /// and show: the lock stays on its text exactly.
    #[test]
    fn a_lock_stays_on_its_text_through_a_late_check_a_remount_and_a_hide_and_show() {
        assert_ok(
            Setup {
                view: Editor,
                base: 0,
                other: 0,
                form: Index,
                revisions: true,
            },
            &[
                Act::Enter { at: 0 },
                Act::Type { at: 0, ch: 'x' },
                Act::Ranges {
                    doc: 0,
                    anchor: Anchor::Revision,
                    worked_on: 1,
                    same_task: false,
                },
                Act::Remount { doc: 0 },
                Act::Type { at: 1, ch: 'a' },
                Act::Deliver {
                    doc: 0,
                    pick: 1,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
                Act::FreshMount { doc: 0 },
            ],
        );
    }

    /// A new text of the host's own with the same ranges reads them in it
    /// (no whole-line lock), in a v0.1.0 host too.
    #[test]
    fn the_same_ranges_with_a_new_text_of_the_hosts_own_lock_exactly() {
        for revisions in [false, true] {
            assert_ok(
                Setup {
                    view: Editor,
                    base: 1,
                    other: 0,
                    form: Lines,
                    revisions,
                },
                &[
                    Act::Own {
                        doc: 0,
                        edit: 2,
                        ranges: true,
                        same_task: false,
                    },
                    Act::Type { at: 3, ch: 'x' },
                ],
            );
        }
    }

    /// Typing, a hide and show, then a reset (`replaceValue` and the host's
    /// list of it): the gone editor's late echo brings its typing back for
    /// a moment and the reset's report replaces it. Nothing typed in the
    /// new editor is lost.
    #[test]
    fn repro_editor_reset_after_a_hide_and_show_replaces_the_typing_brought_back() {
        assert_ok(
            Setup {
                view: Editor,
                base: 1,
                other: 1,
                form: Index,
                revisions: false,
            },
            &[
                Act::Type { at: 37807, ch: ' ' },
                Act::FreshMount { doc: 1 },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(7915),
                    same_task: false,
                    via: Via::Both,
                },
            ],
        );
    }

    /// A hide and show, a reset by `setValue`, then the gone editor's late
    /// echo shown over it: the revision of the reset's report still names
    /// the reset's text (a list for it never lands on the echo's text).
    #[test]
    fn repro_editor_a_resets_revision_names_its_text_after_an_older_echo_shows() {
        assert_ok(
            Setup {
                view: Editor,
                base: 2,
                other: 0,
                form: Index,
                revisions: true,
            },
            &[
                Act::Own {
                    doc: 0,
                    edit: 53966,
                    ranges: true,
                    same_task: false,
                },
                Act::Type { at: 20633, ch: 'a' },
                Act::Delete { at: 62131, len: 1 },
                Act::FreshMount { doc: 1 },
                Act::Reset {
                    doc: 0,
                    kind: NewVersion(51439),
                    same_task: false,
                    via: Via::SetValue,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 28090,
                    dup: true,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
            ],
        );
    }

    /// The same, through a re-mount and lists naming their revisions.
    #[test]
    fn repro_editor_a_resets_revision_names_its_text_through_a_remount() {
        assert_ok(
            Setup {
                view: Editor,
                base: 0,
                other: 3,
                form: Index,
                revisions: true,
            },
            &[
                Act::Type { at: 22220, ch: 'b' },
                Act::Reset {
                    doc: 1,
                    kind: LastListed,
                    same_task: true,
                    via: Via::Both,
                },
                Act::FreshMount { doc: 0 },
                Act::Reset {
                    doc: 1,
                    kind: NewVersion(16482),
                    same_task: true,
                    via: Via::SetValue,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 13056,
                    dup: false,
                    ranges: true,
                    same_task: true,
                    anchor: Anchor::Text,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
                Act::Remount { doc: 1 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: true,
                    anchor: Anchor::Revision,
                },
            ],
        );
    }

    /// P4: two resets by `setValue`, the host's own text equal to the
    /// first (its echoes dropped), then a reset to the document as it is
    /// (not reported): the editor still tells the host the document once
    /// the pause is over.
    #[test]
    fn repro_editor_a_reset_to_the_document_leaves_the_host_heard_again() {
        assert_ok(
            Setup {
                view: Editor,
                base: 2,
                other: 0,
                form: Index,
                revisions: false,
            },
            &[
                Act::Reset {
                    doc: 0,
                    kind: Twice(50458),
                    same_task: false,
                    via: Via::SetValue,
                },
                Act::Own {
                    doc: 0,
                    edit: 35404,
                    ranges: true,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: Same,
                    same_task: false,
                    via: Via::ReplaceValue,
                },
            ],
        );
    }

    /// The same in a documents view: two resets, the first one listed
    /// late, the store listed again (the second's listing dropped), then a
    /// `setValue` to the document as it is.
    #[test]
    fn repro_documents_a_reset_to_the_document_leaves_the_host_heard_again() {
        assert_ok(
            Setup {
                view: Documents,
                base: 3,
                other: 2,
                form: Index,
                revisions: false,
            },
            &[
                Act::Reset {
                    doc: 1,
                    kind: Twice(36312),
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Ranges {
                    doc: 1,
                    anchor: Anchor::None,
                    worked_on: 1004,
                    same_task: true,
                },
                Act::Reset {
                    doc: 1,
                    kind: Same,
                    same_task: false,
                    via: Via::SetValue,
                },
            ],
        );
    }

    /// P4, a host keeping revisions: two resets with the parent's state
    /// set too, typing, the first reset's old revision rendered late, then
    /// the host's own text equal to the second: it is news (the host gave
    /// the first one last), and the two converge.
    #[test]
    fn repro_documents_an_old_revision_rendered_is_the_text_given_last() {
        assert_ok(
            Setup {
                view: Documents,
                base: 0,
                other: 3,
                form: Lines,
                revisions: true,
            },
            &[
                Act::Reset {
                    doc: 1,
                    kind: Twice(26901),
                    same_task: false,
                    via: Via::Both,
                },
                Act::Select { doc: 1 },
                Act::Delete { at: 10238, len: 2 },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
                Act::Own {
                    doc: 1,
                    edit: 56860,
                    ranges: true,
                    same_task: true,
                },
            ],
        );
    }

    /// P4, a host keeping revisions: typing, a hide and show, typing in
    /// the new editor, then the gone editor's late echo: older than the
    /// new typing on its way, it keeps it (in both views).
    #[test]
    fn a_gone_editors_late_echo_keeps_the_new_typing_with_revisions() {
        for view in [Editor, Documents] {
            assert_ok(
                Setup {
                    view,
                    base: 0,
                    other: 1,
                    form: Lines,
                    revisions: true,
                },
                &[
                    Act::Type { at: 26650, ch: ' ' },
                    Act::FreshMount { doc: 0 },
                    Act::Type { at: 59253, ch: 'a' },
                    Act::Deliver {
                        doc: 0,
                        pick: 0,
                        dup: false,
                        ranges: true,
                        same_task: false,
                        anchor: Anchor::Text,
                    },
                ],
            );
        }
    }

    /// P4, a v0.1.0 host: the gone editor's late echo is the same text
    /// the new editor sent after a hide and show. The binding handed the
    /// gone editor's texts on their way to the new one, so each echo takes
    /// its own copy, and neither reads as the host's text (qa39 39b).
    #[test]
    fn repro_documents_a_gone_editors_echo_takes_a_new_copy() {
        assert_ok(
            Setup {
                view: Documents,
                base: 2,
                other: 1,
                form: Index,
                revisions: false,
            },
            &[
                Act::Enter { at: 26147 },
                Act::FreshMount { doc: 0 },
                Act::Type { at: 14236, ch: 'x' },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: false,
                    via: Via::SetValue,
                },
                Act::Enter { at: 21527 },
                Act::Type { at: 2704, ch: ' ' },
            ],
        );
    }

    /// The same with a reset's listing: the gone editor's `setValue`
    /// report and the new editor's reset to that text each take their own
    /// copy, and the reset's listing never reads as the host's text.
    #[test]
    fn repro_editor_a_gone_editors_echo_takes_a_resets_copy() {
        assert_ok(
            Setup {
                view: Editor,
                base: 1,
                other: 0,
                form: Index,
                revisions: false,
            },
            &[
                Act::Own {
                    doc: 0,
                    edit: 26090,
                    ranges: true,
                    same_task: false,
                },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: false,
                    via: Via::SetValue,
                },
                Act::FreshMount { doc: 0 },
                Act::Reset {
                    doc: 0,
                    kind: Original,
                    same_task: false,
                    via: Via::ReplaceValue,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::MoveLine { line: 63811 },
                Act::Deliver {
                    doc: 0,
                    pick: 63179,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Text,
                },
            ],
        );
    }

    /// P4, a v0.1.0 host: shown again on the text the gone editor's reset
    /// gave (its report still on its way), then typing, then the gone
    /// editor's echoes: the report is the reset's copy, handed on with the
    /// rest, and keeps the typing.
    #[test]
    fn a_gone_resets_report_after_a_hide_and_show_keeps_the_typing() {
        assert_ok(
            Setup {
                view: Editor,
                base: 2,
                other: 3,
                form: Lines,
                revisions: false,
            },
            &[
                Act::Type { at: 27496, ch: ':' },
                Act::Type { at: 56894, ch: 'a' },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 0,
                    kind: LastListed,
                    same_task: true,
                    via: Via::SetValue,
                },
                Act::FreshMount { doc: 0 },
                Act::Type { at: 15981, ch: ' ' },
                Act::Deliver {
                    doc: 1,
                    pick: 0,
                    dup: false,
                    ranges: false,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
            ],
        );
    }

    /// A host keeping revisions, shown again on an older revision than one
    /// the gone editor heard back (a reset given back before the host's
    /// store took an older echo): the newer text, given then, is the host
    /// catching up and applies; the host and the editor agree.
    #[test]
    fn shown_on_an_older_revision_the_newer_one_given_then_applies() {
        assert_ok(
            Setup {
                view: Editor,
                base: 1,
                other: 0,
                form: Lines,
                revisions: true,
            },
            &[
                Act::Own {
                    doc: 0,
                    edit: 3880,
                    ranges: true,
                    same_task: false,
                },
                Act::Type { at: 37713, ch: 'b' },
                Act::Deliver {
                    doc: 1,
                    pick: 49138,
                    dup: false,
                    ranges: false,
                    same_task: true,
                    anchor: Anchor::Text,
                },
                Act::Type { at: 19671, ch: ':' },
                Act::Delete { at: 46921, len: 0 },
                Act::MoveLine { line: 4563 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: true,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Text,
                },
                Act::Type { at: 25723, ch: 'b' },
                Act::Deliver {
                    doc: 1,
                    pick: 55026,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Type { at: 56714, ch: ':' },
                Act::Delete { at: 64450, len: 1 },
                Act::Deliver {
                    doc: 1,
                    pick: 34637,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Reset {
                    doc: 1,
                    kind: Twice(14062),
                    same_task: false,
                    via: Via::Both,
                },
                Act::Deliver {
                    doc: 0,
                    pick: 41828,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::FreshMount { doc: 0 },
            ],
        );
    }

    /// P4, a v0.1.0 host: a conditional render with an `echoKey` hides the
    /// editor and shows a new one (another element) on the host's older
    /// text; typing there, then the gone editor's late echo. The record
    /// kept under the name is the new editor's: the echo keeps the typing
    /// (without it, the echo was the host's own text and replaced it).
    #[test]
    fn a_keyed_hide_and_show_keeps_the_typing_from_a_late_echo() {
        assert_ok(
            Setup {
                view: Editor,
                base: 3,
                other: 0,
                form: Lines,
                revisions: false,
            },
            &[
                Act::Type { at: 38237, ch: ':' },
                Act::Hide { doc: 1 },
                Act::Type { at: 19170, ch: 'x' },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::Revision,
                },
            ],
        );
    }

    /// The same with an echo the host's store took while the document was
    /// hidden (no editor heard it), and one coming in the same task as
    /// the typing after the show.
    #[test]
    fn a_keyed_hide_and_show_keeps_the_typing_after_an_echo_taken_while_hidden() {
        assert_ok(
            Setup {
                view: Editor,
                base: 0,
                other: 1,
                form: Lines,
                revisions: false,
            },
            &[
                Act::Type { at: 42516, ch: ' ' },
                Act::Delete { at: 25390, len: 1 },
                Act::Hide { doc: 0 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: false,
                    anchor: Anchor::None,
                },
                Act::Delete { at: 22413, len: 0 },
                Act::Deliver {
                    doc: 0,
                    pick: 0,
                    dup: false,
                    ranges: true,
                    same_task: true,
                    anchor: Anchor::Text,
                },
            ],
        );
    }
}
