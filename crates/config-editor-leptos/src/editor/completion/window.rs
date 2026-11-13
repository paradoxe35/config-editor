//! The suggestions rendered of a long list: those around how far it is
//! scrolled ([`row_window`]), and the highlighted one wherever it is, so the
//! textarea's `aria-activedescendant` always names an option in the page.

use std::ops::Range;

use leptos::prelude::*;

use super::super::state::EditorState;
use super::super::work_count::{Work, note};
use super::{COMPLETION_POPUP_MAX_HEIGHT, Completion, RowSize};
use crate::reactive::{most_rows, row_window};

/// One suggestion as rendered: its place in the list, of how many, and of
/// which list (see `CompletionState::listing`).
#[derive(Clone)]
pub(crate) struct ShownOption {
    pub(crate) listing: usize,
    pub(crate) index: usize,
    pub(crate) count: usize,
    pub(crate) option: Completion,
}

/// The most rows of a list with rows `size` high rendered around its scroll
/// position.
pub(crate) fn most_option_rows(size: RowSize) -> usize {
    most_rows(size.height(), COMPLETION_POPUP_MAX_HEIGHT)
}

/// The rows of a list of `count` to render when it is scrolled `top`
/// pixels. A `top` past the end (the list got shorter, and the browser has
/// yet to say how far it scrolls now) is taken as the end, so the rows in
/// view are among them.
pub(crate) fn option_rows(top: f64, count: usize, size: RowSize) -> Range<usize> {
    let row = size.height();
    let end = (count as f64 * row - COMPLETION_POPUP_MAX_HEIGHT).max(0.0);
    row_window(top.clamp(0.0, end), row, COMPLETION_POPUP_MAX_HEIGHT, count)
}

impl EditorState {
    /// The rows of the list rendered around its scroll position (tracked).
    pub(crate) fn shown_rows(&self) -> Range<usize> {
        let count = self.completion().suggestions.with(Vec::len);
        option_rows(
            self.completion().list_top.get(),
            count,
            self.completion().row_size.get(),
        )
    }

    /// The suggestions rendered around the list's scroll position
    /// (tracked).
    pub(crate) fn shown_options(&self) -> Vec<ShownOption> {
        let rows = self.shown_rows();
        self.shown_of(rows)
    }

    /// The highlighted suggestion when it is not among
    /// [`shown_options`](Self::shown_options) (tracked).
    pub(crate) fn pinned_option(&self) -> Option<ShownOption> {
        let rows = self.shown_rows();
        let active = self.completion().active.get();
        let count = self.completion().suggestions.with(Vec::len);
        if rows.contains(&active) || active >= count {
            return None;
        }
        self.shown_of(active..active + 1).pop()
    }

    fn shown_of(&self, rows: Range<usize>) -> Vec<ShownOption> {
        let listing = self.completion().listing.get();
        self.completion().suggestions.with(|options| {
            let count = options.len();
            rows.filter_map(|index| {
                note(Work::SuggestionRow);
                Some(ShownOption {
                    listing,
                    index,
                    count,
                    option: options.get(index)?.clone(),
                })
            })
            .collect()
        })
    }
}

#[cfg(all(test, feature = "yaml", feature = "schema-validation"))]
mod tests {
    use std::sync::Arc;

    use leptos::prelude::*;

    use super::super::super::diagnostics::build_schema;
    use super::super::super::languages::Language;
    use super::super::super::test_state::TestEditor;
    use super::super::super::work_count::{Work, counted};
    use super::super::RowSize;
    use super::most_option_rows;

    /// An editor whose schema offers `count` root keys, with its list open.
    fn open_with(count: usize) -> (Owner, super::EditorState) {
        open_described(count, true)
    }

    /// As `open_with`, its keys with a description or without.
    fn open_described(count: usize, described: bool) -> (Owner, super::EditorState) {
        let properties: serde_json::Map<String, serde_json::Value> = (1..=count)
            .map(|index| {
                let key = format!("k{index:04}");
                if described {
                    let description = format!("Key {index}");
                    (
                        key,
                        serde_json::json!({ "type": "string", "description": description }),
                    )
                } else {
                    (key, serde_json::json!({ "type": "string" }))
                }
            })
            .collect();
        let schema = serde_json::json!({ "type": "object", "properties": properties }).to_string();
        let mut editor = TestEditor::new(Language::YAML, "");
        editor.schema = Some(Arc::new(build_schema(&schema).expect("the schema")));
        let (owner, state) = editor.mount();
        state.completion().explicit.set(true);
        state.completion().requested.set(true);
        assert_eq!(
            state.completion().suggestions.with_untracked(Vec::len),
            count
        );
        (owner, state)
    }

    /// D5.perf-3: 1,200 keys in scope built 1,200 options on every open
    /// and filter. The rows built stay those of one window, however many
    /// there are, wherever the list is scrolled and the highlight is.
    #[test]
    fn the_rows_built_do_not_grow_with_the_suggestions() {
        let built = |count: usize, top: f64, active: usize| {
            let (_owner, state) = open_with(count);
            state.completion().list_top.set(top);
            state.completion().active.set(active);
            let (rows, built) = counted(Work::SuggestionRow, || {
                (state.shown_options(), state.pinned_option())
            });
            let pinned = rows.1.map(|row| row.index);
            let indexes: Vec<usize> = rows.0.iter().map(|row| row.index).collect();
            (built, indexes, pinned)
        };
        let (few, few_rows, _) = built(50, 0.0, 0);
        let (many, many_rows, _) = built(1200, 0.0, 0);
        assert_eq!(few, many, "rows built for 50 and for 1,200 suggestions");
        assert!(many <= most_option_rows(RowSize::Tall), "{many} rows built");
        assert_eq!(few_rows, many_rows);
        // Scrolled to the middle with the highlight at the end: the rows
        // there, and the highlighted one besides.
        let (far, rows, pinned) = built(1200, 600.0 * 44.0, 1199);
        assert_eq!(far, many + 1);
        assert!(rows.contains(&600), "{rows:?}");
        assert_eq!(pinned, Some(1199));
        // Scrolled past the end of a list that got shorter: its last rows.
        let (_, rows, _) = built(50, 600.0 * 44.0, 0);
        assert_eq!(rows.last(), Some(&49));
    }

    /// D12.a11y-1: each rendered option knows its place among all of them.
    #[test]
    fn each_shown_option_knows_its_place_and_the_count() {
        let (_owner, state) = open_with(1200);
        state.completion().list_top.set(300.0 * 44.0);
        for row in state.shown_options() {
            assert_eq!(row.count, 1200);
            assert_eq!(row.option.label, format!("k{:04}", row.index + 1));
        }
    }

    /// A list with no description is 28 px a row and shows the rows of its
    /// height; one description makes every row 44 px.
    #[test]
    fn rows_are_as_high_as_the_list_needs() {
        for (described, size) in [(false, RowSize::Compact), (true, RowSize::Tall)] {
            let (_owner, state) = open_described(1200, described);
            assert_eq!(state.completion().row_size.get_untracked(), size);
            let rows = state.shown_rows();
            assert!(rows.len() <= most_option_rows(size), "{rows:?}");
            assert!(rows.len() as f64 * size.height() >= 320.0, "{rows:?}");
            // Scrolled `size.height()` * 600: row 600 is the first in view.
            state.completion().list_top.set(600.0 * size.height());
            assert!(state.shown_rows().contains(&600), "{size:?}");
        }
        let (_owner, state) = open_described(1200, false);
        assert!(state.shown_rows().len() > most_option_rows(RowSize::Tall));
    }
}
