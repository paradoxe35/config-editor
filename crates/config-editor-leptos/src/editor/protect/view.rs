//! How protected text looks, line by line: a lock in the gutter on each
//! line a lock covers, and its value muted (its own key is not); a key in
//! the gutter beside a secret placeholder, drawn as a chip.

use super::super::line_rows::MarkedLines;
use super::super::text::{TextRange, TextSize};
use super::locate::LineIndex;
use super::locks::Protected;
use super::texts;
use crate::EditorStrings;

/// The marks of the lines of `text` that have any (none when nothing is
/// protected),
/// with its secret placeholders at `tokens` and their `hint`.
/// One pass over the text: each piece is measured within its own line,
/// so a document with thousands of placeholders stays linear.
pub(crate) fn line_marks(
    text: &str,
    protected: &Protected,
    tokens: &[TextRange],
    hint: &str,
    strings: &EditorStrings,
) -> MarkedLines {
    if protected.is_empty() && tokens.is_empty() {
        return MarkedLines::new();
    }
    let lines = LineIndex::new(text);
    let mut marks = MarkedLines::new();
    // A piece as bytes of its line (the rows' marks).
    let in_line = |piece: TextRange, line: usize| {
        let start = lines.line_bounds(line).start();
        usize::from(piece.start() - start)..usize::from(piece.end() - start)
    };
    for lock in &protected.locks {
        let label = lock.reason.label(strings);
        for range in &lock.ranges {
            for (piece, line) in pieces(range, lock.key.as_ref(), &lines) {
                let entry = marks.entry(line).or_default();
                if !piece.is_empty() {
                    entry.muted.push(in_line(piece, line));
                }
                entry.lock.get_or_insert_with(|| label.clone());
            }
        }
    }
    let key_label = texts::secret_label(strings, hint);
    for token in tokens {
        for (piece, line) in pieces(token, None, &lines) {
            let entry = marks.entry(line).or_default();
            entry.secret.push(in_line(piece, line));
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
    range: &TextRange,
    key: Option<&TextRange>,
    lines: &LineIndex<'_>,
) -> Vec<(TextRange, usize)> {
    let mut out = Vec::new();
    let last = range
        .end()
        .checked_sub(TextSize::from(1))
        .unwrap_or_default()
        .max(range.start());
    for line in lines.line_of(range.start())..=lines.line_of(last) {
        let bounds = lines.line_bounds(line);
        let start = range.start().max(bounds.start());
        let end = range.end().min(bounds.end()).max(start);
        match key.filter(|key| key.start() < end && start < key.end()) {
            Some(key) => {
                out.push((TextRange::new(start, key.start().max(start)), line));
                out.push((TextRange::new(key.end().min(end), end), line));
            }
            None => out.push((TextRange::new(start, end), line)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::text::{TextRange, TextSize, text_size};
    use super::super::locks::Protected;
    use super::line_marks;

    /// Each placeholder is measured within its own line: a document with
    /// thousands of them (a large config's secrets) is marked in one pass.
    /// Measured from the document's start, as before, this took minutes.
    #[test]
    fn many_placeholders_are_marked_in_linear_time() {
        let line = "  secret: «secret 3f2a» # é\n";
        let token_start = "  secret: ".len();
        let token_len = TextSize::of("«secret 3f2a»");
        let strings = crate::EditorStrings::default();
        let marked = |lines: usize| {
            let text = line.repeat(lines);
            let tokens: Vec<_> = (0..lines)
                .map(|index| TextRange::at(text_size(index * line.len() + token_start), token_len))
                .collect();
            line_marks(&text, &Protected::default(), &tokens, "hint", &strings)
        };
        // Twice the lines, twice the steps placing the offsets (counted).
        let grown = crate::editor::work_count::grown_faster(2.5, (10_000, 20_000), marked);
        assert!(grown.is_empty(), "{grown:?}");
        let lines = 20_000;
        let marks = marked(lines);
        let byte_start = "  secret: ".len();
        let expected = byte_start..byte_start + "«secret 3f2a»".len();
        // Only the lines with a placeholder: not an entry for every line.
        assert_eq!(marks.len(), lines);
        assert!(
            marks
                .values()
                .all(|marks| marks.secret == [expected.clone()])
        );
    }
}
