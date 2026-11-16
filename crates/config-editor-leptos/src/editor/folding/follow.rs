//! Closed folds through edits: they move with the lines around them, an
//! edit that touches one opens it, and one whose structure is gone goes.

use super::super::edit::{Assoc, Change};
use super::super::fold_ranges::FoldRange;

/// `folds` (closed in `old`) moved through `change`, which makes `new` of
/// `old`. A replacement at or inside a fold's hidden text, from its first
/// line's end through its last line's end, opens it (typing at the end of
/// a folded line, Backspace at the start of the line after it, a replace
/// across it); a fold the change moves whole goes with it; the others keep
/// their lines, wherever the change puts them.
pub(crate) fn follow_edit(
    folds: &[FoldRange],
    old: &str,
    change: &Change,
    new: &str,
) -> Vec<FoldRange> {
    if folds.is_empty() {
        return Vec::new();
    }
    let old_ends = line_ends(old);
    let new_ends = line_ends(new);
    let line_end = |line: usize| old_ends[line.min(old_ends.len() - 1)];
    // The line of byte `at` of `new`: the number of line ends before it.
    let line_of = |at: usize| new_ends.partition_point(|end| *end < at);
    folds
        .iter()
        .filter_map(|fold| {
            let (hidden_start, hidden_end) = (line_end(fold.start), line_end(fold.end));
            let moved_whole = change
                .moved()
                .iter()
                .any(|moved| moved.from <= hidden_start && hidden_end <= moved.from + moved.len);
            let touched = change
                .edits()
                .any(|(range, _)| range.start <= hidden_end && range.end >= hidden_start);
            if touched && !moved_whole {
                return None;
            }
            let start = change.map_pos(hidden_start, Assoc::Before).pos;
            let end = change.map_pos(hidden_end, Assoc::Before).pos;
            Some(FoldRange::new(line_of(start), line_of(end)))
        })
        .collect()
}

/// The byte offset of each line's end in `text`, the text's end last.
fn line_ends(text: &str) -> Vec<usize> {
    text.match_indices('\n')
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
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
