//! Go to line (`Ctrl+G`): a small field taking `line` or `line:column`.

use leptos::prelude::*;

use super::super::state::{EditorState, line_count};

/// The go-to-line field's state.
#[derive(Clone, Copy)]
pub(in crate::editor) struct GoToLine {
    pub(super) open: RwSignal<bool>,
    pub(super) text: RwSignal<String>,
    pub(super) widget_ref: NodeRef<leptos::html::Div>,
    pub(super) input_ref: NodeRef<leptos::html::Input>,
    /// Distinguishes this editor's hint element from other editors'.
    pub(super) id: usize,
}

static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl GoToLine {
    pub(in crate::editor) fn new() -> Self {
        Self {
            open: RwSignal::new(false),
            text: RwSignal::new(String::new()),
            widget_ref: NodeRef::new(),
            input_ref: NodeRef::new(),
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }
}

/// One-based `(line, column)` from `line`, `line:column` or `line,column`;
/// `None` for anything else, zero included.
pub(super) fn parse_position(input: &str) -> Option<(usize, Option<usize>)> {
    let input = input.trim();
    let (line, column) = match input.split_once([':', ',']) {
        Some((line, column)) => (line, Some(column)),
        None => (input, None),
    };
    let number = |text: &str| {
        let text = text.trim();
        (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| text.parse::<usize>().ok())
            .flatten()
            .filter(|number| *number > 0)
    };
    let line = number(line)?;
    match column {
        Some(column) => Some((line, Some(number(column)?))),
        None => Some((line, None)),
    }
}

/// Opens the field and focuses it.
pub(super) fn open(state: EditorState) {
    let go_to = state.go_to_line();
    super::super::input::flush_input(state);
    go_to.text.set(String::new());
    go_to.open.set(true);
    // Shown now rather than when the view catches up, so the digits typed
    // right after Ctrl+G land in the field.
    if let Some(widget) = go_to.widget_ref.get_untracked() {
        widget.set_hidden(false);
    }
    state.close_suggestions();
    state.hide_hover();
    if let Some(input) = go_to.input_ref.get_untracked() {
        input.set_value("");
        let _ = input.focus();
    }
}

/// Closes the field; the caret stays where it was.
pub(super) fn cancel(state: EditorState) {
    close(state);
    if let Some(element) = state.textarea() {
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus();
        element.set_scroll_top(top);
        element.set_scroll_left(left);
    }
}

/// Moves the caret to the typed position, in view; `false` when the text
/// is not a position (the field stays open).
pub(super) fn go(state: EditorState) -> bool {
    let Some((line, column)) = state
        .go_to_line()
        .text
        .with_untracked(|text| parse_position(text))
    else {
        return false;
    };
    let line = line.min(state.doc().text.with_untracked(|text| line_count(text)));
    close(state);
    state.focus_at(line, column.unwrap_or(1));
    state.forget_typing_aids();
    true
}

/// Leaving the field (a click elsewhere) closes it.
pub(super) fn cancel_on_blur(state: EditorState) {
    if state.go_to_line().open.get_untracked() {
        close(state);
    }
}

fn close(state: EditorState) {
    let go_to = state.go_to_line();
    go_to.open.set(false);
    if let Some(widget) = go_to.widget_ref.get_untracked() {
        widget.set_hidden(true);
    }
}

/// The field's hint: the range of lines, or what is wrong with the text.
pub(super) fn hint(strings: &crate::EditorStrings, text: &str, lines: usize) -> String {
    let template = if text.trim().is_empty() || parse_position(text).is_some() {
        &strings.go_to_line_hint
    } else {
        &strings.go_to_line_invalid
    };
    crate::strings::fill(template, &[("lines", &lines)])
}

#[cfg(test)]
mod tests {
    use super::{hint, parse_position};

    #[test]
    fn positions_parse_as_line_or_line_and_column() {
        assert_eq!(parse_position("12"), Some((12, None)));
        assert_eq!(parse_position(" 12 "), Some((12, None)));
        assert_eq!(parse_position("12:5"), Some((12, Some(5))));
        assert_eq!(parse_position("12,5"), Some((12, Some(5))));
        assert_eq!(parse_position("12 : 5"), Some((12, Some(5))));
        assert_eq!(parse_position("1"), Some((1, None)));
    }

    #[test]
    fn anything_else_is_not_a_position() {
        for text in [
            "",
            " ",
            "0",
            "0:1",
            "1:0",
            "-3",
            "+3",
            "x",
            "12:",
            ":5",
            "1:2:3",
            "1.5",
            "１２",
            "99999999999999999999999",
        ] {
            assert_eq!(parse_position(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_hint_says_what_to_type() {
        let strings = crate::EditorStrings::default();
        assert_eq!(hint(&strings, "", 40), "Line 1–40, or line:column");
        assert_eq!(hint(&strings, "3:4", 40), "Line 1–40, or line:column");
        assert_eq!(
            hint(&strings, "abc", 40),
            "Type a line number between 1 and 40"
        );
    }
}
