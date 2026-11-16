//! Typing aids: comment toggling, auto-closed pairs and language closers, typing
//! over closers, re-indenting, and indentation-aware Backspace.

use crate::editor::language::ConfigLanguage;
use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{backspace_indent, toggled_change, toggled_selection, typed_closer};
use super::super::completion::ListMode;
use super::super::edit::{Assoc, Change, Edit};
use super::super::keyboard::{
    delete_empty_pair, is_closer, is_closing_bracket, is_quote, pair_of, paired_edit,
    skip_closing_pair,
};
use super::super::state::{EditorState, is_backward, select, textarea_selection};
use super::super::typing::Closer;
use super::{complete, typed_character, unmodified};
use crate::editor::text::{TextRange, TextSize, byte_of, byte_range, slice, text_size, utf16_of};

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
    // The language API counts the selection in UTF-16 units.
    let units = (utf16_of(&before, start), utf16_of(&before, end));
    let Some(toggled) = state.behavior().toggle_comments(&before, units.0, units.1) else {
        return true;
    };
    let Some((change, updated)) = toggled_change(&before, &toggled) else {
        return true;
    };
    let (selection_start, selection_end) =
        toggled_selection(&before, units, &updated, toggled.selection);
    let (selection_start, selection_end) = (
        byte_of(&updated, selection_start),
        byte_of(&updated, selection_end),
    );
    // The caret is the end that moves: the start of a backward selection.
    let (anchor, caret) = if backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    let restore = if backward { (end, start) } else { (start, end) };
    let edit = Edit::command(before.clone(), change, restore);
    // Refused (a read-only editor or field): the selection stays.
    if updated != before && !state.commit(edit.selecting(anchor, caret)) {
        return true;
    }
    select(&element, selection_start, selection_end, backward);
    state.close_suggestions();
    true
}

/// With auto-closing on: typing over closers, deleting an inserted pair,
/// re-indenting a closing line, what the language closes as it is typed, and opening pairs.
pub(super) fn pairs_and_tags(state: EditorState, event: &KeyboardEvent) -> bool {
    if !unmodified(event) {
        return false;
    }
    if caret_in_string_body(state) {
        return string_body_pairs(state, event);
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

/// Inside a multi-line string's body (a heredoc, a block scalar) no
/// bracket or quote pairs, but what the language closes there does (HCL's
/// `${` in a heredoc writes `}`), typed over and deleted with its opener.
fn string_body_pairs(state: EditorState, event: &KeyboardEvent) -> bool {
    let Some(element) = state.textarea() else {
        return false;
    };
    let (start, end) = textarea_selection(&element);
    if start != end {
        return false;
    }
    let ours = state.closer_at(start).is_some();
    match typed_character(event) {
        Some(typed) => {
            (ours && type_over_closer(state, event, start, typed))
                || auto_close(state, event, start, typed)
        }
        None => ours && event.key() == "Backspace" && delete_pair(state, event, start),
    }
}

/// Typed over: closers the editor wrote, and the quote that ends the
/// string the caret is in.
fn type_over_closer(
    state: EditorState,
    event: &KeyboardEvent,
    start: TextSize,
    typed: char,
) -> bool {
    let source = state.doc().text.get_untracked();
    // A `>` typed before a tag's own `>` (`<tcp|></tcp>`) in a markup
    // language moves over it rather than closing the tag a second time.
    if state
        .behavior()
        .markup_delimiters()
        .is_some_and(|(_, close)| typed == close)
    {
        if slice::after(&source, start).is_some_and(|rest| rest.starts_with(typed))
            && state
                .behavior()
                .lexical_state(&source, utf16_of(&source, start))
                .quote
                .is_none()
        {
            event.prevent_default();
            let past = start + TextSize::of(typed);
            stepped_over(state, event, start, past);
            state.place_caret(past);
            return true;
        }
        return false;
    }
    let ours = state.closer_at(start);
    // Any character of a completion the editor wrote (`</|tcp>`) is typed
    // over; otherwise only closing brackets and quotes are.
    if !ours.is_some_and(|closer| closer.completion) && !is_closer(state.behavior(), typed) {
        return false;
    }
    let ours = ours.is_some();
    let ends_string = is_quote(state.behavior(), typed)
        && state
            .behavior()
            .lexical_state(&source, utf16_of(&source, start))
            .quote
            == Some(typed);
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
        && closer.len > next - start
    {
        state.remember(Closer {
            start: next,
            len: closer.len - (next - start),
            ..closer
        });
    }
    stepped_over(state, event, start, next);
    state.place_caret(next);
    // The value or container is finished: its suggestions are done too.
    state.close_suggestions();
    true
}

/// Typing goes on past a closer typed over, in one undo step: the caret
/// steps from byte `from` to `to` of the text (unchanged by the step).
fn stepped_over(state: EditorState, event: &KeyboardEvent, from: TextSize, to: TextSize) {
    state
        .doc()
        .history
        .update(|history| history.stepped_over(from, to, event.time_stamp()));
}

/// A closer typed where the same one already follows: the `}` the editor
/// put on the next line when the block was opened (at the indent the typed
/// one would take), or the `</tcp>` written when `<tcp>` was. The caret
/// moves onto it instead of closing twice.
fn closer_already_there(
    state: EditorState,
    event: &KeyboardEvent,
    start: TextSize,
    typed: char,
) -> bool {
    let source = state.doc().text.get_untracked();
    let byte = usize::from(start.min(TextSize::of(source.as_str())));
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
    let (closer, typed_from) = if is_closing_bracket(state.behavior(), typed)
        && before.trim().is_empty()
    {
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
    // `</tcp` + `>` right before the `</tcp>` written for it: what was
    // typed of it goes, the caret steps past the one written.
    if closer.len() > 1 && after.starts_with(&closer) {
        event.prevent_default();
        let change = Change::replace(&source, byte_range(typed_from..byte), "");
        let past = byte + closer.len();
        state.commit(
            Edit::command(source, change, (start, start))
                .keeping_caret(text_size(past), Assoc::After)
                .typed("insertText", event.time_stamp()),
        );
        state.close_suggestions();
        return true;
    }
    // Alone on its line, the same closer alone on the next one, at the
    // indent of the line that opened the block: that block's closer.
    let target = state
        .behavior()
        .reindent_on_type(&source, utf16_of(&source, start), typed)
        .map(|indent| indent.len())
        .or_else(|| opener_indent(state.behavior(), &source[..line_start], typed));
    let same_place = next_line.trim() == closer && Some(indent_of(next_line)) == target;
    if !(after.trim().is_empty()
        && before[..typed_from - line_start].trim().is_empty()
        && same_place
        && line_end < source.len())
    {
        return false;
    }
    event.prevent_default();
    // This line goes; the caret steps past the closer on the next one.
    let change = Change::replace(&source, byte_range(line_start..next_start), "");
    let past = next_start + indent_of(next_line) + closer.len();
    state.commit(
        Edit::command(source, change, (start, start))
            .keeping_caret(text_size(past), Assoc::After)
            .typed("insertText", event.time_stamp()),
    );
    state.close_suggestions();
    true
}

/// The indent of the line holding the opener `closer` would close (one of
/// the language's brackets), the last one in `before` left unclosed
/// (quotes skipped).
fn opener_indent(behavior: &dyn ConfigLanguage, before: &str, closer: char) -> Option<usize> {
    let config = behavior.config();
    let (opener, _) = config.brackets.iter().find(|(_, close)| *close == closer)?;
    let opener = *opener;
    let opens_quote = |character: char| {
        config
            .quotes
            .iter()
            .any(|quote| quote.starts_with(character))
    };
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let characters: Vec<(usize, char)> = before.char_indices().collect();
    for &(at, character) in characters.iter().rev() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if opens_quote(character) => quote = Some(character),
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
fn delete_pair(state: EditorState, event: &KeyboardEvent, start: TextSize) -> bool {
    let Some(closer) = state.closer_at(start) else {
        return false;
    };
    let before = state.doc().text.get_untracked();
    // A quote run the editor wrote (`"""|"""`): one quote goes on each side.
    if let (Some(head), Some(tail)) = (slice::before(&before, start), slice::after(&before, start))
        && let Some(quote) = tail.chars().next()
        && closer.len > TextSize::of(quote)
        && is_quote(state.behavior(), quote)
        && head.ends_with(quote)
    {
        event.prevent_default();
        let quote_len = TextSize::of(quote);
        let change = Change::replace(
            &before,
            TextRange::new(start - quote_len, start + quote_len),
            "",
        );
        let edit = Edit::command(before, change, (start, start))
            .typed("deleteContentBackward", event.time_stamp());
        // The edit takes the closer it cuts; what is left of it stays.
        if state.commit(edit) {
            state.remember_closer(start - quote_len, closer.len - quote_len, closer.opener);
        }
        return true;
    }
    let Some(change) = delete_empty_pair(&before, start, state.behavior()) else {
        return false;
    };
    event.prevent_default();
    state.commit(
        Edit::command(before, change, (start, start))
            .typed("deleteContentBackward", event.time_stamp()),
    );
    true
}

/// Text the language closes as it is typed (`>` finishing `<k>` writes
/// `</k>`; `"""` writes its closing `"""`).
fn auto_close(state: EditorState, event: &KeyboardEvent, start: TextSize, typed: char) -> bool {
    let before = state.doc().text.get_untracked();
    let units = utf16_of(&before, start);
    let Some(closing) = state.behavior().auto_close(&before, units, typed) else {
        return false;
    };
    event.prevent_default();
    let change = Change::replace(
        &before,
        TextRange::empty(start),
        &format!("{typed}{closing}"),
    );
    // The caret after the typed character, the closing after it.
    let caret = start + TextSize::of(typed);
    let edit = Edit::command(before, change, (start, start))
        .caret_at(caret)
        .typed("insertText", event.time_stamp());
    if state.commit(edit)
        && let Some((len, opener)) = kept_closer(state.behavior(), &closing, typed)
    {
        state.remember_closer(caret, len, opener);
    }
    state.close_suggestions();
    true
}

/// The closer `closing`, which typing `typed` wrote, leaves to type over:
/// its length in bytes and the opener a value typed in it follows.
fn kept_closer(
    behavior: &dyn ConfigLanguage,
    closing: &str,
    typed: char,
) -> Option<(TextSize, Option<char>)> {
    if let Some(len) = behavior.closing_markup_len(closing) {
        // Closing markup is the editor's: Enter after a value steps past it.
        return Some((text_size(len.min(closing.len())), Some(typed)));
    }
    let mut characters = closing.chars();
    if characters
        .next()
        .is_some_and(|closer| is_closing_bracket(behavior, closer))
        && characters.next().is_none()
    {
        // A bracket the language closes (HCL's `${` in a string): typed
        // over, deleted with its opener.
        return Some((TextSize::of(closing), Some(typed)));
    }
    // A closing quote run (TOML's `"""`) is typed over and deleted with its
    // opener; Enter inside it is a line break of the string.
    (!closing.is_empty() && closing.chars().all(|quote| is_quote(behavior, quote)))
        .then(|| (TextSize::of(closing), None))
}

/// The history kind of a selection wrapped in a typed pair (see
/// `EditHistory::record_input`).
pub(in crate::editor) const WRAP: &str = "wrapSelection";

/// An opening bracket or quote writes its pair (or wraps the selection).
fn open_pair(
    state: EditorState,
    event: &KeyboardEvent,
    (start, end): (TextSize, TextSize),
    opening: char,
) -> bool {
    let before = state.doc().text.get_untracked();
    let Some((change, selection_start, selection_end)) =
        paired_edit(&before, start, end, opening, state.behavior())
    else {
        return false;
    };
    let backward = state
        .textarea()
        .is_some_and(|element| is_backward(&element));
    event.prevent_default();
    let wrapped_backward = backward && selection_start != selection_end;
    let (anchor, caret) = if wrapped_backward {
        (selection_end, selection_start)
    } else {
        (selection_start, selection_end)
    };
    // A wrapped selection is replaced by what is typed next, in the same
    // undo step.
    let kind = if start == end { "insertText" } else { WRAP };
    let restore = if backward { (end, start) } else { (start, end) };
    let edit = Edit::command(before, change, restore)
        .selecting(anchor, caret)
        .typed(kind, event.time_stamp());
    // A prevented keydown fires no input event, so open suggestions here
    // when the pair starts an entry.
    let opens_entry = selection_start == selection_end
        && state
            .behavior()
            .entry_started(&edit.text, utf16_of(&edit.text, selection_start));
    // A quote ends a string value; Enter opens a bracket pair instead of
    // stepping out of it.
    let opener = is_quote(state.behavior(), opening).then_some(opening);
    let closer_len = pair_of(state.behavior(), opening)
        .map_or(TextSize::of(opening), |(closing, _)| TextSize::of(closing));
    if state.commit(edit) {
        state.remember_closer(selection_end, closer_len, opener);
    }
    // A wrapped selection keeps its direction.
    if backward
        && selection_start != selection_end
        && let Some(element) = state.textarea()
    {
        select(&element, selection_start, selection_end, true);
    }
    state.set_list_mode(if opens_entry {
        ListMode::TYPED
    } else {
        ListMode::Closed
    });
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
    let byte = usize::from(start);
    if start != end || !slice::after(&text, start).is_some_and(|rest| rest.starts_with('\n')) {
        return false;
    }
    event.prevent_default();
    let rest = &text[byte + 1..];
    let indentation = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let change = Change::replace(&text, byte_range(byte..byte + 1 + indentation), "");
    state.commit(Edit::command(text, change, (start, end)));
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
    let key = event.key();
    let mut typed = key.chars();
    let edit = match (key.as_str(), typed.next(), typed.next()) {
        ("Backspace", _, _) => backspace_indent(&text, start, &state.doc().indent.get_value()),
        // A closing bracket of the language aligns with its opener.
        (_, Some(closer), None) => typed_closer(&text, start, closer, state.behavior()),
        _ => None,
    };
    let Some(change) = edit else {
        return false;
    };
    event.prevent_default();
    state.commit(Edit::command(text, change, (start, end)));
    true
}
