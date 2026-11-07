//! The pipeline on a live editor state: the stages' order, a refused edit
//! changing nothing, undo through folds, and the host's own text passing
//! the read-only check.

use std::cell::RefCell;

use leptos::prelude::*;

use super::super::languages::Language;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::sync::take_host_value;
use super::super::test_state::TestEditor;
use super::{Edit, Stage, take_trace};

/// `a: 1` (line 1) is read-only; `b:` (line 2) opens a block that folds.
const TEXT: &str = "a: 1\nb:\n  c: 2\n  d: 3\n";

/// The end of `b:`, where typing opens its fold.
const FOLDED_LINE_END: usize = 7;

/// What `on_change` saw: the text, the caret, and whether the edit could
/// be undone already.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    text: String,
    caret: usize,
    can_undo: bool,
}

thread_local! {
    static STATE: std::cell::Cell<Option<EditorState>> = const { std::cell::Cell::new(None) };
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
}

fn seen() -> Vec<Seen> {
    SEEN.with(|seen| std::mem::take(&mut *seen.borrow_mut()))
}

/// An editor on [`TEXT`] whose first line is read-only, reporting what
/// `on_change` sees.
fn editor() -> (Owner, EditorState) {
    let (owner, state) = TestEditor {
        read_only_ranges: vec![TextRange::new(
            TextPosition::new(1, 1),
            TextPosition::new(1, 5),
        )],
        on_change: Some(Callback::new(|text: String| {
            let state = STATE.with(std::cell::Cell::get).expect("mounted");
            let seen = Seen {
                text,
                caret: state.doc.caret.get_untracked(),
                can_undo: state
                    .doc
                    .history
                    .with_untracked(|history| history.can_undo()),
            };
            SEEN.with(|all| all.borrow_mut().push(seen));
        })),
        ..TestEditor::new(Language::YAML, TEXT)
    }
    .mount();
    STATE.with(|cell| cell.set(Some(state)));
    seen();
    take_trace();
    (owner, state)
}

fn text(state: EditorState) -> String {
    state.doc.text.get_untracked()
}

fn can_undo(state: EditorState) -> bool {
    state
        .doc
        .history
        .with_untracked(|history| history.can_undo())
}

fn can_redo(state: EditorState) -> bool {
    state
        .doc
        .history
        .with_untracked(|history| history.can_redo())
}

/// `TEXT` with `insert` typed at the end of `b:`.
fn typed_after_b(insert: &str) -> (String, usize) {
    let mut text = TEXT.to_string();
    text.insert_str(FOLDED_LINE_END, insert);
    (text, FOLDED_LINE_END + insert.len())
}

/// `TEXT` with the read-only `a: 1` changed.
fn read_only_changed() -> String {
    TEXT.replacen("a: 1", "a: 2", 1)
}

#[test]
fn an_edit_passes_the_stages_in_order_and_on_change_sees_it_whole() {
    let (_owner, state) = editor();
    let (after, caret) = typed_after_b(" # note");
    assert!(state.commit(Edit::new(TEXT.into(), (7, 7), after.clone(), caret)));
    assert_eq!(
        take_trace(),
        [
            Stage::Prepare,
            Stage::Check,
            Stage::Apply,
            Stage::Record,
            Stage::Notify
        ]
    );
    // Notified last: the text, the caret and the undo step are all there.
    assert_eq!(
        seen(),
        [Seen {
            text: after,
            caret,
            can_undo: true
        }]
    );
}

#[test]
fn a_refused_edit_changes_nothing() {
    let (_owner, state) = editor();
    // A key about to edit at the end of a folded line opens the fold.
    assert!(state.fold_line(1));
    let closed = state.fold.folded.get_untracked();
    state.open_folds_for_edit(FOLDED_LINE_END, FOLDED_LINE_END);
    assert!(state.fold.folded.with_untracked(Vec::is_empty));
    let dirty = state.editing.dirty.get_untracked();

    let refused = Edit::new(TEXT.into(), (7, 7), read_only_changed(), 7);
    assert!(!state.commit(refused));

    assert_eq!(take_trace(), [Stage::Prepare, Stage::Check, Stage::Refused]);
    assert_eq!(text(state), TEXT);
    assert_eq!(
        state.fold.folded.get_untracked(),
        closed,
        "folds closed again"
    );
    assert!(!can_undo(state), "nothing recorded");
    assert_eq!(state.editing.dirty.get_untracked(), dirty);
    assert_eq!(seen(), [], "no on_change");
    assert_eq!(
        state.doc.caret.get_untracked(),
        7,
        "the selection it started from"
    );
    // The reason shows at the caret.
    assert!(state.protect.tip.with_untracked(Option::is_some));
}

#[test]
fn read_only_refuses_every_edit_of_the_user() {
    let (_owner, state) = TestEditor {
        read_only: true,
        ..TestEditor::new(Language::YAML, TEXT)
    }
    .mount();
    let (after, caret) = typed_after_b(" x");
    assert!(!state.commit(Edit::new(TEXT.into(), (7, 7), after, caret)));
    assert_eq!(text(state), TEXT);
    // Silently: there is no field to blame.
    assert!(state.protect.tip.with_untracked(Option::is_none));
}

#[test]
fn undo_of_an_edit_that_opened_a_fold_restores_the_text_and_leaves_it_open() {
    let (_owner, state) = editor();
    assert!(state.fold_line(1));
    state.open_folds_for_edit(FOLDED_LINE_END, FOLDED_LINE_END);
    let (after, caret) = typed_after_b(" # note");
    assert!(state.commit(Edit::new(TEXT.into(), (7, 7), after.clone(), caret)));
    assert!(state.fold.folded.with_untracked(Vec::is_empty));
    seen();

    let undo = state.history_edit(false).expect("one step");
    assert!(state.commit(undo));
    assert_eq!(text(state), TEXT);
    assert_eq!(state.doc.caret.get_untracked(), FOLDED_LINE_END);
    assert!(
        state.fold.folded.with_untracked(Vec::is_empty),
        "the fold stays open"
    );
    assert!(!can_undo(state) && can_redo(state));
    assert_eq!(
        seen(),
        [Seen {
            text: TEXT.into(),
            caret: FOLDED_LINE_END,
            can_undo: false
        }]
    );

    let redo = state.history_edit(true).expect("one step");
    assert!(state.commit(redo));
    assert_eq!(text(state), after);
    assert!(can_undo(state) && !can_redo(state));
}

#[test]
fn the_hosts_own_text_passes_the_read_only_check() {
    let (_owner, state) = editor();
    let edit = || Edit::new(TEXT.into(), (0, 0), read_only_changed(), 0);
    assert!(!state.commit(edit()));
    assert!(state.commit(edit().reloading(read_only_changed())));
    assert_eq!(text(state), read_only_changed());
    assert!(can_undo(state), "one undo step");
    assert_eq!(seen().len(), 1, "sent through on_change");
}

#[test]
fn a_refused_undo_keeps_the_history() {
    let (_owner, state) = editor();
    // The host's text changed the read-only value; undoing it would too.
    assert!(state.commit(
        Edit::new(TEXT.into(), (0, 0), read_only_changed(), 0).reloading(read_only_changed())
    ));
    let undo = state.history_edit(false).expect("one step");
    take_trace();
    assert!(!state.commit(undo));
    assert_eq!(take_trace(), [Stage::Prepare, Stage::Check, Stage::Refused]);
    assert_eq!(text(state), read_only_changed());
    assert!(
        can_undo(state) && !can_redo(state),
        "the step is still there"
    );
}

#[test]
fn typing_is_grouped_into_one_undo_step() {
    let (_owner, state) = editor();
    let (one, after_one) = typed_after_b(" x");
    let (two, after_two) = typed_after_b(" xy");
    let input = |before: &str, text: &str, caret: usize, time: f64| {
        Edit::input(
            before.into(),
            (caret - 1, caret - 1),
            text.into(),
            (caret, caret),
            "insertText".into(),
            time,
        )
    };
    // " x" arrives as two characters; the first one is a space.
    let (space, after_space) = typed_after_b(" ");
    assert!(state.commit(input(TEXT, &space, after_space, 0.0)));
    assert!(state.commit(input(&space, &one, after_one, 10.0)));
    assert!(state.commit(input(&one, &two, after_two, 20.0)));
    assert_eq!(text(state), two);
    let undo = state.history_edit(false).expect("one step");
    assert!(state.commit(undo));
    assert_eq!(text(state), TEXT);
    assert!(!can_undo(state));
}

#[test]
fn a_host_value_is_neither_recorded_nor_sent_back() {
    let (_owner, state) = editor();
    let (after, _) = typed_after_b(" # from the host");
    take_host_value(state, after.clone());
    assert_eq!(
        take_trace(),
        [
            Stage::Prepare,
            Stage::Check,
            Stage::Apply,
            Stage::Record,
            Stage::Notify
        ]
    );
    assert_eq!(text(state), after);
    assert!(!can_undo(state));
    assert_eq!(seen(), []);
}
