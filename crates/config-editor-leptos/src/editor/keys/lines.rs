//! The line commands, as in VS Code: `Alt+↑` / `Alt+↓` move the caret's
//! lines (or the selected ones) up / down, `Shift+Alt+↑` / `Shift+Alt+↓`
//! copy them up / down, `Ctrl+Shift+K` deletes them, `Ctrl+]` / `Ctrl+[`
//! indent / outdent them. A closed fold is one unit: a folded line takes
//! its hidden lines along and stays folded, and a line moves past a fold
//! whole (see `line_edit`). Each is one edit through the pipeline: next to
//! a lock, or in a read-only editor, it is refused and says why, as typing
//! is.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::bridge::links::Splice;
use super::super::commands::indent_lines_through;
use super::super::edit::{Edit, change_of_splices};
use super::super::fold_ranges::FoldRange;
use super::super::history::Selection;
use super::super::state::{EditorState, is_backward, select, textarea_selection};
use super::line_edit::{Folded, LineCommand, block_lines, fold_units, line_edit, neighbor_units};
use crate::editor::text::{byte_of, utf16_of};

/// A line command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum Command {
    Lines(LineCommand),
    Indent { outdent: bool },
}

/// The command `event` asks for.
fn command(event: &KeyboardEvent) -> Option<Command> {
    let control = event.ctrl_key() || event.meta_key();
    let (shift, alt) = (event.shift_key(), event.alt_key());
    match event.key().as_str() {
        "ArrowUp" | "ArrowDown" if alt && !control => {
            let down = event.key() == "ArrowDown";
            Some(Command::Lines(if shift {
                LineCommand::Copy { down }
            } else {
                LineCommand::Move { down }
            }))
        }
        "k" | "K" if control && shift && !alt => Some(Command::Lines(LineCommand::Delete)),
        "]" | "[" if control && !shift && !alt => Some(Command::Indent {
            outdent: event.key() == "[",
        }),
        _ => None,
    }
}

/// Runs the line command `event` asks for; whether it was one.
pub(super) fn line_commands(state: EditorState, event: &KeyboardEvent) -> bool {
    let Some(command) = command(event) else {
        return false;
    };
    event.prevent_default();
    // In a read-only editor, any of them says it cannot edit (as typing
    // does), even where it would change nothing.
    if state.config().read_only.get_untracked() {
        state.note_read_only();
        return true;
    }
    let Some(element) = state.textarea() else {
        return true;
    };
    run(
        state,
        command,
        textarea_selection(&element),
        is_backward(&element),
    );
    true
}

/// Runs `command` on `selection` (bytes, made backward with `backward`).
pub(in crate::editor) fn run(
    state: EditorState,
    command: Command,
    selection: Selection,
    backward: bool,
) {
    let text = state.doc().text.get_untracked();
    // The line edits count the text in UTF-16 units, as the folds they
    // keep do (`FoldRange`).
    let units = |(start, end): Selection| (utf16_of(&text, start), utf16_of(&text, end));
    let folds = state.with_closed_folds_untracked(<[_]>::to_vec);
    let fold_spans = fold_units(&text, &folds, state.behavior());
    match command {
        Command::Indent { outdent } => {
            indent(
                state,
                text,
                selection,
                (outdent, backward),
                (folds, &fold_spans),
            );
        }
        Command::Lines(command) => {
            let moving = matches!(command, LineCommand::Move { .. });
            // Lines holding a closed fold move past whole constructs, and
            // take no other line into it.
            let neighbors = moving
                .then(|| neighbors(state, &text, units(selection), (&folds, &fold_spans)))
                .flatten();
            let folded = Folded {
                folds: &folds,
                units: &fold_spans,
                neighbors: neighbors.as_deref().unwrap_or_default(),
            };
            let edit = line_edit(&text, units(selection), command, folded)
                .filter(|edit| neighbors.is_none() || folds_kept(state, &edit.text, &edit.folds));
            if let Some(edit) = edit {
                let selected = (
                    byte_of(&edit.text, edit.selection.0),
                    byte_of(&edit.text, edit.selection.1),
                );
                let (anchor, caret) = if backward {
                    (selected.1, selected.0)
                } else {
                    selected
                };
                let restore = if backward {
                    (selection.1, selection.0)
                } else {
                    selection
                };
                // The lines it rewrote, those it moved or copied with them.
                let change = Splice::lines(&text, &edit.text, &edit.pieces)
                    .and_then(|splice| change_of_splices(&text, &[splice]))
                    .filter(|change| change.apply(&text).as_deref() == Some(edit.text.as_str()));
                let made = match change {
                    Some(change) => Edit::command(text, change, restore),
                    None => Edit::new(text, restore, edit.text, caret),
                };
                state.commit(with_folds(
                    made.selecting(anchor, caret),
                    &folds,
                    edit.folds,
                ));
            }
        }
    }
    state.close_suggestions();
}

/// [`run`] on a UTF-16 `selection`, as the tests write it.
#[cfg(test)]
#[allow(dead_code, reason = "used by the tests of some builds only")]
pub(in crate::editor) fn run_units(
    state: EditorState,
    command: Command,
    selection: (usize, usize),
    backward: bool,
) {
    let selection = state
        .doc()
        .text
        .with_untracked(|text| (byte_of(text, selection.0), byte_of(text, selection.1)));
    run(state, command, selection, backward);
}

/// The constructs a move of `selection`'s lines goes past whole, when they
/// hold a closed fold (see [`neighbor_units`]); `None` when they hold none.
fn neighbors(
    state: EditorState,
    text: &str,
    selection: (usize, usize),
    (folds, units): (&[FoldRange], &[FoldRange]),
) -> Option<Vec<FoldRange>> {
    if folds.is_empty() {
        return None;
    }
    let folded = Folded {
        folds,
        units,
        neighbors: &[],
    };
    let lines = block_lines(text, selection, folded)?;
    let foldable = state.foldable_untracked();
    neighbor_units(text, lines, folds, &foldable, state.behavior())
}

/// Whether each of `folds`, closed as a move left them in `text`, still
/// folds just so there: the move took no other line into one (a YAML
/// block moved above its sibling's last child, a TOML table above a key
/// with no table).
fn folds_kept(state: EditorState, text: &str, folds: &[FoldRange]) -> bool {
    let foldable = state.behavior().fold_ranges(text);
    folds.iter().all(|fold| foldable.contains(fold))
}

/// `edit`, leaving `after` closed when folds were (`before`): with none,
/// nothing is to be kept, and the readings of the whole text wait for the
/// frame, as after a typed key.
fn with_folds(edit: Edit, before: &[FoldRange], after: Vec<FoldRange>) -> Edit {
    if before.is_empty() {
        edit
    } else {
        edit.folding(after)
    }
}

/// Indents (or outdents) the selection's lines, a folded line's hidden
/// ones with it, keeping the folds closed and the lines selected.
fn indent(
    state: EditorState,
    text: String,
    (start, end): Selection,
    (outdent, backward): (bool, bool),
    (folds, units): (Vec<FoldRange>, &[FoldRange]),
) {
    let folded = Folded {
        folds: &folds,
        units,
        neighbors: &[],
    };
    let lines = (utf16_of(&text, start), utf16_of(&text, end));
    let Some((_, through)) = block_lines(&text, lines, folded) else {
        return;
    };
    let unit = state.doc().indent.get_value();
    let Some(indented) = indent_lines_through(&text, (start, end), through, outdent, &unit) else {
        return;
    };
    let restore = if backward { (end, start) } else { (start, end) };
    let edit = super::tab::selected_through(
        Edit::command(text, indented.change, restore),
        indented.selection,
        backward,
    );
    let (selection_start, selection_end) = edit.selected();
    let made = if edit.before != edit.text {
        let kept = folds.clone();
        state.commit(with_folds(edit, &folds, kept))
    } else {
        true
    };
    if made && let Some(element) = state.textarea() {
        select(&element, selection_start, selection_end, backward);
    }
}
