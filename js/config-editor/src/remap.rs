//! Ranges a host gave for one text, moved to another: `readOnlyRanges` and
//! host diagnostics computed for a text the user has typed past (an echo
//! skipped, an update's `rangesFor`).
//!
//! The two texts differ in one stretch (what both start and end with is
//! kept). A place before the stretch stays, one after it moves by the
//! length change, and one inside it goes to the stretch's start (a range's
//! start) or end (a range's end): a range only ever grows, so no character
//! it covered and the edits kept is left out of it (a lock never lets a
//! locked character be edited). Typing in one place, the common case, maps
//! every range exactly.

use config_editor_leptos::{DiagnosticTarget, HostDiagnostic, TextPosition, TextRange};

use crate::text_lines::TextLines;

/// Moves positions in `from` to `to` (UTF-16 offsets of the stretch that
/// differs: from `start` to `from_end` in `from`, to `to_end` in `to`).
pub(crate) struct Remap<'a> {
    from: TextLines<'a>,
    to: TextLines<'a>,
    start: usize,
    from_end: usize,
    to_end: usize,
}

impl<'a> Remap<'a> {
    pub(crate) fn new(from: &'a str, to: &'a str) -> Self {
        let mut start = 0;
        let mut start_byte = 0;
        for (left, right) in from.chars().zip(to.chars()) {
            if left != right {
                break;
            }
            start += left.len_utf16();
            start_byte += left.len_utf8();
        }
        let mut end = 0;
        for (left, right) in from[start_byte..]
            .chars()
            .rev()
            .zip(to[start_byte..].chars().rev())
        {
            if left != right {
                break;
            }
            end += left.len_utf16();
        }
        let from = TextLines::new(from);
        let to = TextLines::new(to);
        Self {
            start,
            from_end: from.units() - end,
            to_end: to.units() - end,
            from,
            to,
        }
    }

    /// Where offset `offset` of `from` is in `to` for a range's start
    /// (`at_end` false) or end: inside the stretch, at its start or end; at
    /// a text inserted there, before it for an end and after it for a
    /// start (a range does not take in what is typed at its edge).
    fn offset(&self, offset: usize, at_end: bool) -> usize {
        let after = offset - self.from_end.min(offset) + self.to_end;
        if at_end {
            if offset <= self.start {
                offset
            } else if offset >= self.from_end {
                after
            } else {
                self.to_end
            }
        } else if offset >= self.from_end {
            after
        } else if offset <= self.start {
            offset
        } else {
            self.start
        }
    }

    fn position(&self, position: TextPosition, at_end: bool) -> TextPosition {
        self.to
            .position(self.offset(self.from.offset(position), at_end))
    }

    /// `range` of `from`, in `to`: never smaller than what is left of it.
    pub(crate) fn range(&self, range: TextRange) -> TextRange {
        let start = self.position(range.start, false);
        // An empty range where text was inserted: after it, as its start.
        TextRange::new(start, self.position(range.end, true).max(start))
    }

    pub(crate) fn ranges(&self, ranges: Vec<TextRange>) -> Vec<TextRange> {
        ranges.into_iter().map(|range| self.range(range)).collect()
    }

    /// Host diagnostics given for `from`, in `to` (by path or for the whole
    /// document: as they are).
    pub(crate) fn diagnostics(&self, diagnostics: Vec<HostDiagnostic>) -> Vec<HostDiagnostic> {
        diagnostics
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.target = match diagnostic.target {
                    DiagnosticTarget::Range(range) => DiagnosticTarget::Range(self.range(range)),
                    DiagnosticTarget::Position(at) => {
                        DiagnosticTarget::Position(self.position(at, false))
                    }
                    target => target,
                };
                diagnostic
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use config_editor_leptos::{TextPosition, TextRange};

    use super::Remap;
    use crate::text_lines::TextLines;

    fn rng(mut seed: u64) -> impl FnMut(usize) -> usize {
        move |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % bound.max(1) as u64) as usize
        }
    }

    /// Typing in one place (a key, a line, a paste, a deletion): a range
    /// away from it covers the same text, wherever the lines moved.
    #[test]
    fn a_range_away_from_the_edit_covers_the_same_text() {
        let from = "name: web\r\nid: srv-42\nnote: é😀 x\n";
        let lock = TextRange::new(TextPosition::new(2, 1), TextPosition::new(2, 11));
        for to in [
            "#abcdefgh\nname: web\r\nid: srv-42\nnote: é😀 x\n",
            "name: web\r\nid: srv-42\nnote: é😀 xyz\n",
            "na\r\nid: srv-42\nnote: é😀 x\n",
            "name: web\r\nid: srv-42\nnote: x\n",
        ] {
            let mapped = Remap::new(from, to).range(lock);
            assert_eq!(mapped.slice(to), "id: srv-42", "{to:?}");
        }
    }

    /// Text typed at a range's edge is not taken in; an empty range there
    /// stays empty.
    #[test]
    fn text_typed_at_a_ranges_edge_stays_out_of_it() {
        let range = TextRange::new(TextPosition::new(1, 3), TextPosition::new(1, 5));
        let empty = TextRange::new(TextPosition::new(1, 3), TextPosition::new(1, 3));
        assert_eq!(
            Remap::new("abcdef", "abXcdef")
                .range(range)
                .slice("abXcdef"),
            "cd"
        );
        assert_eq!(
            Remap::new("abcdef", "abcdXef")
                .range(range)
                .slice("abcdXef"),
            "cd"
        );
        assert_eq!(
            Remap::new("abcdef", "abXcdef")
                .range(empty)
                .slice("abXcdef"),
            ""
        );
        // Typed inside: the range grows over it.
        assert_eq!(
            Remap::new("abcdef", "abcXdef")
                .range(range)
                .slice("abcXdef"),
            "cXd"
        );
    }

    /// Random texts, each changed by one to three edits, and random ranges
    /// of them: every character of a range that is kept (outside the
    /// stretch the texts differ in) is inside the range mapped, and a range
    /// clear of that stretch covers the same text.
    #[test]
    fn a_mapped_range_keeps_every_character_kept() {
        let alphabet: Vec<char> = "ab\n é😀".chars().collect();
        let mut next = rng(0x9e37_79b9_7f4a_7c15);
        let mut exact = 0;
        for round in 0..20_000 {
            let mut text: Vec<char> = (0..next(24))
                .map(|_| alphabet[next(alphabet.len())])
                .collect();
            let from: String = text.iter().collect();
            for _ in 0..1 + next(3) {
                let at = next(text.len() + 1);
                let removed = next(4).min(text.len() - at);
                let inserted: Vec<char> = (0..next(4))
                    .map(|_| alphabet[next(alphabet.len())])
                    .collect();
                text.splice(at..at + removed, inserted);
            }
            let to: String = text.iter().collect();
            let (old, new): (Vec<char>, Vec<char>) = (from.chars().collect(), to.chars().collect());
            let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let suffix = old[prefix..]
                .iter()
                .rev()
                .zip(new[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            // A character index of `from` to its place in `to`, if kept.
            let kept = |index: usize| {
                if index < prefix {
                    Some(index)
                } else if index >= old.len() - suffix {
                    Some(index + new.len() - old.len())
                } else {
                    None
                }
            };
            let units = |text: &[char], chars: usize| -> usize {
                text[..chars].iter().copied().map(char::len_utf16).sum()
            };
            let (a, b) = (next(old.len() + 1), next(old.len() + 1));
            let (start, end) = (a.min(b), a.max(b));
            let lines = TextLines::new(&from);
            let range = TextRange::new(
                lines.position(units(&old, start)),
                lines.position(units(&old, end)),
            );
            let mapped = Remap::new(&from, &to).range(range);
            let to_lines = TextLines::new(&to);
            let covered = to_lines.offset(mapped.start)..to_lines.offset(mapped.end);
            for index in start..end {
                if let Some(place) = kept(index) {
                    assert!(
                        covered.contains(&units(&new, place)),
                        "round {round}: {from:?} -> {to:?}, {start}..{end} -> {mapped:?}"
                    );
                }
            }
            if end <= prefix || start >= old.len() - suffix {
                exact += 1;
                assert_eq!(
                    mapped.slice(&to),
                    range.slice(&from),
                    "round {round}: {from:?} -> {to:?}, {start}..{end}"
                );
            }
        }
        assert!(exact > 5_000, "{exact}");
    }
}
