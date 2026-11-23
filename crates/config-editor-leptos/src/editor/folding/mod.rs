//! Folding by structure. View only: the document, `on_change`, undo and the
//! clipboard always see the whole text. The `folding` feature: without it,
//! `off/folding.rs` stands in with the same hooks, and nothing folds.
//!
//! What it builds on stays in every build: the fold ranges' building blocks
//! languages combine in
//! [`Text::fold_ranges`](super::language::Text::fold_ranges)
//! (`fold_ranges`), and the textarea showing the document with the folded
//! lines cut out (`projection`, `source_area`).
//!
//! - `follow`: folds move with edits; an edit touching one opens it.
//! - `actions`: folding commands, and the folds through edits.
//! - `keys`: the shortcuts; `components`: the chevrons and `…` chips.
//! - `sticky`: sticky scroll, the header lines of the blocks scrolled past.
//!
//! How edits meet folds, as in VS Code: typing at the end of a folded line,
//! Backspace at the start of the line after a fold, Delete at the end of a
//! folded line, and any edit whose selection reaches into a fold open it
//! before the edit applies. An edit to the folded line before its end
//! leaves the fold closed. A selection across a fold copies and cuts the
//! hidden lines too.

mod actions;
mod components;
mod follow;
mod handle;
mod keys;
mod sticky;
mod switch;
pub(crate) mod texts;

#[cfg(test)]
mod tests;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod switch_scale_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod switch_tests;

pub(super) use components::mount_fold_layers;
pub(super) use keys::{chord_key, shortcuts, unfold_before_edit};

use crate::editor::text::SharedText;
use leptos::prelude::*;

use super::fold_ranges::FoldRange;
use super::live_settings::on_switch_off;
use super::state::EditorState;

/// The editor's folding state, one `Copy` group of handles.
#[derive(Clone, Copy)]
pub(crate) struct FoldState {
    /// Closed folds, sorted.
    pub(crate) folded: RwSignal<Vec<FoldRange>>,
    /// What can fold in the document. Lazy: only read with folding on.
    pub(crate) fold_ranges: Memo<Vec<FoldRange>>,
    /// The document's blocks, as fold ranges, while folding or sticky
    /// scroll reads them: read once per edit for both.
    pub(crate) blocks: Memo<Vec<FoldRange>>,
    /// The closed folds before an editing key opened some for its edit:
    /// put back if the edit is then refused (a read-only value).
    pub(crate) before_edit: StoredValue<Option<Vec<FoldRange>>>,
    /// The sticky lines, once mounted: what they cover is a jump's inset.
    sticky: StoredValue<Option<sticky::Sticky>>,
}

impl FoldState {
    /// Folding for `text` (in `edit_language`), nothing folded. With
    /// `enabled` off (the `folding` setting) nothing can fold; the blocks
    /// are read while it or `sticky` (the `sticky_scroll` setting) is on.
    pub(crate) fn new(
        text: SharedText,
        edit_language: RwSignal<super::languages::Language>,
        enabled: Memo<bool>,
        sticky: Memo<bool>,
        readings: super::readings::Readings,
    ) -> Self {
        let blocks = readings.memo(text, move || {
            if !enabled.get() && !sticky.get() {
                return Vec::new();
            }
            super::work_count::note(super::work_count::Work::FoldRanges);
            text.with(|text| edit_language.get().behavior().fold_ranges(text))
        });
        let fold_ranges = Memo::new(move |_| {
            if enabled.get() {
                blocks.get()
            } else {
                Vec::new()
            }
        });
        Self {
            folded: RwSignal::new(Vec::new()),
            fold_ranges,
            blocks,
            before_edit: StoredValue::new(None),
            sticky: StoredValue::new(None),
        }
    }
}

impl EditorState {
    /// Calls `f` with the closed folds, sorted (tracked).
    pub(super) fn with_closed_folds<R>(&self, f: impl FnOnce(&[FoldRange]) -> R) -> R {
        self.fold().folded.with(|folds| f(folds))
    }

    /// Calls `f` with the closed folds, sorted (untracked).
    pub(super) fn with_closed_folds_untracked<R>(&self, f: impl FnOnce(&[FoldRange]) -> R) -> R {
        self.fold().folded.with_untracked(|folds| f(folds))
    }

    /// How far down the sticky lines cover the text area now, in pixels
    /// (0 with none drawn).
    pub(super) fn sticky_inset(&self) -> f64 {
        self.fold()
            .sticky
            .get_value()
            .map_or(0.0, sticky::Sticky::covered)
    }

    /// What can fold in the text on screen (untracked).
    pub(super) fn foldable_untracked(&self) -> Vec<FoldRange> {
        self.fold().fold_ranges.get_untracked()
    }
}

/// Switching the `folding` setting off opens every fold.
pub(super) fn follow_settings(state: EditorState) {
    on_switch_off(state.config().settings.folding, move || state.unfold_all());
}
