//! Typing aids: comment toggling, auto-closed pairs and language closers, typing
//! over closers, re-indenting, and indentation-aware Backspace.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{backspace_indent, typed_closer};
use super::super::keyboard::{
    delete_empty_pair, paired_edit, replace_selection, skip_closing_pair, utf16_to_byte_index,
};
use super::super::language::in_string_body;
use super::super::state::{Edit, EditorState, is_backward, select, textarea_selection};
use super::{typed_character, unmodified};

/// Whether the caret is in a multi-line string's body, where typing is text.
fn caret_in_string_body(state: EditorState) -> bool {
    state
        .textarea()
        .map(|element| textarea_selection(&element).0)
        .is_some_and(|caret| {
            state
                .source
                .with_untracked(|text| in_string_body(state.behavior(), text, caret))
        })
}

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
    let backward = is_backward(&element);
    let before = state.source.get_untracked();
    let Some((updated, selection_start, selection_end)) =
        state.behavior().toggle_comments(&before, start, end)
    else {
        return true;
    };
    // The caret is the end that moves: the start of a backward selection.
    let (anchor, caret) = if backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    if updated != before {
        state.commit(Edit::new(before, (start, end), updated, caret).selecting_from(anchor));
    } else {
        state.note_caret(caret);
    }
    select(&element, selection_start, selection_end, backward);
    state.close_suggestions();
    true
}

/// With auto-closing on: typing over closers, deleting an inserted pair,
/// re-indenting a closing line, what the language closes as it is typed, and opening pairs.
pub(super) fn pairs_and_tags(state: EditorState, event: &KeyboardEvent) -> bool {
    if !unmodified(event) || caret_in_string_body(state) {
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
            || auto_close(state, event, start, typed))
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
    let ours = state.closer_at(start).is_some();
    let ends_string = matches!(typed, '"' | '\'')
        && state.behavior().lexical_state(&source, start).quote == Some(typed);
    if !(ours || ends_string) {
        return false;
    }
    let Some(next) = skip_closing_pair(&source, start, typed) else {
        return false;
    };
    event.prevent_default();
    let closer = state.closer_at(start);
    state.forget_closer_at(start);
    // A longer closer (`"""`) is typed over one quote at a time.
    if let Some(closer) = closer
        && closer.len > 1
    {
        state.remember_closer(&source, next, closer.len - 1, closer.opener);
    }
    state.place_caret(next);
    true
}

/// Backspace between a pair the editor inserted deletes both.
fn delete_pair(state: EditorState, event: &KeyboardEvent, start: usize) -> bool {
    let Some(closer) = state.closer_at(start) else {
        return false;
    };
    let before = state.source.get_untracked();
    // A quote run the editor wrote (`"""|"""`): one quote goes on each side.
    if closer.len > 1 && start > 0 {
        let byte = utf16_to_byte_index(&before, start);
        let after = before[byte..].chars().next();
        if after.is_some_and(|quote| matches!(quote, '"' | '\''))
            && before[..byte].ends_with(after.unwrap_or_default())
        {
            event.prevent_default();
            let quote_len = after.map_or(1, char::len_utf8);
            let mut updated = before.clone();
            updated.replace_range(byte - quote_len..byte + quote_len, "");
            state.forget_closer_at(start);
            state.remember_closer(&updated, start - 1, closer.len - 1, closer.opener);
            state.commit(Edit::new(before, (start, start), updated, start - 1));
            return true;
        }
    }
    let Some((updated, caret)) = delete_empty_pair(&before, start) else {
        return false;
    };
    event.prevent_default();
    state.forget_closer_at(start);
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

/// Text the language closes as it is typed (`>` finishing `<k>` writes
/// `</k>`; `"""` writes its closing `"""`).
fn auto_close(state: EditorState, event: &KeyboardEvent, start: usize, typed: char) -> bool {
    let before = state.source.get_untracked();
    let Some(closing) = state.behavior().auto_close(&before, start, typed) else {
        return false;
    };
    event.prevent_default();
    let (updated, _) = replace_selection(&before, start, start, &format!("{typed}{closing}"));
    let caret = start + typed.len_utf16();
    // Closing markup is the editor's: Enter after a value steps past it.
    if let Some(len) = state.behavior().closing_markup_len(&closing) {
        state.remember_closer(
            &updated,
            caret,
            closing[..len].encode_utf16().count(),
            Some(typed),
        );
    } else if !closing.is_empty() && closing.chars().all(|quote| matches!(quote, '"' | '\'')) {
        // A closing quote run (TOML's `"""`) is typed over and deleted with
        // its opener; Enter inside it is a line break of the string.
        state.remember_closer(&updated, caret, closing.encode_utf16().count(), None);
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
    let backward = state
        .textarea()
        .is_some_and(|element| is_backward(&element));
    event.prevent_default();
    // A prevented keydown fires no input event, so open suggestions here
    // when the pair starts an entry.
    let opens_entry = selection_start == selection_end
        && state.behavior().entry_started(&updated, selection_start);
    // A quote ends a string value; Enter opens a bracket pair instead of
    // stepping out of it.
    let opener = matches!(opening, '"' | '\'').then_some(opening);
    state.remember_closer(&updated, selection_end, 1, opener);
    let wrapped_backward = backward && selection_start != selection_end;
    let (anchor, caret) = if wrapped_backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    state.commit(Edit::new(before, (start, end), updated, caret).selecting_from(anchor));
    // A wrapped selection keeps its direction.
    if backward
        && selection_start != selection_end
        && let Some(element) = state.textarea()
    {
        select(&element, selection_start, selection_end, true);
    }
    state.suggestions_requested.set(opens_entry);
    true
}

/// Backspace in indentation removes one unit; a closer on a blank line
/// aligns with its opener.
pub(super) fn indentation(state: EditorState, event: &KeyboardEvent) -> bool {
    if !unmodified(event) || (event.key() != "Backspace" && caret_in_string_body(state)) {
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
        "}" => typed_closer(&text, start, '}', state.behavior()),
        "]" => typed_closer(&text, start, ']', state.behavior()),
        ")" => typed_closer(&text, start, ')', state.behavior()),
        _ => None,
    };
    let Some((updated, caret)) = edit else {
        return false;
    };
    event.prevent_default();
    state.commit(Edit::new(text, (start, end), updated, caret));
    true
}
