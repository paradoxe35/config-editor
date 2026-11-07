//! Folding's built-in texts, in one place. Lines are one-based.

/// The chevron of an open range starting on `line`.
pub(crate) fn fold_label(line: usize) -> String {
    format!("Fold line {line}")
}

/// The chevron or `…` chip of a closed fold over `first..=last`.
pub(crate) fn unfold_label(first: usize, last: usize) -> String {
    format!("Unfold lines {first}–{last}")
}

/// The tooltip of a closed fold's line number when it hides problems or
/// changes.
pub(crate) const HIDDEN_CHANGES: &str = "Folded lines have changes";

/// What the `…` chip shows.
pub(crate) const CHIP: &str = "…";
