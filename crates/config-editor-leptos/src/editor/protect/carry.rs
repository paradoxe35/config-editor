//! Locks carried over a text that does not parse. Such a text cannot be
//! read for where its fields are (an unclosed quote or tag above a locked
//! block hides the block from any reading), so each lock of the text before
//! is moved through the edit instead: a stretch before or after the edited
//! region keeps its text and moves with it, and one on lines the edit left
//! whole (Replace All, a comment toggle) moves with its lines.

use std::cell::OnceCell;
use std::ops::Range;

use super::super::diff::{DiffEdit, myers_edits};
use super::locate::LineIndex;
use super::locks::Lock;

/// How the UTF-16 offsets of one text map onto the text an edit made of it.
pub(super) struct Moves<'a> {
    before: &'a str,
    after: &'a str,
    /// Units the two texts share at their start and end (not overlapping).
    prefix: usize,
    suffix: usize,
    old_len: usize,
    new_len: usize,
    lines: OnceCell<Option<LineMoves<'a>>>,
}

/// The line diff of the two texts, read only when an edit changed several
/// places.
struct LineMoves<'a> {
    old: LineIndex<'a>,
    new: LineIndex<'a>,
    /// For each line of the text before, the line it is in the text after
    /// when the edit kept it whole.
    to_new: Vec<Option<usize>>,
}

impl<'a> Moves<'a> {
    pub(super) fn new(before: &'a str, after: &'a str) -> Self {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take_while(|(a, b)| a == b)
            .count()
            .min(old.len() - prefix)
            .min(new.len() - prefix);
        Moves {
            before,
            after,
            prefix,
            suffix,
            old_len: old.len(),
            new_len: new.len(),
            lines: OnceCell::new(),
        }
    }

    /// The lock in the text after, or `None` when the edit changed its text.
    pub(super) fn lock(&self, lock: &Lock) -> Option<Lock> {
        let ranges = |ranges: &[Range<usize>]| -> Option<Vec<Range<usize>>> {
            ranges.iter().map(|range| self.range(range)).collect()
        };
        Some(Lock {
            pointer: lock.pointer.clone(),
            key: match &lock.key {
                Some(key) => Some(self.range(key)?),
                None => None,
            },
            ranges: ranges(&lock.ranges)?,
            guards: ranges(&lock.guards)?,
            reason: lock.reason.clone(),
        })
    }

    /// Where `range` of the text before is in the text after, when the edit
    /// kept its text.
    fn range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        if range.end <= self.prefix {
            return Some(range.clone());
        }
        if range.start >= self.old_len - self.suffix {
            let start = range.start + self.new_len - self.old_len;
            return Some(start..start + range.len());
        }
        let lines = self
            .lines
            .get_or_init(|| LineMoves::new(self.before, self.after))
            .as_ref()?;
        let first = lines.old.line_of(range.start);
        let last = lines
            .old
            .line_of(range.end.saturating_sub(1).max(range.start));
        let new_first = (*lines.to_new.get(first)?)?;
        for line in first..=last {
            if *lines.to_new.get(line)? != Some(new_first + line - first) {
                return None;
            }
        }
        let start = lines.new.line_bounds(new_first).start
            + (range.start - lines.old.line_bounds(first).start);
        let end = lines.new.line_bounds(new_first + last - first).start
            + (range.end - lines.old.line_bounds(last).start);
        Some(start..end)
    }
}

impl<'a> LineMoves<'a> {
    fn new(before: &'a str, after: &'a str) -> Option<Self> {
        let old: Vec<&str> = before.split('\n').collect();
        let new: Vec<&str> = after.split('\n').collect();
        let mut to_new = Vec::with_capacity(old.len());
        let mut at = 0;
        for edit in myers_edits(&old, &new)? {
            match edit {
                DiffEdit::Equal => {
                    to_new.push(Some(at));
                    at += 1;
                }
                DiffEdit::Delete => to_new.push(None),
                DiffEdit::Insert => at += 1,
            }
        }
        Some(LineMoves {
            old: LineIndex::new(before),
            new: LineIndex::new(after),
            to_new,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_move_with_the_text_around_them() {
        let moves = Moves::new("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 2\nXc: 3\n");
        assert_eq!(moves.range(&(5..9)), Some(5..9));
        assert_eq!(moves.range(&(10..14)), Some(11..15));
        assert_eq!(moves.range(&(9..11)), None);
        // Two places changed, the line between them kept whole.
        let moves = Moves::new("x\nb: 2\ny\n", "xx\nb: 2\nyy\n");
        assert_eq!(moves.range(&(2..6)), Some(3..7));
    }
}
