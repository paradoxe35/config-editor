//! Closed folds through edits: they move with the lines around them, an
//! edit that touches one opens it, and one whose structure is gone goes.

use super::super::fold_ranges::FoldRange;
use super::super::projection::{common_prefix, common_suffix};

/// `folds` (closed in `old`) moved to `new`. An edit at or inside a fold's
/// hidden text, from its first line's end through its last line's end,
/// opens it (typing at the end of a folded line, Backspace at the start of
/// the line after it, a replace across it); folds before the edit stay,
/// folds after it move by the lines it added or removed.
pub(crate) fn follow_edit(folds: &[FoldRange], old: &str, new: &str) -> Vec<FoldRange> {
    if folds.is_empty() || old == new {
        return folds.to_vec();
    }
    let prefix = common_prefix(old, new);
    let suffix = common_suffix(&old[prefix..], &new[prefix..]);
    let (edit_start, edit_end) = (prefix, old.len() - suffix);
    let removed = old[edit_start..edit_end].matches('\n').count();
    let added = new[prefix..new.len() - suffix].matches('\n').count();
    // Byte offset of each line's end in `old`.
    let ends: Vec<usize> = old
        .match_indices('\n')
        .map(|(at, _)| at)
        .chain(std::iter::once(old.len()))
        .collect();
    let line_end = |line: usize| ends[line.min(ends.len() - 1)];
    folds
        .iter()
        .filter_map(|fold| {
            let (hidden_start, hidden_end) = (line_end(fold.start), line_end(fold.end));
            if edit_start <= hidden_end && edit_end >= hidden_start {
                None
            } else if hidden_end < edit_start {
                Some(*fold)
            } else {
                Some(FoldRange::new(
                    (fold.start + added).checked_sub(removed)?,
                    (fold.end + added).checked_sub(removed)?,
                ))
            }
        })
        .collect()
}

/// `folds` that still start a fold range of the text (`ranges`, sorted by
/// start), each taking that range's end: a fold whose structure is gone
/// goes.
pub(crate) fn keep_valid(folds: &[FoldRange], ranges: &[FoldRange]) -> Vec<FoldRange> {
    let mut kept: Vec<FoldRange> = folds
        .iter()
        .filter_map(|fold| {
            let index = ranges
                .binary_search_by_key(&fold.start, |range| range.start)
                .ok()?;
            Some(ranges[index])
        })
        .collect();
    kept.sort_unstable();
    kept.dedup();
    kept
}
