//! The host's read-only ranges followed through an edit: before the text
//! it replaced a range stays, after it the range shifts; text inserted at
//! a range's edge goes outside it (before it at its start, after it at its
//! end), so a line added above a locked one is free; inside it, the
//! range's lines are looked for, whole, among the lines the edit wrote (a
//! line command moved or copied them, a reorder pasted them), and the range
//! goes where they are, at the same columns: the copy nearest where they
//! were. A range whose lines the edit changed keeps what is left of it.

use super::super::edit::{Assoc, Change};
use super::super::text::{TextRange, TextSize};
use super::super::text_change::edited_bytes;
use super::locate::LineIndex;

/// `ranges` (of `before`) after the edit making `after`: mapped through
/// `change`, what the edit recorded it did (text inserted at a range's
/// start goes before it, at its end after it; a range's lines a line
/// command moved go with them); for an edit with no record, read off the
/// two texts and the caret it left (see the module).
pub(super) fn follow_ranges(
    ranges: &[TextRange],
    before: &str,
    after: &str,
    caret: TextSize,
    change: Option<&Change>,
) -> Vec<TextRange> {
    if let Some(change) = change {
        return ranges
            .iter()
            .map(|range| {
                let start = change.map_pos(range.start(), Assoc::After).pos;
                let end = change.map_pos(range.end(), Assoc::Before).pos;
                TextRange::new(start, end.max(start))
            })
            .collect();
    }
    let region = edited_bytes(before, after, caret);
    let inserted = (TextSize::of(after) + region.len())
        .checked_sub(TextSize::of(before))
        .unwrap_or_default();
    // The lines of both texts, read only when a range is in the region.
    let mut lines: Option<(LineIndex<'_>, LineIndex<'_>)> = None;
    ranges
        .iter()
        .map(|range| {
            let shifted = shift(range, &region, inserted);
            // Only lines the edit wrote can hold a range's lines again: a
            // deletion writes none (the line after it is not the range's).
            if range.end() <= region.start()
                || range.start() >= region.end()
                || range.is_empty()
                || inserted == TextSize::default()
            {
                return shifted;
            }
            let (old, new) =
                lines.get_or_insert_with(|| (LineIndex::new(before), LineIndex::new(after)));
            let written = TextRange::at(region.start(), inserted);
            moved_with_lines(range, old, new, &written).unwrap_or(shifted)
        })
        .collect()
}

/// A range after an edit replaced `region` with `inserted` bytes: before
/// it stays, after it shifts, across it keeps what is left. Text inserted
/// at one of its edges stays outside it: at its start the range moves past
/// it, at its end the range stops before it.
fn shift(range: &TextRange, region: &TextRange, inserted: TextSize) -> TextRange {
    let map = |at: TextSize| {
        if at <= region.start() {
            at
        } else if at >= region.end() {
            at + inserted - region.len()
        } else {
            region.start() + inserted
        }
    };
    let start = if region.is_empty() && range.start() == region.start() {
        range.start() + inserted
    } else {
        map(range.start())
    };
    TextRange::new(start, map(range.end()).max(start))
}

/// Where `range`'s lines (of `old`) are among the lines of `new` holding
/// `written` (what the edit wrote), whole and in order: the place nearest
/// the one they had. `None` when no such lines are there.
fn moved_with_lines(
    range: &TextRange,
    old: &LineIndex<'_>,
    new: &LineIndex<'_>,
    written: &TextRange,
) -> Option<TextRange> {
    let first = old.line_of(range.start());
    let last = old.line_of(range.end() - TextSize::from(1)).max(first);
    let block: Vec<&str> = (first..=last).map(|line| old.line_text(line)).collect();
    let from = new.line_of(written.start());
    let to = new.line_of(written.end());
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
    let start = old.line_bounds(first).start();
    let at = new.line_bounds(found).start();
    Some(TextRange::new(
        at + (range.start() - start),
        at + (range.end() - start),
    ))
}
