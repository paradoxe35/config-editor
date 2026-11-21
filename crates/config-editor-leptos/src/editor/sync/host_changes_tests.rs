//! The host's own change (`applyChanges`) through the edit pipeline: made
//! on its text now or on an older revision (rebased over the user's edits),
//! followed exactly by the locks, never an undo step.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::{ChangeError, HostEdits, apply_host_changes};
use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::paths::{TextPosition, TextRange};
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

/// An editor on `text` with `locked` (lines and columns) read-only, and
/// what `on_change` heard.
fn mounted(text: &str, locked: Vec<TextRange>) -> (Owner, EditorState, Arc<Mutex<Vec<String>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let told = heard.clone();
    let (owner, state) = TestEditor {
        on_change: Some(Callback::new(move |text: String| {
            told.lock().expect("heard").push(text);
        })),
        read_only_ranges: locked,
        ..TestEditor::new(Language::YAML, text)
    }
    .mount();
    (owner, state, heard)
}

/// The user types `insert` at byte `at`.
fn typed(state: EditorState, byte: usize, insert: &str) {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{insert}{}", &before[..byte], &before[byte..]);
    state.commit(Edit::new_units(
        before.clone(),
        (byte, byte),
        after,
        byte + insert.len(),
    ));
}

fn locked_text(state: EditorState) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    state.protect().ranges.with_untracked(|ranges| {
        ranges
            .iter()
            .map(|range| text[usize::from(range.start())..usize::from(range.end())].to_string())
            .collect()
    })
}

fn line(line: usize, from: usize, to: usize) -> TextRange {
    TextRange::new(TextPosition::new(line, from), TextPosition::new(line, to))
}

fn utf16(from: usize, to: usize, insert: &str) -> HostEdits {
    HostEdits::Utf16(vec![(from, to, insert.to_string())])
}

#[test]
fn a_change_on_the_hosts_text_moves_the_locks_and_is_no_undo_step() {
    let (_owner, state, heard) = mounted("a: 1\nb: 2\n", vec![line(2, 1, 5)]);
    let revision = apply_host_changes(state, &utf16(0, 0, "top: 0\n"), None).expect("applied");
    assert!(revision.is_some());
    assert_eq!(state.doc().text.get_untracked(), "top: 0\na: 1\nb: 2\n");
    assert_eq!(locked_text(state), ["b: 2"]);
    // The host knows its own change: nothing told, nothing to undo.
    assert!(heard.lock().expect("heard").is_empty());
    assert!(
        !state
            .doc()
            .history
            .with_untracked(|history| history.can_undo())
    );
}

#[test]
fn a_change_on_an_older_revision_is_rebased_over_the_typing_and_told() {
    let (_owner, state, heard) = mounted("a: 1\nb: 2\n", vec![line(2, 1, 5)]);
    let base = state
        .sync()
        .link(|link| link.log.host_revision())
        .expect("mounted");
    // The user types at the end of `a`'s value, then the host, on the text
    // before, changes `a`'s value and adds a line above `b`.
    typed(state, 4, "1");
    let edits = HostEdits::Utf16(vec![(3, 4, "7".to_string()), (5, 5, "c: 3\n".to_string())]);
    apply_host_changes(state, &edits, Some(base)).expect("applied");
    assert_eq!(state.doc().text.get_untracked(), "a: 71\nc: 3\nb: 2\n");
    assert_eq!(locked_text(state), ["b: 2"]);
    assert_eq!(
        heard.lock().expect("heard").last().map(String::as_str),
        Some("a: 71\nc: 3\nb: 2\n")
    );
}

#[test]
fn a_base_never_sent_or_out_of_date_changes_nothing() {
    let (_owner, state, _) = mounted("a: 1\n", Vec::new());
    assert_eq!(
        apply_host_changes(state, &utf16(0, 0, "x"), Some(u64::MAX)),
        Err(ChangeError::Unknown(u64::MAX))
    );
    assert_eq!(
        apply_host_changes(state, &utf16(4, 3, "x"), None),
        Err(ChangeError::BadRanges)
    );
    assert_eq!(state.doc().text.get_untracked(), "a: 1\n");
}

/// A small seeded generator (xorshift).
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % bound.max(1) as u64).unwrap_or(0)
    }
}

fn seed() -> u64 {
    std::env::var("HOST_LOG_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x7f4a_7c15_2545_f491)
}

fn cases(default: usize) -> usize {
    std::env::var("HOST_LOG_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

/// A host change on an older revision gives the text the user's edits
/// make of the host's changed text (both orders meet), and the lock stays
/// on its text.
#[test]
fn host_changes_on_older_revisions_meet_the_users_edits() {
    let mut rng = Rng(seed());
    for case in 0..cases(40) {
        let (_owner, state, _) = mounted("a: 1\nkeep: me\nz: 9\n", vec![line(2, 1, 9)]);
        let base = state
            .sync()
            .link(|link| link.log.host_revision())
            .expect("mounted");
        let base_text = state.doc().text.get_untracked();
        // The user types a few letters outside the lock.
        for _ in 0..rng.below(4) {
            let text = state.doc().text.get_untracked();
            let at = [0, 2, text.len()][rng.below(3)];
            typed(state, at, ["x", "\n", "yy"][rng.below(3)]);
        }
        // The host inserts at a line start of the base text.
        let at = [0, 5, base_text.len()][rng.below(3)];
        let insert = ["h: 1\n", "#\n", ""][rng.below(3)];
        apply_host_changes(state, &utf16(at, at, insert), Some(base)).expect("applied");
        let text = state.doc().text.get_untracked();
        assert_eq!(locked_text(state), ["keep: me"], "case {case}: {text:?}");
        assert!(text.contains(insert), "case {case}: {text:?}");
        assert_eq!(text.matches("keep: me").count(), 1, "case {case}");
    }
}

/// While editing as another language the host's change applies to its
/// text, and the call names the new text's revision (it changed).
#[cfg(feature = "json")]
#[test]
fn a_host_change_while_editing_as_another_language_names_its_revision() {
    let (_owner, state) = TestEditor {
        edit_as: Some(Language::JSON),
        ..TestEditor::new(Language::YAML, "a: 1\n")
    }
    .mount();
    let revision = apply_host_changes(state, &utf16(0, 0, "b: 2\n"), None).expect("applied");
    assert!(revision.is_some());
    assert_eq!(*state.sync().host_text.get_value(), *"b: 2\na: 1\n");
}
