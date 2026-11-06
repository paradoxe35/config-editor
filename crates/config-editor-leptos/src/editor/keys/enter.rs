//! Enter, Ctrl+Enter and Ctrl+Shift+Enter.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::keyboard::{replace_selection, utf16_to_byte_index};
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
    let newline = state.behavior().newline(&text, start);
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
    // Indentation already after the caret is replaced by the new line's
    // own, not added to it.
    let ends_indented = newline.text.contains('\n')
        && newline
            .text
            .rsplit('\n')
            .next()
            .is_some_and(|tail| tail.trim().is_empty());
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
    state.commit(Edit::new(text, selection, updated, caret).revealed());
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
    state.commit(Edit::new(text, selection, updated, caret).revealed());
    state.close_suggestions();
}

/// After a value typed inside closers the editor inserted (`["box|"]`), or
/// before an end tag it wrote (`<mode>balance|</mode>`), Enter steps past
/// them instead of splitting the value. An empty pair (`{|}`) still opens.
fn step_past_inserted_closers(state: EditorState, text: &str, mut start: usize) -> usize {
    let length = text.encode_utf16().count();
    let ours = |at: usize| {
        state
            .auto_closers
            .with_value(|closers| closers.contains(&(length - at)))
    };
    let at_byte = utf16_to_byte_index(text, start);
    let after_typed_quote =
        text[at_byte..].starts_with(['"', '\'']) && !text[..at_byte].ends_with(['"', '\'']);
    if after_typed_quote {
        while start < length
            && ours(start)
            && text[utf16_to_byte_index(text, start)..].starts_with(['"', '\'', ']', '}', ')'])
        {
            start += 1;
        }
    }
    let at_byte = utf16_to_byte_index(text, start);
    let rest = &text[at_byte..];
    let line_start = text[..at_byte].rfind('\n').map_or(0, |index| index + 1);
    let typed_value = !text[line_start..at_byte].trim_end().ends_with('>');
    if rest.starts_with("</")
        && typed_value
        && ours(start)
        && let Some(close) = rest.find('>')
    {
        start += rest[..=close].encode_utf16().count();
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
