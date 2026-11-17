//! Find and replace, as in VS Code: `Ctrl+F` opens the find widget,
//! `Ctrl+H` the replace row too; and go to line (`Ctrl+G`). The `search`
//! feature: without it, `off/search.rs` stands in with the same hooks.
//!
//! - `matcher`, `replace`: the pure part, finding and replacing text.
//! - `actions`: what the widget's keys and buttons do to the editor.
//! - `go_to_line`: the go-to-line field.
//! - `keys`: the shortcuts; `components`: the widgets and match boxes.
//!
//! Matches are only searched while the widget is open, and only those in
//! view are drawn (see `components::SearchHighlights`).
//!
//! The rest of the editor sees only the hooks re-exported here.
//!
//! Matches are typed byte ranges (`TextRange`) from where the regex engine
//! and the standard library's search give them (`matcher`) on.

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

mod actions;
mod components;
mod go_to_line;
mod keys;
mod matcher;
mod replace;
mod reveal;
mod template;

pub(super) use components::{FindWidget, GoToLineWidget, SearchHighlights};
pub(super) use go_to_line::GoToLine;
pub(super) use keys::shortcuts;

pub(super) use actions::replace_all;
use actions::{
    SearchNotice, close, count_label, dismiss, replace_current, say_query_results, select_nearest,
    step, toggle_option,
};
use matcher::{SearchMatch, SearchOptions};
// The random editing tests make Replace through the widget's own action
// (in the builds with every language).
#[cfg(test)]
#[allow(unused_imports, reason = "used by the random editing tests")]
pub(super) use actions::replace_current as replace_one_match;

use leptos::prelude::*;

use super::edit::Change;
use super::live_settings::on_switch_off;
use super::state::EditorState;
use crate::editor::text::TextRange;
use matcher::{Matcher, find_matches};

/// Switching the `search` or `go_to_line` setting off closes its widget.
pub(super) fn follow_settings(state: EditorState) {
    let settings = state.config().settings;
    on_switch_off(settings.search, move || dismiss(state));
    on_switch_off(settings.go_to_line, move || {
        go_to_line::cancel_on_blur(state)
    });
}

/// Escape in the text closes the find widget: `true` when it was open.
pub(super) fn close_on_escape(state: EditorState) -> bool {
    let open = state.search().open.get_untracked();
    if open {
        close(state);
    }
    open
}

/// Whether the find widget or the go-to-line field is open.
pub(super) fn is_open(state: EditorState) -> bool {
    state.search().open.get_untracked() || state.go_to_line().open.get_untracked()
}

/// What the open widget found in the document.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SearchResults {
    pub(super) matches: Vec<SearchMatch>,
    /// More matches than [`matcher::MAX_MATCHES`] exist.
    pub(super) capped: bool,
    /// The regular expression does not compile.
    pub(super) error: Option<String>,
}

/// The find widget's state.
#[derive(Clone, Copy)]
pub(super) struct SearchState {
    pub(super) open: RwSignal<bool>,
    /// The replace row is shown (never in a read-only editor).
    pub(super) replacing: RwSignal<bool>,
    pub(super) query: RwSignal<String>,
    pub(super) replacement: RwSignal<String>,
    pub(super) options: RwSignal<SearchOptions>,
    /// The match stepped to, followed through every edit (see
    /// [`EditorState::follow_current_match`]).
    stepped: RwSignal<Option<TextRange>>,
    /// Index of the match stepped to, shown distinctly: `None` once an edit
    /// changed its text.
    pub(super) current: Memo<Option<usize>>,
    /// What the last replace all did, announced in place of the count.
    pub(super) notice: RwSignal<Option<SearchNotice>>,
    pub(super) results: Memo<SearchResults>,
    pub(super) widget_ref: NodeRef<leptos::html::Div>,
    pub(super) query_ref: NodeRef<leptos::html::Input>,
    pub(super) replace_row_ref: NodeRef<leptos::html::Div>,
    pub(super) replace_ref: NodeRef<leptos::html::Input>,
}

impl SearchState {
    pub(super) fn new(source: RwSignal<String>) -> Self {
        let open = RwSignal::new(false);
        let query = RwSignal::new(String::new());
        let options = RwSignal::new(SearchOptions::default());
        // Closed, it reads nothing: typing pays for search only while the
        // widget is open.
        let results = Memo::new(move |_| {
            if !open.get() {
                return SearchResults::default();
            }
            let options = options.get();
            query.with(|query| match Matcher::new(query, options) {
                Err(error) => SearchResults {
                    error: Some(error),
                    ..SearchResults::default()
                },
                Ok(None) => SearchResults::default(),
                Ok(Some(matcher)) => source.with(|text| {
                    let (matches, capped) = find_matches(text, &matcher);
                    SearchResults {
                        matches,
                        capped,
                        error: None,
                    }
                }),
            })
        });
        let stepped = RwSignal::new(None::<TextRange>);
        let current = Memo::new(move |_| {
            let stepped = stepped.get()?;
            results.with(|results| {
                let index = results
                    .matches
                    .binary_search_by_key(&stepped.start(), |found| found.range.start())
                    .ok()?;
                (results.matches[index].range == stepped).then_some(index)
            })
        });
        Self {
            open,
            replacing: RwSignal::new(false),
            query,
            replacement: RwSignal::new(String::new()),
            options,
            stepped,
            current,
            notice: RwSignal::new(None),
            results,
            widget_ref: NodeRef::new(),
            query_ref: NodeRef::new(),
            replace_row_ref: NodeRef::new(),
            replace_ref: NodeRef::new(),
        }
    }

    /// Steps to the match at `index` of the results (none with `None`).
    pub(super) fn set_current(&self, index: Option<usize>) {
        let found = index.and_then(|index| {
            self.results
                .with_untracked(|results| results.matches.get(index).map(|found| found.range))
        });
        self.stepped.set(found);
    }

    /// The compiled query, when it is valid and not empty.
    fn matcher(&self) -> Option<Matcher> {
        let options = self.options.get_untracked();
        self.query
            .with_untracked(|query| Matcher::new(query, options).ok().flatten())
    }
}

impl EditorState {
    /// Maps the match stepped to through `change` (an edit's
    /// `Edit::mapping`), as the edit pipeline writes it: it stays on the
    /// same text, and is no longer current once the edit changed that text.
    pub(super) fn follow_current_match(&self, change: &Change) {
        let stepped = self.search().stepped;
        if let Some(range) = stepped.get_untracked() {
            stepped.set(change.map_range(&range));
        }
    }
}

#[cfg(test)]
mod tests;
