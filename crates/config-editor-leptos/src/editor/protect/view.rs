//! How protected text looks, line by line: a lock in the gutter on each
//! line a lock covers, and its value muted (its own key is not).

use std::ops::Range;

use super::super::keyboard::utf16_to_byte_index;
use super::locate::LineIndex;
use super::locks::Protected;
use super::texts;

/// What one line shows of protected text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineMarks {
    /// Byte ranges of the line drawn muted.
    pub(crate) muted: Vec<Range<usize>>,
    /// The gutter lock's label, with the reason.
    pub(crate) lock: Option<String>,
}

impl LineMarks {
    /// The byte ranges drawn specially, each with its class.
    pub(crate) fn classes(&self) -> Vec<(Range<usize>, &'static str)> {
        self.muted
            .iter()
            .map(|range| (range.clone(), "config-editor__locked"))
            .collect()
    }
}

/// The marks of every line of `text` (empty when nothing is protected).
pub(crate) fn line_marks(text: &str, protected: &Protected) -> Vec<LineMarks> {
    if protected.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    let mut marks = vec![LineMarks::default(); lines.line_count()];
    for lock in &protected.locks {
        let label = format!("{}: {}", texts::LOCK_LABEL, lock.reason);
        for range in &lock.ranges {
            for (piece, line) in pieces(range, lock.key.as_ref(), &lines) {
                let bounds = lines.line_bounds(line);
                let line_text = &text[utf16_to_byte_index(text, bounds.start)
                    ..utf16_to_byte_index(text, bounds.end)];
                let byte = |at: usize| utf16_to_byte_index(line_text, at - bounds.start);
                let entry = &mut marks[line];
                if piece.start < piece.end {
                    entry.muted.push(byte(piece.start)..byte(piece.end));
                }
                entry.lock.get_or_insert_with(|| label.clone());
            }
        }
    }
    for entry in &mut marks {
        entry.muted.sort_by_key(|range| range.start);
    }
    marks
}

/// `range` cut at line ends and with `key` taken out, each piece with its
/// zero-based line.
fn pieces(
    range: &Range<usize>,
    key: Option<&Range<usize>>,
    lines: &LineIndex<'_>,
) -> Vec<(Range<usize>, usize)> {
    let mut out = Vec::new();
    for line in
        lines.line_of(range.start)..=lines.line_of(range.end.saturating_sub(1).max(range.start))
    {
        let bounds = lines.line_bounds(line);
        let start = range.start.max(bounds.start);
        let end = range.end.min(bounds.end);
        match key.filter(|key| key.start < end && start < key.end) {
            Some(key) => {
                out.push((start..key.start.max(start), line));
                out.push((key.end.min(end)..end, line));
            }
            None => out.push((start..end.max(start), line)),
        }
    }
    out
}
