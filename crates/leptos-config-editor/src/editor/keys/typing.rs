//! Typing aids: comment toggling, auto-closed pairs and end tags, typing
//! over closers, re-indenting, and indentation-aware Backspace.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{backspace_indent, typed_closer};
use super::super::keyboard::{
    delete_empty_pair, paired_edit, replace_selection, skip_closing_pair, utf16_to_byte_index,
};
use super::super::state::{Edit, EditorState, textarea_selection};
use super::{typed_character, unmodified};

/// Ctrl+/ comments or uncomments the selected lines.
pub(super) fn toggle_comments(state: EditorState, event: &KeyboardEvent) -> bool {
    let key = event.key().to_ascii_lowercase();
    if !((event.ctrl_key() || event.meta_key())
        && (event.code() == "Slash" || key == "/" || key == "?"))
    {
        return false;
    }
    event.prevent_default();
    let Some(element) = state.textarea() else {
        return true;
    };
    let (start, end) = textarea_selection(&element);
    let before = state.source.get_untracked();
    let Some((updated, selection_start, selection_end)) =
        state.behavior().toggle_comments(&before, start, end)
    else {
        return true;
    };
    if updated != before {
        state.commit(
            Edit::new(before, (start, end), updated, selection_end).selecting_from(selection_start),
        );
    } else {
        state.note_caret(selection_end);
    }
    let _ = element.set_selection_range(selection_start as u32, selection_end as u32);
    state.close_suggestions();
    true
}

/// With auto-closing on: typing over closers, deleting an inserted pair,
/// re-indenting a closing line, closing XML tags, and opening pairs.
pub(super) fn pairs_and_tags(state: EditorState, event: &KeyboardEvent) -> bool {
    if !unmodified(event) {
        return false;
    }
    let Some(element) = state.textarea() else {
        return false;
    };
    let (start, end) = textarea_selection(&element);
    let typed = typed_character(event);
    if start == end
        && let Some(typed) = typed
        && (type_over_closer(state, event, start, typed)
            || reindent(state, event, start, typed)
            || close_tag(state, event, start, typed))
    {
        return true;
    }
    if start == end && event.key() == "Backspace" && delete_pair(state, event, start) {
        return true;
    }
    match typed {
        Some(opening) => open_pair(state, event, (start, end), opening),
        None => false,
    }
}

/// Typed over: closers the editor wrote, and the quote that ends the
/// string the caret is in.
fn type_over_closer(state: EditorState, event: &KeyboardEvent, start: usize, typed: char) -> bool {
    if !matches!(typed, ')' | ']' | '}' | '"' | '\'') {
        return false;
    }
    let source = state.source.get_untracked();
    let distance = source.encode_utf16().count() - start;
    let ours = state
        .auto_closers
        .with_value(|closers| closers.contains(&distance));
    let ends_string = matches!(typed, '"' | '\'')
        && state.behavior().lexical_state(&source, start).quote == Some(typed);
    if !(ours || ends_string) {
        return false;
    }
    let Some(next) = skip_closing_pair(&source, start, typed) else {
        return false;
    };
    event.prevent_default();
    state
        .auto_closers
        .update_value(|closers| closers.retain(|closer| *closer != distance));
    state.place_caret(next);
    true
}

/// Backspace between a pair the editor inserted deletes both.
fn delete_pair(state: EditorState, event: &KeyboardEvent, start: usize) -> bool {
    let before = state.source.get_untracked();
    let distance = before.encode_utf16().count() - start;
    if !state
        .auto_closers
        .with_value(|closers| closers.contains(&distance))
    {
        return false;
    }
    let Some((updated, caret)) = delete_empty_pair(&before, start) else {
        return false;
    };
    event.prevent_default();
    state
        .auto_closers
        .update_value(|closers| closers.retain(|closer| *closer != distance));
    state.commit(Edit::new(before, (start, start), updated, caret));
    true
}

/// A line that only now shows it closes a block re-indents.
fn reindent(state: EditorState, event: &KeyboardEvent, start: usize, typed: char) -> bool {
    let before = state.source.get_untracked();
    let Some(indent) = state.behavior().reindent_on_type(&before, start, typed) else {
        return false;
    };
    event.prevent_default();
    let byte = utf16_to_byte_index(&before, start);
    let line_start = before[..byte].rfind('\n').map_or(0, |index| index + 1);
    let content = before[line_start..byte].trim_start();
    let line_start_utf16 = before[..line_start].encode_utf16().count();
    let (updated, caret) = replace_selection(
        &before,
        line_start_utf16,
        start,
        &format!("{indent}{content}{typed}"),
    );
    state.commit(Edit::new(before, (start, start), updated, caret));
    state.close_suggestions();
    true
}

/// `>` closing an XML start tag writes its end tag after the caret.
fn close_tag(state: EditorState, event: &KeyboardEvent, start: usize, typed: char) -> bool {
    let before = state.source.get_untracked();
    let Some(end_tag) = state.behavior().auto_close(&before, start, typed) else {
        return false;
    };
    event.prevent_default();
    let (updated, _) = replace_selection(&before, start, start, &format!("{typed}{end_tag}"));
    let caret = start + typed.len_utf16();
    // The end tag is the editor's: Enter after a value steps past it.
    if end_tag.starts_with("</") {
        let distance = updated.encode_utf16().count() - caret;
        state
            .auto_closers
            .update_value(|closers| closers.push(distance));
    }
    state.commit(Edit::new(before, (start, start), updated, caret));
    state.close_suggestions();
    true
}

/// An opening bracket or quote writes its pair (or wraps the selection).
fn open_pair(
    state: EditorState,
    event: &KeyboardEvent,
    (start, end): (usize, usize),
    opening: char,
) -> bool {
    let before = state.source.get_untracked();
    let Some((updated, selection_start, selection_end)) =
        paired_edit(&before, start, end, opening, state.behavior())
    else {
        return false;
    };
    event.prevent_default();
    // A prevented keydown fires no input event, so open suggestions here
    // when the pair starts an entry.
    let opens_entry = selection_start == selection_end
        && state.behavior().entry_started(&updated, selection_start);
    let distance = updated.encode_utf16().count() - selection_end;
    state
        .auto_closers
        .update_value(|closers| closers.push(distance));
    state.commit(
        Edit::new(before, (start, end), updated, selection_end).selecting_from(selection_start),
    );
    state.suggestions_requested.set(opens_entry);
    true
}

/// Backspace in indentation removes one unit; a closer on a blank line
/// aligns with its opener.
pub(super) fn indentation(state: EditorState, event: &KeyboardEvent) -> bool {
    if !unmodified(event) {
        return false;
    }
    let Some(element) = state.textarea() else {
        return false;
    };
    let (start, end) = textarea_selection(&element);
    if start != end {
        return false;
    }
    let text = state.source.get_untracked();
    let edit = match event.key().as_str() {
        "Backspace" => backspace_indent(&text, start),
        "}" => typed_closer(&text, start, '}'),
        "]" => typed_closer(&text, start, ']'),
        ")" => typed_closer(&text, start, ')'),
        _ => None,
    };
    let Some((updated, caret)) = edit else {
        return false;
    };
    event.prevent_default();
    state.commit(Edit::new(text, (start, end), updated, caret));
    true
}
