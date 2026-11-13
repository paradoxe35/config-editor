//! How protected text looks, line by line: a lock in the gutter on each
//! line a lock covers, and its value muted (its own key is not); a key in
//! the gutter beside a secret placeholder, drawn as a chip.

use std::ops::Range;

use super::super::keyboard::utf16_to_byte_index;
use super::super::line_rows::MarkedLines;
use super::locate::LineIndex;
use super::locks::Protected;
use super::texts;
use crate::EditorStrings;

/// The marks of the lines of `text` that have any (none when nothing is
/// protected),
/// with its secret placeholders at `tokens` (UTF-16) and their `hint`.
/// One pass over the text: each piece is measured within its own line,
/// so a document with thousands of placeholders stays linear.
pub(crate) fn line_marks(
    text: &str,
    protected: &Protected,
    tokens: &[Range<usize>],
    hint: &str,
    strings: &EditorStrings,
) -> MarkedLines {
    if protected.is_empty() && tokens.is_empty() {
        return MarkedLines::new();
    }
    let lines = LineIndex::new(text);
    let mut marks = MarkedLines::new();
    for lock in &protected.locks {
        let label = lock.reason.label(strings);
        for range in &lock.ranges {
            for (piece, line) in pieces(range, lock.key.as_ref(), &lines) {
                let bounds = lines.line_bounds(line);
                let line_text = lines.line_text(line);
                let byte = |at: usize| utf16_to_byte_index(line_text, at - bounds.start);
                let entry = marks.entry(line).or_default();
                if piece.start < piece.end {
                    entry.muted.push(byte(piece.start)..byte(piece.end));
                }
                entry.lock.get_or_insert_with(|| label.clone());
            }
        }
    }
    let key_label = texts::secret_label(strings, hint);
    for token in tokens {
        for (piece, line) in pieces(token, None, &lines) {
            let bounds = lines.line_bounds(line);
            let line_text = lines.line_text(line);
            let byte = |at: usize| utf16_to_byte_index(line_text, at - bounds.start);
            let entry = marks.entry(line).or_default();
            entry.secret.push(byte(piece.start)..byte(piece.end));
            entry.key.get_or_insert_with(|| key_label.clone());
        }
    }
    for entry in marks.values_mut() {
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

#[cfg(test)]
mod tests {
    use super::super::locks::Protected;
    use super::line_marks;

    /// Each placeholder is measured within its own line: a document with
    /// thousands of them (a large config's secrets) is marked in one pass.
    /// Measured from the document's start, as before, this took minutes.
    #[test]
    fn many_placeholders_are_marked_in_linear_time() {
        let lines = 20_000;
        let line = "  secret: «secret 3f2a» # é\n";
        let text = line.repeat(lines);
        let line_units = line.encode_utf16().count();
        let token_start = "  secret: ".encode_utf16().count();
        let token_units = "«secret 3f2a»".encode_utf16().count();
        let tokens: Vec<_> = (0..lines)
            .map(|index| {
                let start = index * line_units + token_start;
                start..start + token_units
            })
            .collect();
        let strings = crate::EditorStrings::default();
        let (took, marks) = crate::editor::test_timing::best_of(3, || {
            line_marks(&text, &Protected::default(), &tokens, "hint", &strings)
        });
        let byte_start = "  secret: ".len();
        let expected = byte_start..byte_start + "«secret 3f2a»".len();
        // Only the lines with a placeholder: not an entry for every line.
        assert_eq!(marks.len(), lines);
        assert!(
            marks
                .values()
                .all(|marks| marks.secret == [expected.clone()])
        );
        assert!(took.as_secs() < 5, "{took:?}");
    }
}
