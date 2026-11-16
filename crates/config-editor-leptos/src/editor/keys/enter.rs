//! Enter, Ctrl+Enter and Ctrl+Shift+Enter.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::copied_lines;
use super::super::edit::{Assoc, Change, Edit};
use super::super::history::Selection;
use super::super::state::{EditorState, textarea_selection};
use super::enter_newline::newline_at;
use crate::editor::text::{TextRange, TextSize, byte_of, slice, text_size, utf16_of};

pub(super) fn enter(state: EditorState, event: &KeyboardEvent) {
    event.prevent_default();
    // Alt+Enter opens a problem's docs (see `a11y::open_docs_key`); with
    // none at the caret it does nothing, as in VS Code.
    if event.alt_key() {
        return;
    }
    let Some(element) = state.textarea() else {
        return;
    };
    let modified = event.ctrl_key() || event.meta_key();
    let pressed = if modified && event.shift_key() {
        Pressed::LineAbove
    } else if modified {
        Pressed::LineBelow
    } else {
        Pressed::Enter
    };
    enter_at(
        state,
        textarea_selection(&element),
        pressed,
        Some(event.time_stamp()),
    );
}

/// The history kind of a line break Enter types (the browser's own).
const LINE_BREAK: &str = "insertLineBreak";

/// Which of the Enter keys was pressed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::editor) enum Pressed {
    Enter,
    /// Ctrl+Enter: a line below, this one not split.
    LineBelow,
    /// Ctrl+Shift+Enter: a line above.
    LineAbove,
}

/// The Enter key `pressed` with `selection` (bytes, before the edit, for
/// undo) in the document; typed at `time` (a key event's), a plain Enter
/// goes on with the typing around it as one undo step.
pub(in crate::editor) fn enter_at(
    state: EditorState,
    selection: Selection,
    pressed: Pressed,
    time: Option<f64>,
) {
    let text = state.doc().text.get_untracked();
    let selection = {
        let len = TextSize::of(text.as_str());
        (selection.0.min(len), selection.1.min(len))
    };
    let typed = |edit: Edit| match time {
        Some(time) if pressed == Pressed::Enter => edit.typed(LINE_BREAK, time),
        _ => edit,
    };
    if pressed == Pressed::LineAbove {
        open_line_above(state, text, selection);
        return;
    }
    if pressed == Pressed::LineBelow
        && let Some(block_end) = folded_block_end(state, &text, selection.1)
    {
        open_line_after_fold(state, text, selection, block_end);
        return;
    }
    let modified = pressed == Pressed::LineBelow;
    let (mut start, mut end) = selection;
    // Ctrl+Enter opens a line below without splitting this one.
    if modified {
        start = line_end_of(&text, end);
        end = start;
    }
    if start == end {
        start = step_past_inserted_closers(state, &text, start);
        end = start;
    }
    // Inside a line's leading indentation, before its text: the line moves
    // down whole, its indentation kept, and the caret with it.
    if start == end
        && !modified
        && let Some(change) = break_in_indentation(&text, start)
    {
        let edit = Edit::command(text, change, selection).keeping_caret(start, Assoc::After);
        state.commit(typed(edit));
        state.close_suggestions();
        return;
    }
    // In a comment or a multi-line string's body the line break is text:
    // keep the line's indentation, nothing more.
    let behavior = state.behavior();
    state.settle_readings();
    let bodies = state.view().string_bodies.get_untracked();
    let in_body = |line: usize| bodies.iter().any(|lines| lines.contains(&line));
    let newline = newline_at(behavior, &text, start, &in_body);
    // A line of only indentation is left empty, not with trailing spaces.
    let line_start = line_start_of(&text, start);
    let blank_before = (line_start <= start)
        .then(|| slice::slice(&text, TextRange::new(line_start, start)))
        .flatten()
        .is_some_and(|before| before.bytes().all(|byte| matches!(byte, b' ' | b'\t')));
    if start == end && start > line_start && blank_before && line_end_of(&text, end) == end {
        start = line_start;
    }
    // `replaces` and `caret` are bytes of what the language writes.
    let end_byte = usize::from(end);
    let replaced_byte = (end_byte + newline.replaces).min(text.len());
    // When the new line writes its own indentation (`|    }` moving the
    // closer to its opener's indent), indentation already after the caret is
    // replaced, not added to. Enter at column 0 of an indented line writes
    // none, and the line keeps its indentation.
    let ends_indented = newline.text.contains('\n')
        && newline
            .text
            .rsplit('\n')
            .next()
            .is_some_and(|tail| !tail.is_empty() && tail.trim().is_empty());
    // The spacing before a comment stays (`l2|  # mc`): in YAML it is what
    // makes the comment one once text is typed before it.
    let following = if ends_indented {
        let rest = slice::after(&text, text_size(replaced_byte)).unwrap_or_default();
        let spacing = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let before_comment = spacing > 0 && behavior.comment_start(rest) == Some(spacing);
        if before_comment { 0 } else { spacing }
    } else {
        0
    };
    let replaced_end = text_size(replaced_byte + following);
    let start = text_size(usize::from(start).saturating_sub(newline.replaces_before));
    let change = Change::replace(
        &text,
        TextRange::new(start, replaced_end.max(start)),
        &newline.text,
    );
    // Where the language puts the caret in what it writes.
    let caret = start + text_size(newline.caret.min(newline.text.len()));
    state.commit(typed(
        Edit::command(text, change, selection).caret_at(caret),
    ));
    state.close_suggestions();
}

/// [`enter_at`] with `selection` in UTF-16 units, as the tests write it.
#[cfg(test)]
#[allow(dead_code, reason = "used by the tests of some builds only")]
pub(in crate::editor) fn enter_units(
    state: EditorState,
    selection: (usize, usize),
    pressed: Pressed,
    time: Option<f64>,
) {
    let selection = state
        .doc()
        .text
        .with_untracked(|text| (byte_of(text, selection.0), byte_of(text, selection.1)));
    enter_at(state, selection, pressed, time);
}

/// A line break before the line whose leading indentation holds byte `at`
/// (strictly inside it, with text after it), or `None`.
pub(super) fn break_in_indentation(text: &str, at: TextSize) -> Option<Change> {
    let line_start = line_start_of(text, at);
    let line = slice::after(text, line_start)?.split('\n').next()?;
    let column = at.checked_sub(line_start)?;
    let indent = TextSize::of(line) - TextSize::of(line.trim_start_matches([' ', '\t']));
    (column > TextSize::default() && column < indent && indent < TextSize::of(line))
        .then(|| Change::replace(text, TextRange::empty(line_start), "\n"))
}

/// Ctrl+Shift+Enter opens a line above, at this line's indent.
fn open_line_above(state: EditorState, text: String, selection: Selection) {
    let line_start = line_start_of(&text, selection.0);
    let indent = indent_at(&text, line_start);
    let change = Change::replace(&text, TextRange::empty(line_start), &format!("{indent}\n"));
    // On the new line, after its indent.
    let caret = line_start + TextSize::of(indent.as_str());
    state.commit(Edit::command(text, change, selection).caret_at(caret));
    state.close_suggestions();
}

/// Where Ctrl+Enter on a folded line opens its line: after the whole block
/// the fold closes (its hidden lines and a closing line it leaves shown),
/// as the other line commands take a fold; `None` on a line not folded.
fn folded_block_end(state: EditorState, text: &str, caret: TextSize) -> Option<TextSize> {
    state.view().projection.with_untracked(|projection| {
        projection.hidden_from(line_end_of(text, caret))?;
        // The copied lines are counted in UTF-16 units.
        let (_, end) = copied_lines(state.behavior(), text, utf16_of(text, caret), projection);
        let end = byte_of(text, end);
        // Before the line break that ends the block's last line.
        let ends_line = slice::before(text, end).is_some_and(|head| head.ends_with('\n'));
        Some(if ends_line {
            end - TextSize::of('\n')
        } else {
            end
        })
    })
}

/// Ctrl+Enter on a folded line: a line after the block at `block_end`, at
/// the folded line's own indentation (beside the block, not in it), the
/// fold kept closed.
fn open_line_after_fold(
    state: EditorState,
    text: String,
    selection: Selection,
    block_end: TextSize,
) {
    let indent = indent_at(&text, line_start_of(&text, selection.1));
    // Written at the start of the line after the block, not at the end of
    // its last (hidden) line, which would open the fold.
    let next_line = block_end + TextSize::of('\n');
    let edit = if next_line <= TextSize::of(text.as_str()) {
        let change = Change::replace(&text, TextRange::empty(next_line), &format!("{indent}\n"));
        let caret = next_line + TextSize::of(indent.as_str());
        Edit::command(text, change, selection).caret_at(caret)
    } else {
        let change = Change::replace(&text, TextRange::empty(block_end), &format!("\n{indent}"));
        Edit::command(text, change, selection)
    };
    state.commit(edit);
    state.close_suggestions();
}

/// The indentation of the line starting at byte `line_start`.
fn indent_at(text: &str, line_start: TextSize) -> String {
    slice::after(text, line_start)
        .unwrap_or_default()
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect()
}

/// After a value typed before a closer the editor wrote that ends it (the
/// quote in `["box|"]`, the end tag in `<mode>balance|</mode>`), Enter steps
/// past it, past brackets it wrote right after, and past the comma ending the
/// item, instead of splitting the value. An empty value (`"|"`) or bracket pair (`{|}`) still opens.
fn step_past_inserted_closers(state: EditorState, text: &str, mut start: TextSize) -> TextSize {
    let Some(first) = state.closer_at(start) else {
        return start;
    };
    let Some(opener) = first.opener else {
        return start;
    };
    if slice::before(text, start).is_some_and(|head| head.ends_with(opener)) {
        return start;
    }
    start += first.len;
    while let Some(next) = state.closer_at(start)
        && next.opener.is_none()
    {
        start += next.len;
    }
    // The comma ending the item comes along, so the next item starts on the
    // new line (`"cert": "a",|` rather than `"cert": "a"|,`).
    if slice::after(text, start).is_some_and(|rest| rest.starts_with(',')) {
        start += TextSize::of(',');
    }
    start
}

/// The byte where the line holding byte `offset` starts.
fn line_start_of(text: &str, offset: TextSize) -> TextSize {
    let head = slice::before(text, offset).unwrap_or_default();
    text_size(head.rfind('\n').map_or(0, |index| index + 1))
}

/// The byte where the line holding byte `offset` ends (before its
/// newline).
fn line_end_of(text: &str, offset: TextSize) -> TextSize {
    let rest = slice::after(text, offset).unwrap_or_default();
    offset + TextSize::of(rest.split('\n').next().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::break_in_indentation;
    use crate::editor::text::TextSize;

    /// Enter inside a line's indentation keeps the line's column.
    #[test]
    fn enter_inside_indentation_moves_the_line_down_whole() {
        for (text, cursor, expected) in [
            ("a:\n  b: 1\n  c: 2", 4, Some("a:\n\n  b: 1\n  c: 2")),
            ("{\n    \"b\": 1\n}", 4, Some("{\n\n    \"b\": 1\n}")),
            ("<a>\n\t\t<b/>\n</a>", 5, Some("<a>\n\n\t\t<b/>\n</a>")),
            // At the line start, at the text, or on a blank line: Enter as usual.
            ("a:\n  b: 1", 3, None),
            ("a:\n  b: 1", 5, None),
            ("a:\n    \nb: 1", 5, None),
        ] {
            assert_eq!(
                break_in_indentation(text, TextSize::from(cursor))
                    .and_then(|change| change.apply(text))
                    .as_deref(),
                expected,
                "{text:?} at {cursor}"
            );
        }
    }
}
