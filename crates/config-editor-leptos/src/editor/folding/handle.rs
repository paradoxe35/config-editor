//! The folding methods of [`EditorHandle`]: they exist with the `folding`
//! feature only.

use leptos::prelude::*;

use super::super::handle::EditorHandle;
use super::super::paths::FieldPath;

/// Folding (the `folding` feature).
impl EditorHandle {
    /// Folds the block the field at `path` opens (its key's line), as its
    /// gutter chevron does. Returns whether there was one to fold; `false`
    /// with the `folding` setting off.
    pub fn fold(&self, path: impl Into<FieldPath>) -> bool {
        let (Some(state), Some(range)) = (self.editor(), self.range_of(path)) else {
            return false;
        };
        state.fold_starting_at(range.start.line - 1)
    }

    /// Opens the folds around the field at `path`, so it shows. Returns
    /// whether one was closed.
    pub fn unfold(&self, path: impl Into<FieldPath>) -> bool {
        let (Some(state), Some(range)) = (self.editor(), self.range_of(path)) else {
            return false;
        };
        state.unfold_line(range.start.line - 1)
    }

    /// Folds every block (`Ctrl+K Ctrl+0`).
    pub fn fold_all(&self) {
        if let Some(state) = self.editor() {
            state.fold_all();
        }
    }

    /// Opens every fold (`Ctrl+K Ctrl+J`).
    pub fn unfold_all(&self) {
        if let Some(state) = self.editor() {
            state.unfold_all();
        }
    }

    /// The folded lines, as one-based `(first, last)` pairs: the line that
    /// stays visible and the last hidden one. Reactive.
    pub fn folded_lines(&self) -> Vec<(usize, usize)> {
        self.editor_tracked()
            .and_then(|state| state.fold().folded.try_get())
            .unwrap_or_default()
            .iter()
            .map(|fold| (fold.start + 1, fold.end + 1))
            .collect()
    }
}
