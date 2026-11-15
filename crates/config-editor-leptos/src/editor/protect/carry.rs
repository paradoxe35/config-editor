//! Locks carried over a text that does not parse. Such a text cannot be
//! read for where its fields are (an unclosed quote or tag above a locked
//! block hides the block from any reading), so each lock of the text before
//! is moved through the edit instead: a stretch before or after the edited
//! region keeps its text and moves with it, and one whose first and last
//! lines the edit left whole (Replace All, a comment toggle, a line copied
//! in among them) moves with its lines.

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

/// An edit inserting or deleting text only (the texts alike but for it,
/// `prefix` and `suffix` units kept), slid back over the text it repeats
/// to start at a line start when it can: a line pasted above one that
/// starts alike (`password2: x` above `password: hunter2`) is that line,
/// not a piece starting inside the line below, so the spans after it stay
/// on their own text. Otherwise as given.
pub(super) fn slid_to_line_start(
    old: &[u16],
    new: &[u16],
    prefix: usize,
    suffix: usize,
) -> (usize, usize) {
    let (removed, inserted) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
    let (text, len) = match (removed, inserted) {
        (0, 0) => return (prefix, suffix),
        (0, len) => (new, len),
        (len, 0) => (old, len),
        _ => return (prefix, suffix),
    };
    let line_start = |at: usize| at == 0 || text[at - 1] == u16::from(b'\n');
    if line_start(prefix) {
        return (prefix, suffix);
    }
    let mut at = prefix;
    while at > 0 && text[at - 1] == text[at - 1 + len] {
        at -= 1;
        if line_start(at) {
            return (at, suffix + (prefix - at));
        }
    }
    (prefix, suffix)
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
        let (prefix, suffix) = slid_to_line_start(&old, &new, prefix, suffix);
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

    /// The lock in the text after, or `None` when the edit changed its
    /// value's text. A key holding it the edit changed (one the guard let
    /// change: above a field a host range covers) is no longer one.
    pub(super) fn lock(&self, lock: &Lock) -> Option<Lock> {
        let ranges: Option<Vec<Range<usize>>> =
            lock.ranges.iter().map(|range| self.range(range)).collect();
        Some(Lock {
            pointer: lock.pointer.clone(),
            key: lock.key.as_ref().and_then(|key| self.range(key)),
            ranges: ranges?,
            guards: lock
                .guards
                .iter()
                .filter_map(|range| self.range(range))
                .collect(),
            reason: lock.reason.clone(),
        })
    }

    /// Where `range` of the text before is in the text after, when the edit
    /// kept its text.
    pub(super) fn range(&self, range: &Range<usize>) -> Option<Range<usize>> {
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
        // Its first and last lines kept whole, in order: the stretch spans
        // what lies between them now (a line copied or moved in among its
        // lines is in it). A diff may take a line moved past its neighbour
        // for the neighbour moved: its lines written once in the text after,
        // whole, are where it went.
        let kept = lines
            .to_new
            .get(first)
            .copied()
            .flatten()
            .zip(lines.to_new.get(last).copied().flatten());
        let (new_first, new_last) = match kept {
            Some((new_first, new_last))
                if new_last >= new_first && (last == first || new_last > new_first) =>
            {
                (new_first, new_last)
            }
            _ => lines.written_once(first, last)?,
        };
        let start = lines.new.line_bounds(new_first).start
            + (range.start - lines.old.line_bounds(first).start);
        let end =
            lines.new.line_bounds(new_last).start + (range.end - lines.old.line_bounds(last).start);
        Some(start..end)
    }
}

impl<'a> LineMoves<'a> {
    /// The lines of the text after holding old lines `first..=last`, when
    /// they are written there once, whole and together.
    fn written_once(&self, first: usize, last: usize) -> Option<(usize, usize)> {
        let block: Vec<&str> = (first..=last)
            .map(|line| self.old.line_text(line))
            .collect();
        let count = block.len();
        let total = self.new.line_count();
        let mut found = None;
        for at in 0..total.saturating_sub(count - 1) {
            if (0..count).all(|index| self.new.line_text(at + index) == block[index]) {
                if found.is_some() {
                    return None;
                }
                found = Some((at, at + count - 1));
            }
        }
        found
    }

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
