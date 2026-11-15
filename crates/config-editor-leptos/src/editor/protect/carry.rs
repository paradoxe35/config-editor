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
use super::texts::Reason;

/// How the UTF-16 offsets of one text map onto the text an edit made of it.
pub(super) struct Moves<'a> {
    before: &'a str,
    after: &'a str,
    old: Vec<u16>,
    new: Vec<u16>,
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

/// Where the region an edit replaced starts, when the caret it left
/// (`caret`, UTF-16 in `new`) places it: the inserted text ends there, and
/// the texts agree before it starts and after it ends (`prefix` and
/// `shared_end` units the texts share at their start and end, which may
/// overlap). `None` when the caret does not fit the texts.
pub(super) fn placed_by_caret(
    old: &[u16],
    new: &[u16],
    prefix: usize,
    shared_end: usize,
    caret: usize,
) -> Option<usize> {
    let tail = shared_end
        .min(old.len() - prefix.min(old.len()))
        .min(new.len() - prefix.min(new.len()));
    let removed = old.len() - tail - prefix.min(old.len());
    let inserted = new.len() - tail - prefix.min(new.len());
    let from = caret.checked_sub(inserted)?;
    (from <= prefix && from + removed <= old.len() && old.len() - (from + removed) <= shared_end)
        .then_some(from)
}

impl<'a> Moves<'a> {
    pub(super) fn new(before: &'a str, after: &'a str) -> Self {
        Self::at(before, after, None)
    }

    /// The moves of the edit from `before` to `after` that left its caret
    /// at `caret` (UTF-16), when known: the region it replaced is placed by
    /// it, as the guard placed it (see [`placed_by_caret`]), so text
    /// inserted beside a copy of itself goes where it was typed.
    pub(super) fn at(before: &'a str, after: &'a str, caret: Option<usize>) -> Self {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let shared_end = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = shared_end.min(old.len() - prefix).min(new.len() - prefix);
        let (prefix, suffix) =
            match caret.and_then(|caret| placed_by_caret(&old, &new, prefix, shared_end, caret)) {
                Some(from) => {
                    let removed = old.len() - prefix - suffix;
                    (from, old.len() - from - removed)
                }
                None => slid_to_line_start(&old, &new, prefix, suffix),
            };
        Moves {
            before,
            after,
            prefix,
            suffix,
            old_len: old.len(),
            new_len: new.len(),
            old,
            new,
            lines: OnceCell::new(),
        }
    }

    /// The lock in the text after, or `None` when the edit changed its
    /// value's text. A key holding it the edit changed (one the guard let
    /// change: above a field a host range covers) is no longer one. A key
    /// on the line its value starts on moves with the value, as written
    /// there: of two alike lines, the one the value went to (the other
    /// edited, or the value's line one of two copies) holds its key too.
    pub(super) fn lock(&self, lock: &Lock) -> Option<Lock> {
        // A field a host range covers keeps its value's text, not its key's
        // (the guard lets the key change where no range is on screen): its
        // value is followed past the key, so a renamed key leaves the field
        // locked, and held where it reads otherwise.
        let value_only = |range: &Range<usize>| match &lock.key {
            Some(key)
                if lock.pointer.is_some()
                    && lock.reason == Reason::Range
                    && key.start <= range.start
                    && range.start < key.end
                    && key.end < range.end =>
            {
                key.end..range.end
            }
            _ => range.clone(),
        };
        let ranges: Vec<Range<usize>> = lock
            .ranges
            .iter()
            .map(|range| self.range(range).or_else(|| self.range(&value_only(range))))
            .collect::<Option<_>>()?;
        let key = |span: &Range<usize>| {
            lock.ranges
                .iter()
                .zip(&ranges)
                .find_map(|(old, new)| self.along(span, old, new))
                .or_else(|| self.range(span))
        };
        Some(Lock {
            pointer: lock.pointer.clone(),
            key: lock.key.as_ref().and_then(key),
            guards: lock.guards.iter().filter_map(key).collect(),
            ranges,
            reason: lock.reason.clone(),
        })
    }

    /// `span` moved as `old` moved to `new`, when it is on the line `old`
    /// starts on and its text is written there in the text after.
    fn along(
        &self,
        span: &Range<usize>,
        old: &Range<usize>,
        new: &Range<usize>,
    ) -> Option<Range<usize>> {
        let (low, high) = (span.start.min(old.start), span.end.max(old.start));
        if high > self.old.len() || self.old[low..high].contains(&u16::from(b'\n')) {
            return None;
        }
        let start = (span.start + new.start).checked_sub(old.start)?;
        let moved = start..start + span.len();
        (self.new.get(moved.clone()) == Some(&self.old[span.clone()])).then_some(moved)
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
        if let Some(moved) = self.through_spacing(range) {
            return Some(moved);
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

impl Moves<'_> {
    /// Where `range` is after an edit that changed only whitespace where it
    /// changed anything (the guard lets spacing between tokens change in a
    /// text that does not parse, and re-flowing in one that does): each end
    /// stays with the characters beside it. `None` for any other edit.
    fn through_spacing(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let old = &self.old[self.prefix..self.old_len - self.suffix];
        let new = &self.new[self.prefix..self.new_len - self.suffix];
        let solid = |unit: &&u16| !is_blank(**unit);
        if !old.iter().filter(solid).eq(new.iter().filter(solid)) {
            return None;
        }
        // The new region's offset of its `n`th solid unit (from 0), or its
        // end.
        let nth_solid = |n: usize| {
            new.iter()
                .enumerate()
                .filter(|(_, unit)| !is_blank(**unit))
                .nth(n)
                .map_or(new.len(), |(at, _)| at)
        };
        let map = |at: usize, end: bool| {
            if at <= self.prefix {
                return at;
            }
            if at >= self.old_len - self.suffix {
                return at + self.new_len - self.old_len;
            }
            let before = old[..at - self.prefix].iter().filter(solid).count();
            let at = match (end, before) {
                (true, 0) => 0,
                (true, n) => nth_solid(n - 1) + 1,
                (false, n) => nth_solid(n),
            };
            self.prefix + at
        };
        let start = map(range.start, false);
        Some(start..map(range.end, true).max(start))
    }
}

/// Whether `unit` is whitespace.
fn is_blank(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(char::is_whitespace)
}

impl<'a> LineMoves<'a> {
    /// The lines of the text after holding old lines `first..=last`,
    /// whole and together: of several copies, the one nearest where they
    /// were, as the host's ranges follow their lines (`follow`), so a
    /// field's lock and the range covering it stay on the same copy.
    fn written_once(&self, first: usize, last: usize) -> Option<(usize, usize)> {
        let block: Vec<&str> = (first..=last)
            .map(|line| self.old.line_text(line))
            .collect();
        let count = block.len();
        let total = self.new.line_count();
        (0..total.saturating_sub(count - 1))
            .filter(|at| (0..count).all(|index| self.new.line_text(at + index) == block[index]))
            .min_by_key(|at| at.abs_diff(first))
            .map(|at| (at, at + count - 1))
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

    #[test]
    fn ranges_move_through_spacing_changed_inside_them() {
        // A space typed after `:` inside the span, the text broken: it
        // keeps its text, its spacing new.
        let before = "  user: svc\n  password: hunter2\n";
        let after = "  user: svc\n  password:  hunter2\n";
        let moves = Moves::new(before, after);
        assert_eq!(moves.range(&(14..31)), Some(14..32));
        // An indent changed above it.
        let moves = Moves::new("a:\n  b: 1\n", "a:\n    b: 1\n");
        assert_eq!(moves.range(&(5..9)), Some(7..11));
        // A word typed is no spacing.
        let moves = Moves::new(before, "  user: svc\n  password: x hunter2\n");
        assert_eq!(moves.range(&(14..31)), None);
    }

    #[test]
    fn a_key_moves_with_its_value_to_the_alike_line_it_went_to() {
        use super::super::texts::Reason;
        // Of two alike lines, the first's value edited: the lock's text is
        // the second's now, its key and the key holding it with it.
        let moves = Moves::new("a: <k: 1>\na: <k: 1>\n", "a: <k: 2>\na: <k: 1>\n");
        let lock = Lock {
            pointer: Some("/a/k".to_string()),
            key: Some(4..5),
            ranges: std::iter::once(4..8).collect(),
            guards: std::iter::once(0..1).collect(),
            reason: Reason::Field,
        };
        let moved = moves.lock(&lock).expect("its text is kept");
        assert_eq!(moved.ranges.first(), Some(&(14..18)));
        assert_eq!(moved.key, Some(14..15));
        assert_eq!(moved.guards.first(), Some(&(10..11)));
    }
}
