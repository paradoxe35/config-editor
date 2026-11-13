//! Enter, Ctrl+Enter and Ctrl+Shift+Enter.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::Edit;
use super::super::keyboard::{replace_selection, utf16_to_byte_index};
use super::super::state::{EditorState, textarea_selection};
use super::enter_newline::newline_at;

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

/// The Enter key `pressed` with `selection` (UTF-16, before the edit, for
/// undo) in the document; typed at `time` (a key event's), a plain Enter
/// goes on with the typing around it as one undo step.
pub(in crate::editor) fn enter_at(
    state: EditorState,
    selection: (usize, usize),
    pressed: Pressed,
    time: Option<f64>,
) {
    let text = state.doc().text.get_untracked();
    let typed = |edit: Edit| match time {
        Some(time) if pressed == Pressed::Enter => edit.typed(LINE_BREAK, time),
        _ => edit,
    };
    if pressed == Pressed::LineAbove {
        open_line_above(state, text, selection);
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
        && let Some(updated) = break_in_indentation(&text, start)
    {
        state.commit(typed(Edit::new(text, selection, updated, start + 1)));
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
    let blank_before = text
        .encode_utf16()
        .skip(line_start)
        .take(start - line_start)
        .all(|unit| unit == u16::from(b' ') || unit == u16::from(b'\t'));
    if start == end && start > line_start && blank_before && line_end_of(&text, end) == end {
        start = line_start;
    }
    // `replaces` and `caret` are bytes; the cursor is in UTF-16 units.
    let end_byte = utf16_to_byte_index(&text, end);
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
        let rest = &text[replaced_byte..];
        let spacing = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let before_comment = spacing > 0 && behavior.comment_start(rest) == Some(spacing);
        if before_comment { 0 } else { spacing }
    } else {
        0
    };
    let replaced_end = end
        + text[end_byte..replaced_byte + following]
            .encode_utf16()
            .count();
    let start_byte = utf16_to_byte_index(&text, start);
    let start = start
        - text[start_byte.saturating_sub(newline.replaces_before)..start_byte]
            .encode_utf16()
            .count();
    let (updated, _) = replace_selection(&text, start, replaced_end, &newline.text);
    let caret = start + newline.text[..newline.caret].encode_utf16().count();
    state.commit(typed(Edit::new(text, selection, updated, caret)));
    state.close_suggestions();
}

/// `text` with a line break before the line whose leading indentation holds
/// `cursor` (strictly inside it, with text after it), or `None`.
pub(super) fn break_in_indentation(text: &str, cursor: usize) -> Option<String> {
    let line_start = line_start_of(text, cursor);
    let line_end = line_end_of(text, cursor);
    let units: Vec<u16> = text
        .encode_utf16()
        .skip(line_start)
        .take(line_end - line_start)
        .collect();
    let column = cursor - line_start;
    let indent = units
        .iter()
        .take_while(|unit| **unit == u16::from(b' ') || **unit == u16::from(b'\t'))
        .count();
    (column > 0 && column < indent && indent < units.len())
        .then(|| replace_selection(text, line_start, line_start, "\n").0)
}

/// Ctrl+Shift+Enter opens a line above, at this line's indent.
fn open_line_above(state: EditorState, text: String, selection: (usize, usize)) {
    let line_start = line_start_of(&text, selection.0);
    let indent: String = text[utf16_to_byte_index(&text, line_start)..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let (updated, _) = replace_selection(&text, line_start, line_start, &format!("{indent}\n"));
    let caret = line_start + indent.encode_utf16().count();
    state.commit(Edit::new(text, selection, updated, caret));
    state.close_suggestions();
}

/// After a value typed before a closer the editor wrote that ends it (the
/// quote in `["box|"]`, the end tag in `<mode>balance|</mode>`), Enter steps
/// past it, past brackets it wrote right after, and past the comma ending the
/// item, instead of splitting the value. An empty value (`"|"`) or bracket pair (`{|}`) still opens.
fn step_past_inserted_closers(state: EditorState, text: &str, mut start: usize) -> usize {
    let Some(first) = state.closer_at(start) else {
        return start;
    };
    let Some(opener) = first.opener else {
        return start;
    };
    if text[..utf16_to_byte_index(text, start)].ends_with(opener) {
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
    if text[utf16_to_byte_index(text, start)..].starts_with(',') {
        start += 1;
    }
    start
}

/// The UTF-16 offset where the line holding `offset` starts.
fn line_start_of(text: &str, offset: usize) -> usize {
    let byte = utf16_to_byte_index(text, offset);
    text[..text[..byte].rfind('\n').map_or(0, |index| index + 1)]
        .encode_utf16()
        .count()
}

/// The UTF-16 offset where the line holding `offset` ends (before its
/// newline).
fn line_end_of(text: &str, offset: usize) -> usize {
    let byte = utf16_to_byte_index(text, offset);
    text[..text[byte..]
        .find('\n')
        .map_or(text.len(), |index| byte + index)]
        .encode_utf16()
        .count()
}

#[cfg(test)]
mod tests {
    use super::break_in_indentation;

    /// Round 11: Enter inside a line's indentation keeps the line's column.
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
                break_in_indentation(text, cursor).as_deref(),
                expected,
                "{text:?} at {cursor}"
            );
        }
    }
}
