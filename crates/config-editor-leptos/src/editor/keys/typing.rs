//! Typing aids: comment toggling, auto-closed pairs and language closers, typing
//! over closers, re-indenting, and indentation-aware Backspace.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{backspace_indent, toggled_selection, typed_closer};
use super::super::edit::Edit;
use super::super::keyboard::{
    delete_empty_pair, paired_edit, replace_selection, skip_closing_pair, utf16_to_byte_index,
};
use super::super::state::{EditorState, is_backward, select, textarea_selection};
use super::super::typing::Closer;
use super::{complete, typed_character, unmodified};

/// Whether the caret is in a multi-line string's body, where typing is text.
fn caret_in_string_body(state: EditorState) -> bool {
    state
        .textarea()
        .map(|element| textarea_selection(&element).0)
        .is_some_and(|caret| state.in_string_body(caret))
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
    let before = state.doc().text.get_untracked();
    let Some((updated, selection_start, selection_end)) =
        state.behavior().toggle_comments(&before, start, end)
    else {
        return true;
    };
    let (selection_start, selection_end) = toggled_selection(
        &before,
        (start, end),
        &updated,
        (selection_start, selection_end),
    );
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
            || closer_already_there(state, event, start, typed)
            || complete::reindent(state, event, start, typed)
            || complete::complete(state, event, start, typed)
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
    let source = state.doc().text.get_untracked();
    // A `>` typed before a tag's own `>` (`<tcp|></tcp>`) in a markup
    // language moves over it rather than closing the tag a second time.
    if typed == '>' && state.behavior().closing_markup_len("</a>").is_some() {
        let byte = utf16_to_byte_index(&source, start);
        if source[byte..].starts_with('>')
            && state
                .behavior()
                .lexical_state(&source, start)
                .quote
                .is_none()
        {
            event.prevent_default();
            state.place_caret(start + 1);
            return true;
        }
        return false;
    }
    let ours = state.closer_at(start);
    // Any character of a completion the editor wrote (`</|tcp>`) is typed
    // over; otherwise only closing brackets and quotes are.
    if !ours.is_some_and(|closer| closer.completion)
        && !matches!(typed, ')' | ']' | '}' | '"' | '\'')
    {
        return false;
    }
    let ours = ours.is_some();
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
        state.remember(Closer {
            distance: closer.distance - (next - start),
            len: closer.len - (next - start),
            ..closer
        });
    }
    state.place_caret(next);
    // The value or container is finished: its suggestions are done too.
    state.close_suggestions();
    state.completion().explicit.set(false);
    true
}

/// A closer typed where the same one already follows: the `}` the editor
/// put on the next line when the block was opened (at the indent the typed
/// one would take), or the `</tcp>` written when `<tcp>` was. The caret
/// moves onto it instead of closing twice.
fn closer_already_there(
    state: EditorState,
    event: &KeyboardEvent,
    start: usize,
    typed: char,
) -> bool {
    let source = state.doc().text.get_untracked();
    let byte = utf16_to_byte_index(&source, start);
    let line_start = source[..byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[byte..]
        .find('\n')
        .map_or(source.len(), |at| byte + at);
    let before = &source[line_start..byte];
    let after = &source[byte..line_end];
    let next_start = (line_end + 1).min(source.len());
    let next_line = source[next_start..].split('\n').next().unwrap_or("");
    let indent_of = |text: &str| text.len() - text.trim_start().len();
    // What the typed character finishes on this line: a bracket alone, or
    // markup the language closes elements with (`</tcp` + `>`).
    let (closer, typed_from) = if matches!(typed, '}' | ']' | ')') && before.trim().is_empty() {
        (typed.to_string(), byte)
    } else {
        // Markup the language closes elements with, ending with the typed
        // character (`<tcp></tcp` + `>`): the nearest start that makes one.
        let behavior = state.behavior();
        let found = before
            .char_indices()
            .rev()
            .take(256)
            .map(|(start, _)| (start, format!("{}{typed}", &before[start..])))
            .find(|(_, candidate)| behavior.closing_markup_len(candidate) == Some(candidate.len()));
        match found {
            Some((tag_start, candidate)) => (candidate, line_start + tag_start),
            None => return false,
        }
    };
    // `</tcp` + `>` right before the `</tcp>` written for it.
    if closer.len() > 1 && after.starts_with(&closer) {
        event.prevent_default();
        let mut updated = source.clone();
        updated.replace_range(typed_from..byte, "");
        let caret = updated[..typed_from + closer.len()].encode_utf16().count();
        state.commit(Edit::new(source, (start, start), updated, caret));
        state.close_suggestions();
        return true;
    }
    // Alone on its line, the same closer alone on the next one, at the
    // indent of the line that opened the block: that block's closer.
    let target = state
        .behavior()
        .reindent_on_type(&source, start, typed)
        .map(|indent| indent.len())
        .or_else(|| opener_indent(&source[..line_start], typed));
    let same_place = next_line.trim() == closer && Some(indent_of(next_line)) == target;
    if !(after.trim().is_empty()
        && before[..typed_from - line_start].trim().is_empty()
        && same_place
        && line_end < source.len())
    {
        return false;
    }
    event.prevent_default();
    let mut updated = source.clone();
    updated.replace_range(line_start..next_start, "");
    let caret = updated[..line_start + indent_of(next_line) + closer.len()]
        .encode_utf16()
        .count();
    state.commit(Edit::new(source, (start, start), updated, caret));
    state.close_suggestions();
    true
}

/// The indent of the line holding the opener `closer` would close, the
/// last one in `before` left unclosed (quotes skipped).
fn opener_indent(before: &str, closer: char) -> Option<usize> {
    let opener = match closer {
        '}' => '{',
        ']' => '[',
        ')' => '(',
        _ => return None,
    };
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let characters: Vec<(usize, char)> = before.char_indices().collect();
    for &(at, character) in characters.iter().rev() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == closer => depth += 1,
            None if character == opener => {
                if depth == 0 {
                    let line_start = before[..at].rfind('\n').map_or(0, |index| index + 1);
                    let line = &before[line_start..];
                    return Some(line.len() - line.trim_start().len());
                }
                depth -= 1;
            }
            None => {}
        }
    }
    None
}

/// Backspace between a pair the editor inserted deletes both.
fn delete_pair(state: EditorState, event: &KeyboardEvent, start: usize) -> bool {
    let Some(closer) = state.closer_at(start) else {
        return false;
    };
    let before = state.doc().text.get_untracked();
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

/// Text the language closes as it is typed (`>` finishing `<k>` writes
/// `</k>`; `"""` writes its closing `"""`).
fn auto_close(state: EditorState, event: &KeyboardEvent, start: usize, typed: char) -> bool {
    let before = state.doc().text.get_untracked();
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
    } else if matches!(closing.as_str(), ")" | "]" | "}") {
        // A bracket the language closes (HCL's `${` in a string): typed
        // over, deleted with its opener.
        state.remember_closer(&updated, caret, 1, Some(typed));
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
    let before = state.doc().text.get_untracked();
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
    state.completion().requested.set(opens_entry);
    true
}

/// Ctrl+Delete at the end of a line joins the next one to it, dropping its
/// indentation, as in VS Code (the browser would delete its first word too).
pub(super) fn join_with_next_line(state: EditorState, event: &KeyboardEvent) -> bool {
    if !(event.ctrl_key() || event.meta_key()) || event.shift_key() || event.key() != "Delete" {
        return false;
    }
    let Some(element) = state.textarea() else {
        return false;
    };
    let (start, end) = textarea_selection(&element);
    let text = state.doc().text.get_untracked();
    let byte = utf16_to_byte_index(&text, start);
    if start != end || !text[byte..].starts_with('\n') {
        return false;
    }
    event.prevent_default();
    let rest = &text[byte + 1..];
    let indentation = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let removed = text[byte..byte + 1 + indentation].encode_utf16().count();
    let (updated, _) = replace_selection(&text, start, start + removed, "");
    state.commit(Edit::new(text, (start, end), updated, start));
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
    let text = state.doc().text.get_untracked();
    let edit = match event.key().as_str() {
        "Backspace" => backspace_indent(&text, start, &state.doc().indent.get_value()),
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
