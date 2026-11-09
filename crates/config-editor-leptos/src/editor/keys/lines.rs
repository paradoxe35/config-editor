//! The line commands, as in VS Code: `Alt+↑` / `Alt+↓` move the caret's
//! lines (or the selected ones) up / down, `Shift+Alt+↑` / `Shift+Alt+↓`
//! copy them up / down, `Ctrl+Shift+K` deletes them, `Ctrl+]` / `Ctrl+[`
//! indent / outdent them. Each is one edit through the pipeline: next to a
//! lock, or in a read-only editor, it is refused and says why, as typing
//! is (QA 28).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::Edit;
use super::super::keyboard::utf16_to_byte_index;
use super::super::state::{EditorState, is_backward, textarea_selection};

/// A line command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Move { down: bool },
    Copy { down: bool },
    Delete,
    Indent { outdent: bool },
}

/// The command `event` asks for.
fn command(event: &KeyboardEvent) -> Option<Command> {
    let control = event.ctrl_key() || event.meta_key();
    let (shift, alt) = (event.shift_key(), event.alt_key());
    match event.key().as_str() {
        "ArrowUp" | "ArrowDown" if alt && !control => {
            let down = event.key() == "ArrowDown";
            Some(if shift {
                Command::Copy { down }
            } else {
                Command::Move { down }
            })
        }
        "k" | "K" if control && shift && !alt => Some(Command::Delete),
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
    let (start, end) = textarea_selection(&element);
    let backward = is_backward(&element);
    if let Command::Indent { outdent } = command {
        let text = state.doc().text.get_untracked();
        super::tab::indent_block(state, text, (start, end), outdent, backward);
        return true;
    }
    // Folds the lines it moves past open first (closed again if refused).
    let text = state.doc().text.get_untracked();
    let (from, to) = reach(&text, start, end);
    state.open_folds_for_edit(from, to);
    let text = state.doc().text.get_untracked();
    let Some(edit) = line_edit(&text, (start, end), command) else {
        return true;
    };
    let (anchor, caret) = if backward {
        (edit.selection.1, edit.selection.0)
    } else {
        edit.selection
    };
    state.commit(Edit::new(text, (start, end), edit.text, caret).selecting_from(anchor));
    state.close_suggestions();
    true
}

/// The text after a line command, and the selection then (UTF-16).
#[derive(Debug, PartialEq)]
struct LineEdit {
    text: String,
    selection: (usize, usize),
}

/// The UTF-16 stretch a command on the lines of `start..end` may touch:
/// those lines and the ones just above and below.
fn reach(text: &str, start: usize, end: usize) -> (usize, usize) {
    let (first, last) = block(text, start, end);
    let above = text[..first.saturating_sub(1)]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let below = text[last..]
        .find('\n')
        .map_or(text.len(), |at| last + at + 1);
    (units(&text[..above]), units(&text[..below]))
}

/// The whole lines of the selection `start..end` (UTF-16) as a byte range:
/// from the first's start to past the last's newline (or the text's end).
/// A selection ending at a line's start leaves that line out.
fn block(text: &str, start: usize, end: usize) -> (usize, usize) {
    let (start, end) = (
        utf16_to_byte_index(text, start),
        utf16_to_byte_index(text, end),
    );
    let last_end = if end > start && text[..end].ends_with('\n') {
        end - 1
    } else {
        end
    };
    let first = text[..start].rfind('\n').map_or(0, |at| at + 1);
    let last = text[last_end..]
        .find('\n')
        .map_or(text.len(), |at| last_end + at + 1);
    (first, last)
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The edit `command` makes of the lines of `selection` in `text`; none
/// when it changes nothing (the first line moved up).
fn line_edit(text: &str, selection: (usize, usize), command: Command) -> Option<LineEdit> {
    let (first, last) = block(text, selection.0, selection.1);
    // The lines with a newline after each, the text's last included.
    let lines = with_newline(&text[first..last]);
    let ends_open = !text.ends_with('\n');
    let (before, after) = (&text[..first], &text[last..]);
    let shift = |by: isize| {
        let moved = |at: usize| at.checked_add_signed(by).unwrap_or(0);
        (moved(selection.0), moved(selection.1))
    };
    let length = units(&lines) as isize;
    let (joined, selection) = match command {
        Command::Move { down: false } => {
            if first == 0 {
                return None;
            }
            let above = before[..first - 1].rfind('\n').map_or(0, |at| at + 1);
            let moved_over = with_newline(&before[above..]);
            let joined = format!("{}{lines}{moved_over}{after}", &before[..above]);
            (joined, shift(-(units(&moved_over) as isize)))
        }
        Command::Move { down: true } => {
            if after.is_empty() {
                return None;
            }
            let next_end = after.find('\n').map_or(after.len(), |at| at + 1);
            let moved_over = with_newline(&after[..next_end]);
            let joined = format!("{before}{moved_over}{lines}{}", &after[next_end..]);
            (joined, shift(units(&moved_over) as isize))
        }
        Command::Copy { down } => {
            let joined = format!("{before}{lines}{lines}{after}");
            (joined, if down { shift(length) } else { selection })
        }
        Command::Delete => {
            let joined = format!("{before}{after}");
            let at = units(before);
            (joined, (at, at))
        }
        Command::Indent { .. } => return None,
    };
    // The text kept its last newline, or its lack of one.
    let text_after = if ends_open {
        joined.strip_suffix('\n').unwrap_or(&joined).to_string()
    } else {
        joined
    };
    let most = units(&text_after);
    Some(LineEdit {
        text: text_after,
        selection: (selection.0.min(most), selection.1.min(most)),
    })
}

/// `lines` with a newline at its end.
fn with_newline(lines: &str) -> String {
    if lines.ends_with('\n') {
        lines.to_string()
    } else {
        format!("{lines}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, LineEdit, line_edit};

    /// The text and selection (`|` marks the caret, `[` `]` a selection)
    /// after `command` on `marked`.
    fn run(marked: &str, command: Command) -> Option<String> {
        let caret = marked.find('|');
        let (start, end) = match caret {
            Some(at) => (at, at),
            None => (marked.find('[').unwrap(), marked.find(']').unwrap() - 1),
        };
        let text: String = marked.chars().filter(|c| !"|[]".contains(*c)).collect();
        let LineEdit { text, selection } = line_edit(&text, (start, end), command)?;
        let mut out = text.clone();
        if selection.0 == selection.1 {
            out.insert(selection.0, '|');
        } else {
            out.insert(selection.1, ']');
            out.insert(selection.0, '[');
        }
        Some(out)
    }

    #[test]
    fn alt_arrows_move_the_lines() {
        let up = Command::Move { down: false };
        let down = Command::Move { down: true };
        assert_eq!(
            run("a: 1\nb: |2\nc: 3\n", up).unwrap(),
            "b: |2\na: 1\nc: 3\n"
        );
        assert_eq!(
            run("a: 1\nb: |2\nc: 3\n", down).unwrap(),
            "a: 1\nc: 3\nb: |2\n"
        );
        assert_eq!(run("a: |1\nb: 2\n", up), None);
        assert_eq!(run("a: 1\nb: |2\n", down), None);
        // The last line without a newline: the text still has none.
        assert_eq!(run("a: 1\nb: |2", up).unwrap(), "b: |2\na: 1");
        assert_eq!(run("a: |1\nb: 2", down).unwrap(), "b: 2\na: |1");
        // Selected lines move together; a selection ending at a line's
        // start leaves that line.
        assert_eq!(run("x\n[a\nb\n]c\n", down).unwrap(), "x\nc\n[a\nb\n]");
        assert_eq!(run("x\n[a\nb]\nc\n", up).unwrap(), "[a\nb]\nx\nc\n");
    }

    #[test]
    fn shift_alt_arrows_copy_the_lines() {
        let up = Command::Copy { down: false };
        let down = Command::Copy { down: true };
        assert_eq!(run("a: |1\nb: 2\n", down).unwrap(), "a: 1\na: |1\nb: 2\n");
        assert_eq!(run("a: |1\nb: 2\n", up).unwrap(), "a: |1\na: 1\nb: 2\n");
        assert_eq!(run("a: 1\nb: |2", down).unwrap(), "a: 1\nb: 2\nb: |2");
    }

    #[test]
    fn ctrl_shift_k_deletes_the_lines() {
        assert_eq!(
            run("a: 1\nb: |2\nc: 3\n", Command::Delete).unwrap(),
            "a: 1\n|c: 3\n"
        );
        assert_eq!(run("a: 1\nb: |2", Command::Delete).unwrap(), "a: 1|");
        assert_eq!(run("[a\nb]\nc\n", Command::Delete).unwrap(), "|c\n");
    }
}
