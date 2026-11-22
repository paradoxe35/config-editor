//! The list Find uses shows: a name's uses grouped by document (this one
//! first, then the other open ones in tab order, then the host's files as
//! they come), one row per use, and the row the keyboard is on.

use leptos::prelude::*;

use crate::FieldPath;
use crate::editor::text::{TextRange, TextSize};

/// Where a group's uses are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UsePlace {
    /// The editor's own document.
    ThisDocument,
    /// Another open document of the documents view, by id.
    Open(String),
    /// A file the host listed, by its source.
    Host(String),
}

/// One use in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseRow {
    pub(crate) path: FieldPath,
    /// Its one-based line, where known.
    pub(crate) line: Option<usize>,
    /// What the row shows: the line's text, else the path.
    pub(crate) text: String,
    /// Where it is written, in the editor's own text.
    pub(crate) range: Option<TextRange>,
}

/// The uses in one document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseGroup {
    /// The group's title: "this document", a file name, a host's source.
    pub(crate) label: String,
    pub(crate) place: UsePlace,
    pub(crate) rows: Vec<UseRow>,
}

/// The list open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsesList {
    /// The name whose uses it lists.
    pub(crate) name: String,
    /// Where it points: the name's start in the text.
    pub(crate) anchor: crate::editor::text::TextSize,
    pub(crate) groups: Vec<UseGroup>,
    /// The host was asked for its files' uses and has not answered.
    pub(crate) searching: bool,
    /// The host searches files beyond the open ones (`on_find_uses`).
    pub(crate) host_searches: bool,
    /// Which request it answers: a host's answer to another is dropped.
    pub(crate) request: u64,
    /// The caret and the text's version when it opened: either moving on
    /// closes it.
    pub(crate) opened: (TextSize, u64),
}

impl UsesList {
    /// How many uses it lists.
    pub(crate) fn count(&self) -> usize {
        self.groups.iter().map(|group| group.rows.len()).sum()
    }

    /// The group and row of use `index` (counted over every group).
    pub(crate) fn row(&self, index: usize) -> Option<(&UseGroup, &UseRow)> {
        let mut left = index;
        for group in &self.groups {
            if left < group.rows.len() {
                return Some((group, &group.rows[left]));
            }
            left -= group.rows.len();
        }
        None
    }

    /// The index (counted over every group) of each group's first row.
    pub(crate) fn firsts(&self) -> Vec<usize> {
        let mut first = 0;
        self.groups
            .iter()
            .map(|group| {
                let at = first;
                first += group.rows.len();
                at
            })
            .collect()
    }
}

/// The Find uses part of the editor state.
#[derive(Clone, Copy)]
pub(crate) struct UsesState {
    /// The list open, if any.
    pub(crate) list: RwSignal<Option<UsesList>>,
    /// The row the keyboard (or the pointer) is on.
    pub(crate) active: RwSignal<usize>,
    /// The last request made: answers to older ones are dropped.
    pub(crate) request: StoredValue<u64>,
}

impl UsesState {
    pub(crate) fn new() -> Self {
        Self {
            list: RwSignal::new(None),
            active: RwSignal::new(0),
            request: StoredValue::new(0),
        }
    }

    /// Whether the list is open.
    pub(crate) fn is_open(&self) -> bool {
        self.list.with_untracked(Option::is_some)
    }
}
