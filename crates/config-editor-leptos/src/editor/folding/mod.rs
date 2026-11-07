//! Folding by structure. View only: the document, `on_change`, undo and the
//! clipboard always see the whole text.
//!
//! - `ranges`: what can fold, the building blocks languages combine in
//!   [`ConfigLanguage::fold_ranges`](super::language::ConfigLanguage::fold_ranges).
//! - `projection`: the textarea holds the document with the folded lines cut
//!   out; [`Projection`] maps offsets and rows between the two.
//! - `area`: [`SourceArea`], the textarea through that mapping. Moving the
//!   selection into a fold opens it.
//! - `follow`: folds move with edits; an edit touching one opens it.
//! - `actions`: folding commands and writing the document.
//!
//! How edits meet folds, as in VS Code: typing at the end of a folded line,
//! Backspace at the start of the line after a fold, Delete at the end of a
//! folded line, and any edit whose selection reaches into a fold open it
//! before the edit applies. An edit to the folded line before its end
//! leaves the fold closed. A selection across a fold copies and cuts the
//! hidden lines too.

mod actions;
mod area;
mod follow;
mod projection;
mod ranges;
pub(crate) mod texts;

#[cfg(test)]
mod tests;

pub(crate) use area::SourceArea;

use std::sync::Arc;

use leptos::prelude::*;

/// The editor's folding state, one `Copy` group of handles.
#[derive(Clone, Copy)]
pub(crate) struct FoldState {
    /// The document lines (0-based) on the textarea rows in view.
    pub(crate) visible_lines: Memo<Vec<usize>>,
    /// Closed folds, sorted.
    pub(crate) folded: RwSignal<Vec<FoldRange>>,
    /// The textarea's text: the document with the closed folds cut out.
    pub(crate) projection: RwSignal<Arc<Projection>>,
    /// What can fold in the document. Lazy: only read with folding on.
    pub(crate) fold_ranges: Memo<Vec<FoldRange>>,
    /// `Ctrl+K` was pressed: the next key may finish a folding chord.
    pub(crate) fold_chord: StoredValue<bool>,
    /// The closed folds before an editing key opened some for its edit:
    /// put back if the edit is then refused (a read-only value).
    pub(crate) before_edit: StoredValue<Option<Vec<FoldRange>>>,
}

impl FoldState {
    /// Folding for `text` (in `edit_language`), nothing folded: the
    /// textarea shows `projection`, and `visible_rows` of it are rendered.
    /// With `enabled` off (`enable_folding=false`) nothing can fold.
    pub(crate) fn new(
        projection: RwSignal<Arc<Projection>>,
        visible_rows: Memo<std::ops::Range<usize>>,
        text: RwSignal<String>,
        edit_language: RwSignal<super::languages::Language>,
        enabled: bool,
    ) -> Self {
        let visible_lines = Memo::new(move |_| {
            let rows = visible_rows.get();
            projection.with(|projection| rows.map(|row| projection.line_of_row(row)).collect())
        });
        let fold_ranges = Memo::new(move |_| {
            if !enabled {
                return Vec::new();
            }
            text.with(|text| edit_language.get().behavior().fold_ranges(text))
        });
        Self {
            visible_lines,
            folded: RwSignal::new(Vec::new()),
            projection,
            fold_ranges,
            fold_chord: StoredValue::new(false),
            before_edit: StoredValue::new(None),
        }
    }
}
pub(crate) use projection::Projection;
#[cfg(feature = "toml")]
pub(crate) use ranges::scan_brackets;
pub use ranges::{
    FoldRange, IndentLine, bracket_fold_ranges, comment_fold_ranges, default_fold_ranges,
    indent_fold_ranges, merge_fold_ranges, string_body_fold_ranges,
};
