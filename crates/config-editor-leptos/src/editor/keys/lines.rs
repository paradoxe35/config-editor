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
use super::line_edit::{Folded, LineCommand, block_lines, fold_units, line_edit, neighbor_units};

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

/// Runs `command` on `selection` (made backward with `backward`).
pub(in crate::editor) fn run(
    state: EditorState,
    command: Command,
    selection: (usize, usize),
    backward: bool,
) {
    let text = state.doc().text.get_untracked();
    let folds = state.with_closed_folds_untracked(<[_]>::to_vec);
    let units = fold_units(&text, &folds, state.behavior());
    match command {
        Command::Indent { outdent } => {
            indent(state, text, selection, (outdent, backward), (folds, &units));
        }
        Command::Lines(command) => {
            let moving = matches!(command, LineCommand::Move { .. });
            // Lines holding a closed fold move past whole constructs, and
            // take no other line into it.
            let neighbors = moving
                .then(|| neighbors(state, &text, selection, (&folds, &units)))
                .flatten();
            let folded = Folded {
                folds: &folds,
                units: &units,
                neighbors: neighbors.as_deref().unwrap_or_default(),
            };
            let edit = line_edit(&text, selection, command, folded)
                .filter(|edit| neighbors.is_none() || folds_kept(state, &edit.text, &edit.folds));
            if let Some(edit) = edit {
                let (anchor, caret) = if backward {
                    (edit.selection.1, edit.selection.0)
                } else {
                    edit.selection
                };
                let made = Edit::new(text, selection, edit.text, caret).selecting_from(anchor);
                state.commit(with_folds(made, &folds, edit.folds));
            }
        }
    }
    state.close_suggestions();
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
    (start, end): (usize, usize),
    (outdent, backward): (bool, bool),
    (folds, units): (Vec<FoldRange>, &[FoldRange]),
) {
    let folded = Folded {
        folds: &folds,
        units,
        neighbors: &[],
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
        let made = Edit::new(text, (start, end), updated, caret).selecting_from(anchor);
        let kept = folds.clone();
        state.commit(with_folds(made, &folds, kept))
    } else {
        state.note_caret(caret);
        true
    };
    if made && let Some(element) = state.textarea() {
        select(&element, selection_start, selection_end, backward);
    }
}
