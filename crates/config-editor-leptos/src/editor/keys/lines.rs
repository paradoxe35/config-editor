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

use super::super::commands::indent_lines_through;
use super::super::edit::Edit;
use super::super::fold_ranges::FoldRange;
use super::super::state::{EditorState, is_backward, select, textarea_selection};
use super::line_edit::{Folded, LineCommand, block_lines, fold_units, line_edit};

/// A line command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
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
    let selection = textarea_selection(&element);
    let backward = is_backward(&element);
    let text = state.doc().text.get_untracked();
    let folds = state.with_closed_folds_untracked(<[_]>::to_vec);
    let units = fold_units(&text, &folds, state.behavior());
    match command {
        Command::Indent { outdent } => {
            indent(state, text, selection, (outdent, backward), (folds, &units));
        }
        Command::Lines(command) => {
            let folded = Folded {
                folds: &folds,
                units: &units,
            };
            let Some(edit) = line_edit(&text, selection, command, folded) else {
                return true;
            };
            let (anchor, caret) = if backward {
                (edit.selection.1, edit.selection.0)
            } else {
                edit.selection
            };
            state.commit(
                Edit::new(text, selection, edit.text, caret)
                    .selecting_from(anchor)
                    .folding(edit.folds),
            );
        }
    }
    state.close_suggestions();
    true
}

/// Indents (or outdents) the selection's lines, a folded line's hidden
/// ones with it, keeping the folds closed and the lines selected.
fn indent(
    state: EditorState,
    text: String,
    (start, end): (usize, usize),
    (outdent, backward): (bool, bool),
    (folds, units): (Vec<FoldRange>, &[FoldRange]),
) {
    let folded = Folded {
        folds: &folds,
        units,
    };
    let Some((_, through)) = block_lines(&text, (start, end), folded) else {
        return;
    };
    let unit = state.doc().indent.get_value();
    let Some((updated, selection_start, selection_end)) =
        indent_lines_through(&text, (start, end), through, outdent, &unit)
    else {
        return;
    };
    let (anchor, caret) = if backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    let made = if updated != text {
        state.commit(
            Edit::new(text, (start, end), updated, caret)
                .selecting_from(anchor)
                .folding(folds),
        )
    } else {
        state.note_caret(caret);
        true
    };
    if made && let Some(element) = state.textarea() {
        select(&element, selection_start, selection_end, backward);
    }
}
