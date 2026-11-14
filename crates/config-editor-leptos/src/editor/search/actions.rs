//! What the find widget does to the editor: opening and closing it,
//! stepping through matches, and replacing them through the editor's normal
//! edit path (history, `on_change`, Edit-as).

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::input::flush_input;
use super::super::keyboard::utf16_to_byte_index;
use super::super::state::{EditorState, textarea_selection};
use super::matcher::MAX_MATCHES;
use super::replace;
use super::reveal::reveal_centered;
use super::{SearchOptions, SearchResults};
use crate::EditorStrings;
use crate::strings::{counted, fill};

/// What Replace All adds when it left `count` matches in protected text
/// alone.
fn skipped_matches(strings: &EditorStrings, count: usize) -> String {
    fill(&strings.replace_skipped, &[("count", &count)])
}

/// Longest selection taken as the query when the widget opens.
const SEED_MAX_CHARS: usize = 200;

/// Opens the widget (with the replace row for `Ctrl+H` in an editable
/// editor), taking a one-line selection as the query, and focuses it.
pub(in crate::editor) fn open(state: EditorState, replacing: bool) {
    flush_input(state);
    let search = state.search();
    if let Some(seed) = selected_text(state).filter(|seed| {
        !seed.is_empty() && !seed.contains('\n') && seed.chars().count() <= SEED_MAX_CHARS
    }) {
        search.query.set(seed);
    }
    let replacing = replacing && !state.config().read_only.get_untracked();
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
        let _ = crate::focus::in_view(&input);
        input.select();
    }
    select_nearest(state, false);
}

/// Closes the widget and returns focus to the text, the current match left
/// selected.
pub(in crate::editor) fn close(state: EditorState) {
    if !state.search().open.get_untracked() {
        return;
    }
    dismiss(state);
    if let Some(element) = state.textarea() {
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus_in_place();
        // Focusing scrolls to the caret; the match is already in view.
        element.set_scroll_top(top);
        element.set_scroll_left(left);
    }
}

/// Closes the widget where the focus is (the `search` setting switched off).
pub(in crate::editor) fn dismiss(state: EditorState) {
    let search = state.search();
    search.open.set(false);
    search.replacing.set(false);
    search.current.set(None);
    if let Some(widget) = search.widget_ref.get_untracked() {
        widget.set_hidden(true);
    }
}

/// Makes the first match at or after the selection current (as the query
/// or an option changes), and selects it when `reveal`.
pub(in crate::editor) fn select_nearest(state: EditorState, reveal: bool) {
    flush_input(state);
    let search = state.search();
    search.notice.set(None);
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, _) = textarea_selection(&element);
    let from = state
        .doc()
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

/// A query typed into an empty field that matches nothing: the count
/// already read "No results", so its live text does not change and a
/// screen reader says nothing; the editor's live region says it.
pub(in crate::editor) fn say_no_results_after_empty(state: EditorState, was_empty: bool) {
    let search = state.search();
    let nothing = search.query.with_untracked(|query| !query.is_empty())
        && search
            .results
            .with_untracked(|results| results.matches.is_empty() && results.error.is_none());
    if was_empty && nothing {
        let said = state.strings_untracked(|strings| strings.no_results.clone());
        state.a11y().announce(said);
    }
}

/// Steps to the next match (`Enter`, `F3`) or the previous one
/// (`Shift+Enter`, `Shift+F3`), wrapping around.
pub(in crate::editor) fn step(state: EditorState, backward: bool) {
    flush_input(state);
    let search = state.search();
    search.notice.set(None);
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let next = state.doc().text.with_untracked(|text| {
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
    state.search().options.update(flip);
    select_nearest(state, true);
}

/// Replaces the current match and steps to the next one.
pub(in crate::editor) fn replace_current(state: EditorState) {
    if state.config().read_only.get_untracked() {
        return;
    }
    flush_input(state);
    let search = state.search();
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
    let before = state.doc().text.get_untracked();
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
    if state.config().read_only.get_untracked() {
        return;
    }
    flush_input(state);
    let search = state.search();
    let Some(matcher) = search.matcher() else {
        return;
    };
    let before = state.doc().text.get_untracked();
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
            search.notice.set(Some(SearchNotice::NothingReplaced));
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
    search.notice.set(Some(SearchNotice::Replaced {
        count,
        skipped: replaced.skipped,
    }));
}

/// The match count for the widget: "3 of 12", "No results", the pattern's
/// error, or nothing for an empty query.
pub(in crate::editor) fn count_label(
    strings: &EditorStrings,
    results: &SearchResults,
    current: Option<usize>,
    query_empty: bool,
) -> String {
    if results.error.is_some() {
        return strings.invalid_pattern.clone();
    }
    if query_empty {
        return String::new();
    }
    if results.matches.is_empty() {
        return strings.no_results.clone();
    }
    let total = if results.capped {
        format!("{MAX_MATCHES}+")
    } else {
        results.matches.len().to_string()
    };
    let current = current
        .filter(|index| *index < results.matches.len())
        .map_or_else(|| "?".to_string(), |index| (index + 1).to_string());
    fill(
        &strings.match_position,
        &[("current", &current), ("total", &total)],
    )
}

/// What the last Replace All did, said in place of the match count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum SearchNotice {
    /// `count` matches replaced, `skipped` left in protected text.
    Replaced { count: usize, skipped: usize },
    /// Every match is in read-only text or a secret placeholder.
    NothingReplaced,
}

impl SearchNotice {
    pub(in crate::editor) fn text(&self, strings: &EditorStrings) -> String {
        match self {
            SearchNotice::Replaced { count, skipped } => {
                let mut text = counted(&strings.replaced_one, &strings.replaced_other, *count);
                if *skipped > 0 {
                    text.push_str(&skipped_matches(strings, *skipped));
                }
                text
            }
            SearchNotice::NothingReplaced => strings.nothing_replaced.clone(),
        }
    }
}

/// Selects the current match in the text and scrolls it into view, clear
/// of the widget.
fn show_current(state: EditorState) {
    let search = state.search();
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
        .doc()
        .text
        .with_untracked(|text| (utf16(text, found.start), utf16(text, found.end)));
    let _ = element.set_selection_range(start as u32, end as u32);
    let inset = search
        .widget_ref
        .get_untracked()
        .map_or(0.0, |widget| f64::from(widget.offset_height()));
    state.doc().text.with_untracked(|text| {
        reveal_centered(&element, text, start, state.view().scroll_y, inset)
    });
    state.forget_typing_aids();
    state.close_suggestions();
    state.note_caret(end);
}

fn selected_text(state: EditorState) -> Option<String> {
    let element = state.textarea()?;
    let (start, end) = textarea_selection(&element);
    state.doc().text.with_untracked(|text| {
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
