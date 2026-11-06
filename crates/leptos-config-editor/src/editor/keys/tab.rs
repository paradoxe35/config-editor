//! Tab and Shift+Tab: placeholder slots, block indent, and tab stops.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::commands::{indent_lines, line_range, tab_insert};
use super::super::completion::next_slot;
use super::super::keyboard::{replace_selection, utf16_to_byte_index};
use super::super::state::{Edit, EditorState, textarea_selection};

pub(super) fn tab(state: EditorState, event: &KeyboardEvent) {
    event.prevent_default();
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let text = state.source.get_untracked();
    if !event.shift_key() && start == end && next_placeholder_slot(state, &text, start) {
        return;
    }
    if event.shift_key() || spans_lines(&text, start, end) {
        indent_block(state, text, (start, end), event.shift_key());
        return;
    }
    // A selection within one line is replaced, as typing would.
    let (updated, caret) = replace_selection(&text, start, end, &tab_insert(&text, start));
    state.commit(Edit::new(text, (start, end), updated, caret).revealed());
    state.close_suggestions();
}

/// In a block a suggestion just wrote, Tab goes to its next empty value, as
/// through a snippet's placeholders.
fn next_placeholder_slot(state: EditorState, text: &str, caret: usize) -> bool {
    let Some((region_start, from_end)) = state.slot_region.get_value() else {
        return false;
    };
    let region_end = text.encode_utf16().count().saturating_sub(from_end);
    let slot = (caret >= region_start && caret <= region_end)
        .then(|| next_slot(text, caret, region_end))
        .flatten();
    match slot {
        Some(slot) => {
            state.place_caret(slot);
            true
        }
        None => {
            state.slot_region.set_value(None);
            false
        }
    }
}

/// Whether Tab indents the selected lines rather than replacing the
/// selection: it spans lines, or covers one whole line (as in VS Code).
fn spans_lines(text: &str, start: usize, end: usize) -> bool {
    if start == end {
        return false;
    }
    let (line_start, line_end) = line_range(text, start);
    let content_end = line_end
        - usize::from(text.encode_utf16().nth(line_end.wrapping_sub(1)) == Some(u16::from(b'\n')));
    let indent_end = line_start
        + text[utf16_to_byte_index(text, line_start)..]
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .count();
    let whole_line = start <= indent_end && end >= content_end;
    whole_line
        || text
            .encode_utf16()
            .skip(start)
            .take(end - start)
            .any(|unit| unit == u16::from(b'\n'))
}

/// Indents (or with Shift outdents) the selected lines, keeping them
/// selected.
fn indent_block(state: EditorState, text: String, (start, end): (usize, usize), outdent: bool) {
    let Some((updated, selection_start, selection_end)) = indent_lines(&text, start, end, outdent)
    else {
        return;
    };
    if updated != text {
        state.commit(
            Edit::new(text, (start, end), updated, selection_end).selecting_from(selection_start),
        );
    } else {
        state.note_caret(selection_end);
    }
    // Keep the block selected.
    if let Some(element) = state.textarea() {
        let _ = element.set_selection_range(selection_start as u32, selection_end as u32);
    }
    state.close_suggestions();
}
