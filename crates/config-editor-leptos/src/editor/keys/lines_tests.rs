//! The line commands through the editor: a closed fold moved as one unit
//! stays closed, and so it is again when the move is undone and redone (undo
//! used to restore the text with the fold open).

use leptos::prelude::*;

use super::super::fold_ranges::FoldRange;
use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::line_edit::LineCommand;
use super::lines::{Command, run};

#[test]
fn a_closed_fold_moved_stays_closed_through_undo_and_redo() {
    let text = "a:\n  x: 1\n  y: 2\nb: 3\n";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    state.set_folds(vec![FoldRange::new(0, 2)]);
    let closed = || state.with_closed_folds_untracked(<[_]>::to_vec);
    run(
        state,
        Command::Lines(LineCommand::Move { down: true }),
        (0, 0),
        false,
    );
    let moved = "b: 3\na:\n  x: 1\n  y: 2\n";
    assert_eq!(state.doc().text.get_untracked(), moved);
    assert_eq!(closed(), [FoldRange::new(1, 3)], "moved");
    let undo = state.history_edit(false).expect("a step");
    assert!(state.commit(undo));
    assert_eq!(state.doc().text.get_untracked(), text);
    assert_eq!(closed(), [FoldRange::new(0, 2)], "undone");
    let redo = state.history_edit(true).expect("a step undone");
    assert!(state.commit(redo));
    assert_eq!(state.doc().text.get_untracked(), moved);
    assert_eq!(closed(), [FoldRange::new(1, 3)], "redone");
}
