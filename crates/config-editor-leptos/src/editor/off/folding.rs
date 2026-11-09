//! Folding, compiled out (the `folding` feature is off): the hooks of
//! `folding/` with nothing behind them. Nothing folds, so the textarea
//! shows the document itself; the folding keys stay the browser's and the
//! `folding` setting does nothing.

use web_sys::KeyboardEvent;

use super::fold_ranges::FoldRange;
use super::state::EditorState;

impl EditorState {
    pub(super) fn with_closed_folds<R>(&self, f: impl FnOnce(&[FoldRange]) -> R) -> R {
        f(&[])
    }

    pub(super) fn with_closed_folds_untracked<R>(&self, f: impl FnOnce(&[FoldRange]) -> R) -> R {
        f(&[])
    }

    pub(super) fn folds_through(&self, _new: &str) -> Option<Vec<FoldRange>> {
        None
    }

    /// Shows the text, as with folding but with nothing folded.
    pub(super) fn settle_folds(&self, _moved: Option<Vec<FoldRange>>) -> bool {
        self.show_text()
    }

    pub(super) fn touches_fold(&self, _start: usize, _end: usize) -> bool {
        false
    }

    pub(super) fn unfold_touched(&self, _start: usize, _end: usize) {}

    pub(super) fn reveal_offsets(&self, _start: usize, _end: usize) {}

    pub(super) fn take_opened_folds(&self) -> Option<Vec<FoldRange>> {
        None
    }

    pub(super) fn restore_folds(&self, _folds: Vec<FoldRange>) {}

    pub(super) fn forget_folds(&self) {}

    pub(super) fn folded_fields(&self, _text: &str, _language: super::Language) -> Vec<String> {
        Vec::new()
    }

    pub(super) fn fold_fields(&self, _fields: &[String], _language: super::Language) -> bool {
        false
    }
}

pub(super) fn mount_fold_layers(_state: EditorState) {}

pub(super) fn shortcuts(_state: EditorState, _event: &KeyboardEvent) -> bool {
    false
}

pub(super) fn chord_key(_state: EditorState, _event: &KeyboardEvent) -> bool {
    false
}

pub(super) fn unfold_before_edit(_state: EditorState, _event: &KeyboardEvent) {}

pub(super) fn follow_settings(_state: EditorState) {}
