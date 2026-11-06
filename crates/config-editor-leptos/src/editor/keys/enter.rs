//! Enter, Ctrl+Enter and Ctrl+Shift+Enter.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::keyboard::{replace_selection, utf16_to_byte_index};
use super::super::language::{Newline, in_string_body};
use super::super::state::{Edit, EditorState, textarea_selection};

pub(super) fn enter(state: EditorState, event: &KeyboardEvent) {
    event.prevent_default();
    let Some(element) = state.textarea() else {
        return;
    };
    // The selection before the edit, for undo.
    let selection = textarea_selection(&element);
    let text = state.source.get_untracked();
    let modified = event.ctrl_key() || event.meta_key();
    if modified && event.shift_key() {
        open_line_above(state, text, selection);
        return;
    }
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
    // In a comment or a multi-line string's body the line break is text:
    // keep the line's indentation, nothing more.
    let behavior = state.behavior();
    let newline = if behavior.lexical_state(&text, start).in_comment
        || in_string_body(behavior, &text, start)
    {
        let line_start = line_start_of(&text, start);
        let indent: String = text[utf16_to_byte_index(&text, line_start)..]
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .collect();
        Newline::new(format!("\n{indent}"))
    } else {
        behavior.newline(&text, start)
    };
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
    let following = if ends_indented {
        let rest = &text[replaced_byte..];
        rest.len() - rest.trim_start_matches([' ', '\t']).len()
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
    state.commit(Edit::new(text, selection, updated, caret));
    state.close_suggestions();
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
