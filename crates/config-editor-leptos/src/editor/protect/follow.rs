//! The host's read-only ranges followed through an edit: before the text
//! it replaced a range stays, after it the range shifts; inside it, the
//! range's lines are looked for, whole, among the lines the edit wrote (a
//! line command moved or copied them, a reorder pasted them), and the range
//! goes where they are, at the same columns: the copy nearest where they
//! were. A range whose lines the edit changed keeps what is left of it.

use std::ops::Range;

use super::super::text_change::edit_range;
use super::locate::LineIndex;

/// `ranges` (UTF-16, in `before`) after the edit making `after`, its caret
/// at `caret`.
pub(super) fn follow_ranges(
    ranges: &[Range<usize>],
    before: &str,
    after: &str,
    caret: usize,
) -> Vec<Range<usize>> {
    let region = {
        let (start, end) = edit_range(before, after, caret);
        start..end
    };
    let inserted =
        (after.encode_utf16().count() + region.len()).saturating_sub(before.encode_utf16().count());
    // The lines of both texts, read only when a range is in the region.
    let mut lines: Option<(LineIndex<'_>, LineIndex<'_>)> = None;
    ranges
        .iter()
        .map(|range| {
            let shifted = shift(range, &region, inserted);
            if range.end <= region.start || range.start >= region.end || range.is_empty() {
                return shifted;
            }
            let (old, new) =
                lines.get_or_insert_with(|| (LineIndex::new(before), LineIndex::new(after)));
            let written = region.start..region.start + inserted;
            moved_with_lines(range, old, new, &written).unwrap_or(shifted)
        })
        .collect()
}

/// A range after an edit replaced `region` with `inserted` units: before
/// it stays, after it shifts, across it keeps what is left.
fn shift(range: &Range<usize>, region: &Range<usize>, inserted: usize) -> Range<usize> {
    let map = |at: usize| {
        if at <= region.start {
            at
        } else if at >= region.end {
            at + inserted - region.len()
        } else {
            region.start + inserted
        }
    };
    map(range.start)..map(range.end).max(map(range.start))
}

/// Where `range`'s lines (of `old`) are among the lines of `new` holding
/// `written` (what the edit wrote), whole and in order: the place nearest
/// the one they had. `None` when no such lines are there.
fn moved_with_lines(
    range: &Range<usize>,
    old: &LineIndex<'_>,
    new: &LineIndex<'_>,
    written: &Range<usize>,
) -> Option<Range<usize>> {
    let first = old.line_of(range.start);
    let last = old.line_of(range.end - 1).max(first);
    let block: Vec<&str> = (first..=last).map(|line| old.line_text(line)).collect();
    let from = new.line_of(written.start);
    let to = new.line_of(written.end);
    let holds = |at: usize| {
        at + block.len() <= new.line_count()
            && block
                .iter()
                .enumerate()
                .all(|(index, text)| new.line_text(at + index) == *text)
    };
    let found = (from.saturating_sub(block.len() - 1)..=to)
        .filter(|at| holds(*at))
        .min_by_key(|at| at.abs_diff(first))?;
    let start = old.line_bounds(first).start;
    let at = new.line_bounds(found).start;
    Some(at + (range.start - start)..at + (range.end - start))
}
