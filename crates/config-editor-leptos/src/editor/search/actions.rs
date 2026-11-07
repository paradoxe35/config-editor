//! What the find widget does to the editor: opening and closing it,
//! stepping through matches, and replacing them through the editor's normal
//! edit path (history, `on_change`, Edit-as).

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::input::flush_input;
use super::super::keyboard::utf16_to_byte_index;
use super::super::positioning::reveal_centered;
use super::super::state::{EditorState, textarea_selection};
use super::matcher::MAX_MATCHES;
use super::replace;
use super::{SearchOptions, SearchResults};

/// Longest selection taken as the query when the widget opens.
const SEED_MAX_CHARS: usize = 200;

/// Opens the widget (with the replace row for `Ctrl+H` in an editable
/// editor), taking a one-line selection as the query, and focuses it.
pub(in crate::editor) fn open(state: EditorState, replacing: bool) {
    flush_input(state);
    let search = state.search;
    if let Some(seed) = selected_text(state).filter(|seed| {
        !seed.is_empty() && !seed.contains('\n') && seed.chars().count() <= SEED_MAX_CHARS
    }) {
        search.query.set(seed);
    }
    let replacing = replacing && !state.config.read_only.get_untracked();
    search.open.set(true);
    if replacing {
        search.replacing.set(true);
    }
    search.notice.set(None);
    // Shown now rather than when the view catches up, so keys typed right
    // after Ctrl+F land in the widget.
    if let Some(widget) = search.widget_ref.get_untracked() {
        widget.set_hidden(false);
    }
    if let Some(row) = search.replace_row_ref.get_untracked() {
        row.set_hidden(!search.replacing.get_untracked());
    }
    state.close_suggestions();
    state.hide_hover();
    let query_empty = search.query.with_untracked(String::is_empty);
    let target = if replacing && !query_empty {
        search.replace_ref.get_untracked()
    } else {
        search.query_ref.get_untracked()
    };
    if let Some(input) = target {
        let _ = input.focus();
        input.select();
    }
    select_nearest(state, false);
}

/// Closes the widget and returns focus to the text, the current match left
/// selected.
pub(in crate::editor) fn close(state: EditorState) {
    let search = state.search;
    if !search.open.get_untracked() {
        return;
    }
    search.open.set(false);
    search.replacing.set(false);
    search.current.set(None);
    if let Some(widget) = search.widget_ref.get_untracked() {
        widget.set_hidden(true);
    }
    if let Some(element) = state.textarea() {
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus();
        // Focusing scrolls to the caret; the match is already in view.
        element.set_scroll_top(top);
        element.set_scroll_left(left);
    }
}

/// Makes the first match at or after the selection current (as the query
/// or an option changes), and selects it when `reveal`.
pub(in crate::editor) fn select_nearest(state: EditorState, reveal: bool) {
    flush_input(state);
    let search = state.search;
    search.notice.set(None);
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, _) = textarea_selection(&element);
    let from = state
        .doc
        .text
        .with_untracked(|text| utf16_to_byte_index(text, start));
    let current = search.results.with_untracked(|results| {
        let index = results.matches.partition_point(|found| found.start < from);
        (!results.matches.is_empty()).then(|| index % results.matches.len())
    });
    search.current.set(current);
    if reveal && current.is_some() {
        show_current(state);
    }
}

/// Steps to the next match (`Enter`, `F3`) or the previous one
/// (`Shift+Enter`, `Shift+F3`), wrapping around.
pub(in crate::editor) fn step(state: EditorState, backward: bool) {
    flush_input(state);
    let search = state.search;
    search.notice.set(None);
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let next = state.doc.text.with_untracked(|text| {
        let (start, end) = (
            utf16_to_byte_index(text, start),
            utf16_to_byte_index(text, end),
        );
        search.results.with_untracked(|results| {
            let matches = &results.matches;
            if matches.is_empty() {
                return None;
            }
            // From the current match while it is still the selection, else
            // from wherever the caret moved since.
            let on_current = search
                .current
                .get_untracked()
                .and_then(|index| matches.get(index).map(|found| (index, found)))
                .filter(|(_, found)| found.start == start && found.end == end)
                .map(|(index, _)| index);
            let count = matches.len();
            Some(match (on_current, backward) {
                (Some(index), false) => (index + 1) % count,
                (Some(index), true) => (index + count - 1) % count,
                (None, false) => matches.partition_point(|found| found.start < end) % count,
                (None, true) => matches
                    .partition_point(|found| found.start < start)
                    .checked_sub(1)
                    .unwrap_or(count - 1),
            })
        })
    });
    if next.is_some() {
        search.current.set(next);
        show_current(state);
    }
}

/// Flips a match option (case, whole word, regular expression).
pub(in crate::editor) fn toggle_option(state: EditorState, flip: fn(&mut SearchOptions)) {
    state.search.options.update(flip);
    select_nearest(state, true);
}

/// Replaces the current match and steps to the next one.
pub(in crate::editor) fn replace_current(state: EditorState) {
    if state.config.read_only.get_untracked() {
        return;
    }
    flush_input(state);
    let search = state.search;
    let Some(matcher) = search.matcher() else {
        return;
    };
    let target = search.results.with_untracked(|results| {
        let index = search.current.get_untracked().unwrap_or(0);
        results.matches.get(index).copied()
    });
    let Some(found) = target else {
        return;
    };
    let before = state.doc.text.get_untracked();
    let replacement = search.replacement.get_untracked();
    let (updated, inserted) =
        replace::replace_one(&before, &matcher, found.start..found.end, &replacement);
    let selection = (utf16(&before, found.start), utf16(&before, found.end));
    let caret = utf16(&updated, inserted.end);
    state.commit(Edit::new(before, selection, updated, caret));
    // The next match after the replacement, wrapping around.
    let next = search.results.with_untracked(|results| {
        let matches = &results.matches;
        (!matches.is_empty())
            .then(|| matches.partition_point(|found| found.start < inserted.end) % matches.len())
    });
    search.current.set(next);
    if next.is_some() {
        show_current(state);
    }
}

/// Replaces every match as one edit: one undo step, one `on_change`.
pub(in crate::editor) fn replace_all(state: EditorState) {
    if state.config.read_only.get_untracked() {
        return;
    }
    flush_input(state);
    let search = state.search;
    let Some(matcher) = search.matcher() else {
        return;
    };
    let before = state.doc.text.get_untracked();
    let replacement = search.replacement.get_untracked();
    // Matches in read-only text are left as they are, and counted.
    let protected = state.protected_bytes(&before);
    let skip = |found: &std::ops::Range<usize>| {
        protected
            .iter()
            .any(|range| range.start < found.end && found.start < range.end)
    };
    let Some(replaced) = replace::replace_all_except(&before, &matcher, &replacement, &skip) else {
        if matcher.find_from(&before, 0).is_some() {
            search
                .notice
                .set(Some(super::super::protect::NOTHING_REPLACED.to_string()));
        }
        return;
    };
    let selection = state
        .textarea()
        .map_or((0, 0), |element| textarea_selection(&element));
    let caret = utf16(&replaced.text, replaced.first.end);
    if !state.commit(Edit::new(before, selection, replaced.text, caret)) {
        return;
    }
    let count = replaced.count;
    search.current.set(None);
    let skipped = if replaced.skipped > 0 {
        super::super::protect::skipped_matches(replaced.skipped)
    } else {
        String::new()
    };
    search.notice.set(Some(format!(
        "Replaced {count} {}{skipped}",
        if count == 1 {
            "occurrence"
        } else {
            "occurrences"
        }
    )));
}

/// The match count for the widget: "3 of 12", "No results", or the
/// pattern's error.
pub(in crate::editor) fn count_label(
    results: &SearchResults,
    current: Option<usize>,
    query_empty: bool,
) -> String {
    if results.error.is_some() {
        return "Invalid pattern".to_string();
    }
    if query_empty || results.matches.is_empty() {
        return "No results".to_string();
    }
    let total = if results.capped {
        format!("{MAX_MATCHES}+")
    } else {
        results.matches.len().to_string()
    };
    match current.filter(|index| *index < results.matches.len()) {
        Some(index) => format!("{} of {total}", index + 1),
        None => format!("? of {total}"),
    }
}

/// Selects the current match in the text and scrolls it into view, clear
/// of the widget.
fn show_current(state: EditorState) {
    let search = state.search;
    let Some(found) = search.current.get_untracked().and_then(|index| {
        search
            .results
            .with_untracked(|results| results.matches.get(index).copied())
    }) else {
        return;
    };
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = state
        .doc
        .text
        .with_untracked(|text| (utf16(text, found.start), utf16(text, found.end)));
    let _ = element.set_selection_range(start as u32, end as u32);
    let inset = search
        .widget_ref
        .get_untracked()
        .map_or(0.0, |widget| f64::from(widget.offset_height()));
    state
        .doc
        .text
        .with_untracked(|text| reveal_centered(&element, text, start, state.view.scroll_y, inset));
    state.forget_typing_aids();
    state.close_suggestions();
    state.note_caret(end);
}

fn selected_text(state: EditorState) -> Option<String> {
    let element = state.textarea()?;
    let (start, end) = textarea_selection(&element);
    state.doc.text.with_untracked(|text| {
        let (start, end) = (
            utf16_to_byte_index(text, start),
            utf16_to_byte_index(text, end),
        );
        text.get(start..end).map(str::to_string)
    })
}

/// UTF-16 offset (as the textarea counts) of byte `index`.
fn utf16(text: &str, index: usize) -> usize {
    text[..index].encode_utf16().count()
}
