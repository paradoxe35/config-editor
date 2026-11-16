//! Locks carried over a text that does not parse. Such a text cannot be
//! read for where its fields are (an unclosed quote or tag above a locked
//! block hides the block from any reading), so each lock of the text before
//! is moved through the edit instead: a stretch before or after the edited
//! region keeps its text and moves with it, and one whose first and last
//! lines the edit left whole (Replace All, a comment toggle, a line copied
//! in among them) moves with its lines.

use std::cell::OnceCell;

use super::super::diff::{DiffEdit, myers_edits};
use super::super::edit::Change;
use super::super::text::{TextRange, TextSize, slice, text_size};
use super::locate::LineIndex;
use super::locks::Lock;
use super::texts::Reason;

/// How the offsets of one text map onto the text an edit made of it.
pub(super) struct Moves<'a> {
    before: &'a str,
    after: &'a str,
    /// Bytes the two texts share at their start and end (not overlapping),
    /// each ending on a character boundary.
    prefix: TextSize,
    suffix: TextSize,
    old_len: TextSize,
    new_len: TextSize,
    lines: OnceCell<Option<LineMoves<'a>>>,
    /// What the edit recorded it did, when it did: spans are mapped
    /// through it, never through a line diff.
    change: Option<&'a Change>,
}

/// One range of the text before an edit replaced, the range of the text
/// after it wrote there, and that text.
pub(super) struct Piece {
    pub(super) replaced: TextRange,
    pub(super) wrote: TextRange,
    pub(super) written: String,
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

/// The bytes `before` and `after` share at their start and at their end
/// (the two may overlap), each ending on a character boundary of both.
fn shared_ends(before: &str, after: &str) -> (usize, usize) {
    let (old, new) = (before.as_bytes(), after.as_bytes());
    let mut prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut shared_end = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(old.len() - shared_end) {
        shared_end -= 1;
    }
    (prefix, shared_end)
}

/// An edit inserting or deleting text only (the texts alike but for it,
/// `prefix` and `suffix` bytes kept), slid back over the text it repeats
/// to start at a line start when it can: a line pasted above one that
/// starts alike (`password2: x` above `password: hunter2`) is that line,
/// not a piece starting inside the line below, so the spans after it stay
/// on their own text. Otherwise as given.
fn slid_to_line_start(old: &[u8], new: &[u8], prefix: usize, suffix: usize) -> (usize, usize) {
    let (removed, inserted) = (old.len() - prefix - suffix, new.len() - prefix - suffix);
    let (text, len) = match (removed, inserted) {
        (0, 0) => return (prefix, suffix),
        (0, len) => (new, len),
        (len, 0) => (old, len),
        _ => return (prefix, suffix),
    };
    let line_start = |at: usize| at == 0 || text[at - 1] == b'\n';
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
/// (`caret`, a byte of `after`) places it: the inserted text ends there,
/// and the texts agree before it starts and after it ends (`prefix` and
/// `shared_end` bytes the texts share at their start and end, which may
/// overlap). `None` when the caret does not fit the texts.
fn placed_by_caret(
    (before, after): (&str, &str),
    prefix: usize,
    shared_end: usize,
    caret: TextSize,
) -> Option<usize> {
    let (old, new) = (before.len(), after.len());
    let tail = shared_end
        .min(old - prefix.min(old))
        .min(new - prefix.min(new));
    let removed = old - tail - prefix.min(old);
    let inserted = new - tail - prefix.min(new);
    let from = usize::from(caret).checked_sub(inserted)?;
    (from <= prefix
        && from + removed <= old
        && old - (from + removed) <= shared_end
        && before.is_char_boundary(from))
    .then_some(from)
}

impl<'a> Moves<'a> {
    pub(super) fn new(before: &'a str, after: &'a str) -> Self {
        Self::at(before, after, None, None)
    }

    /// The moves of the edit from `before` to `after`: those `change`
    /// records when it does; else read off the texts and the caret it left
    /// at `caret`, when known: the region it replaced is placed by it (see
    /// [`placed_by_caret`]), so text inserted beside a copy of itself goes
    /// where it was typed.
    pub(super) fn at(
        before: &'a str,
        after: &'a str,
        caret: Option<TextSize>,
        change: Option<&'a Change>,
    ) -> Self {
        let (old, new) = (before.as_bytes(), after.as_bytes());
        let (prefix, shared_end) = shared_ends(before, after);
        let mut suffix = shared_end.min(old.len() - prefix).min(new.len() - prefix);
        while !before.is_char_boundary(old.len() - suffix) {
            suffix -= 1;
        }
        let placed =
            caret.and_then(|caret| placed_by_caret((before, after), prefix, shared_end, caret));
        let (prefix, suffix) = match placed {
            Some(from) => {
                let removed = old.len() - prefix - suffix;
                (from, old.len() - from - removed)
            }
            None => slid_to_line_start(old, new, prefix, suffix),
        };
        Moves {
            before,
            after,
            prefix: text_size(prefix),
            suffix: text_size(suffix),
            old_len: TextSize::of(before),
            new_len: TextSize::of(after),
            lines: OnceCell::new(),
            change,
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
        let value_only = |range: &TextRange| match &lock.key {
            Some(key)
                if lock.pointer.is_some()
                    && lock.reason == Reason::Range
                    && key.start() <= range.start()
                    && range.start() < key.end()
                    && key.end() < range.end() =>
            {
                TextRange::new(key.end(), range.end())
            }
            _ => *range,
        };
        let ranges: Vec<TextRange> = lock
            .ranges
            .iter()
            .map(|range| self.range(range).or_else(|| self.range(&value_only(range))))
            .collect::<Option<_>>()?;
        let key = |span: &TextRange| {
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

    /// What the edit wrote, one piece per range it replaced, in order: as
    /// its change records it (each match of a Replace All its own piece),
    /// else the one stretch between what the two texts share.
    pub(super) fn pieces(&self) -> Vec<Piece> {
        if let Some(change) = self
            .change
            .filter(|change| change.len_after() == self.new_len)
        {
            // What the replacements before each one wrote and deleted.
            let (mut grown, mut shrunk) = (TextSize::default(), TextSize::default());
            return change
                .edits()
                .map(|(replaced, written)| {
                    let wrote =
                        TextRange::at(replaced.start() + grown - shrunk, TextSize::of(written));
                    grown += wrote.len();
                    shrunk += replaced.len();
                    Piece {
                        replaced,
                        wrote,
                        written: written.to_string(),
                    }
                })
                .collect();
        }
        let wrote = TextRange::new(self.prefix, self.new_len - self.suffix);
        vec![Piece {
            replaced: TextRange::new(self.prefix, self.old_len - self.suffix),
            written: slice::slice(self.after, wrote)
                .unwrap_or_default()
                .to_string(),
            wrote,
        }]
    }

    /// `span` moved as `old` moved to `new`, when it is on the line `old`
    /// starts on and its text is written there in the text after.
    fn along(&self, span: &TextRange, old: &TextRange, new: &TextRange) -> Option<TextRange> {
        let (low, high) = (span.start().min(old.start()), span.end().max(old.start()));
        if slice::slice(self.before, TextRange::new(low, high))?.contains('\n') {
            return None;
        }
        let start = (span.start() + new.start()).checked_sub(old.start())?;
        let moved = TextRange::at(start, span.len());
        (slice::slice(self.after, moved) == slice::slice(self.before, *span)).then_some(moved)
    }

    /// Where `range` of the text before is in the text after, when the edit
    /// kept its text.
    pub(super) fn range(&self, range: &TextRange) -> Option<TextRange> {
        if let Some(change) = self.change {
            // Whitespace re-flowed inside it (a reformat in one edit) moves
            // each end with the characters beside it.
            return change
                .map_range(range)
                .or_else(|| self.through_spacing(range));
        }
        if range.end() <= self.prefix {
            return Some(*range);
        }
        if range.start() >= self.old_len - self.suffix {
            // Past the edit, the text after is as long as the text before
            // plus what the edit wrote less what it took.
            return Some(TextRange::at(
                range.start() + self.new_len - self.old_len,
                range.len(),
            ));
        }
        if let Some(moved) = self.through_spacing(range) {
            return Some(moved);
        }
        // An edit made inside it changed its text: no alike line elsewhere
        // is it.
        if range.start() < self.prefix && self.old_len - self.suffix < range.end() {
            return None;
        }
        let lines = self
            .lines
            .get_or_init(|| LineMoves::new(self.before, self.after))
            .as_ref()?;
        let first = lines.old.line_of(range.start());
        let last = lines.old.line_of(
            range
                .end()
                .checked_sub(TextSize::from(1))
                .unwrap_or_default()
                .max(range.start()),
        );
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
        let start = lines.new.line_bounds(new_first).start()
            + (range.start() - lines.old.line_bounds(first).start());
        let end = lines.new.line_bounds(new_last).start()
            + (range.end() - lines.old.line_bounds(last).start());
        Some(TextRange::new(start, end))
    }
}

impl Moves<'_> {
    /// Where `range` is after an edit that changed only whitespace where it
    /// changed anything (the guard lets spacing between tokens change in a
    /// text that does not parse, and re-flowing in one that does): each end
    /// stays with the characters beside it. `None` for any other edit.
    fn through_spacing(&self, range: &TextRange) -> Option<TextRange> {
        let old = slice::slice(
            self.before,
            TextRange::new(self.prefix, self.old_len - self.suffix),
        )?;
        let new = slice::slice(
            self.after,
            TextRange::new(self.prefix, self.new_len - self.suffix),
        )?;
        let solid = |character: &char| !character.is_whitespace();
        if !old.chars().filter(solid).eq(new.chars().filter(solid)) {
            return None;
        }
        // The new region's offset of its `n`th solid character (from 0)
        // and that character, or its end.
        let nth_solid = |n: usize| {
            new.char_indices()
                .filter(|(_, character)| solid(character))
                .nth(n)
                .map(|(at, character)| (text_size(at), TextSize::of(character)))
        };
        let map = |at: TextSize, end: bool| {
            if at <= self.prefix {
                return Some(at);
            }
            if at >= self.old_len - self.suffix {
                return Some(at + self.new_len - self.old_len);
            }
            let before = slice::before(old, at - self.prefix)?
                .chars()
                .filter(solid)
                .count();
            let at = match (end, before) {
                (true, 0) => TextSize::default(),
                (true, n) => nth_solid(n - 1).map_or(TextSize::of(new), |(at, len)| at + len),
                (false, n) => nth_solid(n).map_or(TextSize::of(new), |(at, _)| at),
            };
            Some(self.prefix + at)
        };
        let start = map(range.start(), false)?;
        Some(TextRange::new(start, map(range.end(), true)?.max(start)))
    }
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

    fn r(start: u32, end: u32) -> TextRange {
        TextRange::new(start.into(), end.into())
    }

    #[test]
    fn ranges_move_with_the_text_around_them() {
        let moves = Moves::new("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 2\nXc: 3\n");
        assert_eq!(moves.range(&r(5, 9)), Some(r(5, 9)));
        assert_eq!(moves.range(&r(10, 14)), Some(r(11, 15)));
        assert_eq!(moves.range(&r(9, 11)), None);
        // Two places changed, the line between them kept whole.
        let moves = Moves::new("x\nb: 2\ny\n", "xx\nb: 2\nyy\n");
        assert_eq!(moves.range(&r(2, 6)), Some(r(3, 7)));
    }

    #[test]
    fn ranges_move_through_spacing_changed_inside_them() {
        // A space typed after `:` inside the span, the text broken: it
        // keeps its text, its spacing new.
        let before = "  user: svc\n  password: hunter2\n";
        let after = "  user: svc\n  password:  hunter2\n";
        let moves = Moves::new(before, after);
        assert_eq!(moves.range(&r(14, 31)), Some(r(14, 32)));
        // An indent changed above it.
        let moves = Moves::new("a:\n  b: 1\n", "a:\n    b: 1\n");
        assert_eq!(moves.range(&r(5, 9)), Some(r(7, 11)));
        // A word typed is no spacing.
        let moves = Moves::new(before, "  user: svc\n  password: x hunter2\n");
        assert_eq!(moves.range(&r(14, 31)), None);
    }

    #[test]
    fn a_key_moves_with_its_value_to_the_alike_line_it_went_to() {
        use super::super::texts::Reason;
        // Of two alike lines, the first's value edited: the lock's text is
        // the second's now, its key and the key holding it with it.
        let moves = Moves::new("a: <k: 1>\na: <k: 1>\n", "a: <k: 2>\na: <k: 1>\n");
        let lock = Lock {
            pointer: Some("/a/k".to_string()),
            key: Some(r(4, 5)),
            ranges: std::iter::once(r(4, 8)).collect(),
            guards: std::iter::once(r(0, 1)).collect(),
            reason: Reason::Field,
        };
        let moved = moves.lock(&lock).expect("its text is kept");
        assert_eq!(moved.ranges.first(), Some(&r(14, 18)));
        assert_eq!(moved.key, Some(r(14, 15)));
        assert_eq!(moved.guards.first(), Some(&r(10, 11)));
    }
}
