//! Find and replace, as in VS Code: `Ctrl+F` opens the find widget,
//! `Ctrl+H` the replace row too.
//!
//! - `matcher`, `replace`: the pure part, finding and replacing text.
//! - `actions`: what the widget's keys and buttons do to the editor.
//!
//! Matches are only searched while the widget is open, and only those in
//! view are drawn (see `components::SearchHighlights`).

mod actions;
mod matcher;
mod replace;

pub(super) use actions::{
    SearchNotice, close, count_label, dismiss, open, replace_all, replace_current, select_nearest,
    step, toggle_option,
};
pub(super) use matcher::{SearchMatch, SearchOptions};

use leptos::prelude::*;

use matcher::{Matcher, find_matches};

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
    /// Index of the match stepped to, shown distinctly.
    pub(super) current: RwSignal<Option<usize>>,
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
        Self {
            open,
            replacing: RwSignal::new(false),
            query,
            replacement: RwSignal::new(String::new()),
            options,
            current: RwSignal::new(None),
            notice: RwSignal::new(None),
            results,
            widget_ref: NodeRef::new(),
            query_ref: NodeRef::new(),
            replace_row_ref: NodeRef::new(),
            replace_ref: NodeRef::new(),
        }
    }

    /// The compiled query, when it is valid and not empty.
    fn matcher(&self) -> Option<Matcher> {
        let options = self.options.get_untracked();
        self.query
            .with_untracked(|query| Matcher::new(query, options).ok().flatten())
    }
}

#[cfg(test)]
mod tests;
