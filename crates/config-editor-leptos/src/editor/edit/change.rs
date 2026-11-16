//! What an edit did to the text, exactly: a change set (as CodeMirror 6's
//! `ChangeSet` and Helix's `Transaction`), recorded by the code making the
//! edit, so nothing that follows positions through it reads the change
//! back off the two texts.
//!
//! A [`Change`] is the sorted, apart replacements of a text of known
//! length, each a range of the text before and the text written there,
//! plus the stretches of the text before that the edit moved whole
//! elsewhere (a line command moving lines): their positions go with them.
//! The text after is made from the change ([`Change::apply`]), so the two
//! cannot disagree.
//!
//! Positions are counted in a [`Unit`]: bytes ([`TextSize`]) for the text
//! itself, UTF-16 code units ([`Utf16`]) for what follows the browser's
//! offsets; [`Change::in_utf16`] converts one to the other.
//!
//! - [`Change::map_pos`] maps a position, with an [`Assoc`] saying which
//!   side of an insertion made right at it the position keeps to, and
//!   whether the text around it was deleted;
//! - [`Change::invert`] undoes it, [`Change::compose`] makes one change of
//!   two in turn, [`Change::map`] rebases one change over another made on
//!   the same text (`a.compose(&b.map(&a, false))` reads as
//!   `b.compose(&a.map(&b, true))`).
//!
//! The code making an edit says its change ([`Change::replace`] and the
//! like). Only a text from outside is read off two texts
//! ([`Change::diffed`], [`Change::diffed_by_lines`]): a host value, a
//! reload, a browser input that does not say what it did.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Range, Sub, SubAssign};

use super::super::diff::{DiffEdit, myers_edits};
use super::super::keyboard::utf16_to_byte_index;
use super::super::text::{TextRange, TextSize, Utf16, text_size, utf16_of};
use super::super::text_change::{changed_bytes, edit_range};

/// A position or a length in a [`Unit`].
pub(crate) trait Offset:
    Copy + Debug + Default + Ord + Add<Output = Self> + Sub<Output = Self> + AddAssign + SubAssign
{
}

impl Offset for TextSize {}
impl Offset for Utf16 {}

/// How positions are counted.
pub(crate) trait Unit: Copy + Debug + PartialEq + Eq {
    /// A position in this unit.
    type Offset: Offset;
    /// A range of positions in this unit.
    type Range: Clone + Debug + PartialEq + Eq;
    /// The length of `text` in this unit.
    fn len(text: &str) -> Self::Offset;
    /// The range `start..end` (`start <= end`).
    fn range(start: Self::Offset, end: Self::Offset) -> Self::Range;
    /// Where `range` starts and ends.
    fn bounds(range: &Self::Range) -> (Self::Offset, Self::Offset);
}

/// Positions in bytes of UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bytes;

/// Positions in UTF-16 code units (the browser's offsets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeUnits;

impl Unit for Bytes {
    type Offset = TextSize;
    type Range = TextRange;
    fn len(text: &str) -> TextSize {
        TextSize::of(text)
    }
    fn range(start: TextSize, end: TextSize) -> TextRange {
        TextRange::new(start, end)
    }
    fn bounds(range: &TextRange) -> (TextSize, TextSize) {
        (range.start(), range.end())
    }
}

impl Unit for CodeUnits {
    type Offset = Utf16;
    type Range = Range<Utf16>;
    fn len(text: &str) -> Utf16 {
        Utf16::of(text)
    }
    fn range(start: Utf16, end: Utf16) -> Range<Utf16> {
        start..end
    }
    fn bounds(range: &Range<Utf16>) -> (Utf16, Utf16) {
        (range.start, range.end)
    }
}

/// Which side of text inserted right at a position the position keeps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Assoc {
    /// Before it: the end of a span stays before text typed at its end.
    Before,
    /// After it: the start of a span moves past text typed at its start.
    After,
}

/// A position mapped through a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mapped<O = TextSize> {
    pub(crate) pos: O,
    /// The text on both sides of it was deleted (it lay strictly inside a
    /// replaced range).
    pub(crate) deleted: bool,
}

/// One replacement: `delete` units at `at` of the text before replaced by
/// `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Replace<O> {
    at: O,
    delete: O,
    insert: String,
}

/// A stretch of the text before written whole at `to` of the text after
/// (its own place deleted): positions inside it go with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Moved<O = TextSize> {
    pub(crate) from: O,
    pub(crate) len: O,
    pub(crate) to: O,
}

/// The replacements an edit made to a text of `len` units (see the
/// module).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change<U: Unit = Bytes> {
    len: U::Offset,
    replaced: Vec<Replace<U::Offset>>,
    moved: Vec<Moved<U::Offset>>,
    unit: PhantomData<U>,
}

/// What the replacements before a place wrote and deleted: the place moves
/// by `grown - shrunk`, kept as two sums so no step goes below zero.
#[derive(Clone, Copy, Default)]
struct Shift<O> {
    grown: O,
    shrunk: O,
}

impl<O: Offset> Shift<O> {
    /// Where `at` of the text before (at or past every replacement summed)
    /// is in the text after.
    fn of(self, at: O) -> O {
        at + self.grown - self.shrunk
    }

    fn add(&mut self, inserted: O, deleted: O) {
        self.grown += inserted;
        self.shrunk += deleted;
    }
}

impl<U: Unit> Change<U> {
    /// The change replacing each of `edits` (ranges of a text of `len`
    /// units, sorted and apart) by its text; `None` when they do not fit.
    pub(crate) fn replacing(
        len: U::Offset,
        edits: impl IntoIterator<Item = (U::Range, String)>,
    ) -> Option<Self> {
        let mut replaced: Vec<Replace<U::Offset>> = Vec::new();
        let mut end = U::Offset::default();
        for (range, insert) in edits {
            let (start, range_end) = U::bounds(&range);
            if start < end || range_end < start || range_end > len {
                return None;
            }
            end = range_end;
            let delete = range_end - start;
            if delete == U::Offset::default() && insert.is_empty() {
                continue;
            }
            // Touching replacements are one.
            match replaced.last_mut() {
                Some(last) if last.at + last.delete == start => {
                    last.delete += delete;
                    last.insert.push_str(&insert);
                }
                _ => replaced.push(Replace {
                    at: start,
                    delete,
                    insert,
                }),
            }
        }
        Some(Change {
            len,
            replaced,
            moved: Vec::new(),
            unit: PhantomData,
        })
    }

    /// The same change, with `moved` (stretches of the text before written
    /// whole elsewhere): kept where each is deleted at its place and
    /// written at its destination.
    pub(crate) fn with_moved(mut self, moved: impl IntoIterator<Item = Moved<U::Offset>>) -> Self {
        let mut kept: Vec<Moved<U::Offset>> = moved
            .into_iter()
            .filter(|moved| {
                moved.len > U::Offset::default()
                    && self.deleted_whole(moved.from..moved.from + moved.len)
            })
            .collect();
        kept.sort_by_key(|moved| moved.from);
        self.moved = kept;
        self
    }

    /// Whether the change deletes all of `range` (of the text before).
    fn deleted_whole(&self, range: Range<U::Offset>) -> bool {
        self.replaced
            .iter()
            .any(|edit| edit.at <= range.start && range.end <= edit.at + edit.delete)
    }

    /// The length of the text after.
    pub(crate) fn len_after(&self) -> U::Offset {
        let mut shift = Shift::default();
        for edit in &self.replaced {
            shift.add(U::len(&edit.insert), edit.delete);
        }
        shift.of(self.len)
    }

    /// From the start of the first replacement to the end of the last, in
    /// the text before, and the same in the text after; `None` for a change
    /// that replaces nothing.
    pub(crate) fn hull(&self) -> Option<(U::Range, U::Range)> {
        let first = self.replaced.first()?;
        let last = self.replaced.last()?;
        let before = U::range(first.at, last.at + last.delete);
        let mut shift = Shift::default();
        for edit in &self.replaced[..self.replaced.len() - 1] {
            shift.add(U::len(&edit.insert), edit.delete);
        }
        let end = shift.of(last.at) + U::len(&last.insert);
        Some((before, U::range(first.at, end)))
    }

    /// The stretches of the text before the change moved whole, by where
    /// they were.
    pub(crate) fn moved(&self) -> &[Moved<U::Offset>] {
        &self.moved
    }

    /// The replacements, as ranges of the text before and their texts.
    pub(crate) fn edits(&self) -> impl Iterator<Item = (U::Range, &str)> {
        self.replaced.iter().map(|edit| {
            (
                U::range(edit.at, edit.at + edit.delete),
                edit.insert.as_str(),
            )
        })
    }

    /// Where `pos` (of the text before) is in the text after (see
    /// [`Assoc`]). A position inside a stretch the change moved goes with
    /// it.
    pub(crate) fn map_pos(&self, pos: U::Offset, assoc: Assoc) -> Mapped<U::Offset> {
        for moved in &self.moved {
            let inside = moved.from < pos && pos < moved.from + moved.len
                || (pos == moved.from && assoc == Assoc::After)
                || (pos == moved.from + moved.len && assoc == Assoc::Before);
            if inside {
                return Mapped {
                    pos: moved.to + (pos - moved.from),
                    deleted: false,
                };
            }
        }
        let mut shift = Shift::default();
        for edit in &self.replaced {
            let end = edit.at + edit.delete;
            let inserted = U::len(&edit.insert);
            let at = shift.of(edit.at);
            if pos < edit.at {
                break;
            }
            if pos <= end {
                let deleted = edit.at < pos && pos < end;
                // Its own end, past what was deleted: after the insertion.
                let past = pos == end && edit.delete > U::Offset::default();
                let pos = match assoc {
                    _ if past => at + inserted,
                    Assoc::Before => at,
                    Assoc::After => at + inserted,
                };
                return Mapped { pos, deleted };
            }
            shift.add(inserted, edit.delete);
        }
        Mapped {
            pos: shift.of(pos),
            deleted: false,
        }
    }

    /// `range` (of the text before) in the text after: its start keeps
    /// after text inserted at it, its end before; `None` when the change
    /// deleted text inside it or wrote inside it.
    pub(crate) fn map_range(&self, range: &U::Range) -> Option<U::Range> {
        let (from, to) = U::bounds(range);
        let range = from..to;
        let start = self.map_pos(range.start, Assoc::After);
        let end = self.map_pos(range.end, Assoc::Before);
        let none = U::Offset::default();
        let inside = self.replaced.iter().any(|edit| {
            let end = edit.at + edit.delete;
            (range.start < edit.at && edit.at < range.end)
                || (range.start < end && end < range.end)
                || (edit.at <= range.start && range.end <= end && edit.delete > none)
        });
        let moved = |at: U::Offset| {
            self.moved
                .iter()
                .any(|moved| moved.from <= at && at <= moved.from + moved.len)
        };
        let whole_moved = range.start < range.end
            && self
                .moved
                .iter()
                .any(|moved| moved.from <= range.start && range.end <= moved.from + moved.len);
        // A stretch moved whole keeps its length: its end maps past its
        // start.
        if whole_moved {
            return Some(U::range(start.pos, end.pos.max(start.pos)));
        }
        (!inside && !start.deleted && !end.deleted && !(moved(range.start) ^ moved(range.end)))
            .then(|| U::range(start.pos, end.pos.max(start.pos)))
    }
}

impl Change<Bytes> {
    /// The change replacing each of `edits` (byte ranges of a text of
    /// `len` bytes, sorted and apart) by its text; `None` when they do not
    /// fit.
    pub(crate) fn of_ranges(
        len: TextSize,
        edits: impl IntoIterator<Item = (TextRange, String)>,
    ) -> Option<Self> {
        Self::replacing(len, edits)
    }

    /// The change replacing UTF-16 `start..end` of `text` (the browser's
    /// offsets; past its end is its end) by `insert`.
    pub(crate) fn replace_units(text: &str, start: usize, end: usize, insert: &str) -> Self {
        let start = utf16_to_byte_index(text, start);
        let end = utf16_to_byte_index(text, end).max(start);
        Self::replace(
            text,
            TextRange::new(text_size(start), text_size(end)),
            insert,
        )
    }

    /// The change inserting `insert` at UTF-16 `at` of `text`.
    pub(crate) fn insert_units(text: &str, at: usize, insert: &str) -> Self {
        Self::replace_units(text, at, at, insert)
    }

    /// The change replacing `range` (bytes of `text`, on character
    /// boundaries) by `insert`; a range past the text is cut at its end.
    pub(crate) fn replace(text: &str, range: TextRange, insert: &str) -> Self {
        let end = range.end().min(TextSize::of(text));
        let start = range.start().min(end);
        Self::replacing(
            TextSize::of(text),
            [(TextRange::new(start, end), insert.to_string())],
        )
        .unwrap_or_else(|| Self::unchanged(text))
    }

    /// The change that changes nothing in `text`.
    pub(crate) fn unchanged(text: &str) -> Self {
        Change {
            len: TextSize::of(text),
            replaced: Vec::new(),
            moved: Vec::new(),
            unit: PhantomData,
        }
    }

    /// The change read off two texts: the one stretch between what they
    /// share at their start and at their end. Only for a text from outside,
    /// which says nothing of what changed: a host value, a reload, the
    /// host's change an input burst is replayed over.
    pub(crate) fn diffed(before: &str, after: &str) -> Self {
        let (start, end_before, end_after) = changed_bytes(before, after);
        Self::replace(
            before,
            TextRange::new(text_size(start), text_size(end_before)),
            &after[start..end_after],
        )
    }

    /// [`Self::diffed`] line by line: each run of lines the line diff finds
    /// the texts do not share one replacement, narrowed to what differs in
    /// it. For a text from outside that may differ in several places (the
    /// host's text a reload writes over unsaved edits): a position on a line
    /// both texts keep stays on its text. One stretch, as `diffed`, when the
    /// line diff gives up.
    pub(crate) fn diffed_by_lines(before: &str, after: &str) -> Self {
        let old: Vec<&str> = before.split_inclusive('\n').collect();
        let new: Vec<&str> = after.split_inclusive('\n').collect();
        let Some(script) = myers_edits(&old, &new) else {
            return Self::diffed(before, after);
        };
        let (mut old_lines, mut new_lines) = (old.iter(), new.iter());
        let mut edits: Vec<(TextRange, String)> = Vec::new();
        // The run of lines being replaced: where it starts, what it writes.
        let mut run: Option<(TextSize, String)> = None;
        let mut at = TextSize::default();
        let line_len =
            |line: Option<&&str>| line.map_or(TextSize::default(), |line| TextSize::of(*line));
        for step in script {
            match step {
                DiffEdit::Equal => {
                    if let Some((start, text)) = run.take() {
                        edits.push((TextRange::new(start, at), text));
                    }
                    at += line_len(old_lines.next());
                    new_lines.next();
                }
                DiffEdit::Delete => {
                    run.get_or_insert_with(|| (at, String::new()));
                    at += line_len(old_lines.next());
                }
                DiffEdit::Insert => {
                    let (_, text) = run.get_or_insert_with(|| (at, String::new()));
                    text.push_str(new_lines.next().copied().unwrap_or_default());
                }
            }
        }
        if let Some((start, text)) = run {
            edits.push((TextRange::new(start, at), text));
        }
        let lines = Self::replacing(TextSize::of(before), edits)
            .filter(|change| change.apply(before).as_deref() == Some(after));
        match lines {
            Some(change) => change.clone().narrowed(before).unwrap_or(change),
            None => Self::diffed(before, after),
        }
    }

    /// [`Self::diffed`], for browser input that did not say what it did:
    /// placed by the caret it left (byte `caret` of `after`), where text
    /// it inserted ends, which tells apart places the texts alone cannot
    /// (`aa|a` + `a`).
    pub(crate) fn diffed_at(before: &str, after: &str, caret: TextSize) -> Self {
        let (start, end) = edit_range(before, after, utf16_of(after, caret));
        let range = TextRange::new(
            text_size(utf16_to_byte_index(before, start)),
            text_size(utf16_to_byte_index(before, end)),
        );
        Self::replacing_selection(before, after, range)
            .unwrap_or_else(|| Self::diffed(before, after))
    }

    /// Whether it writes back what it replaces everywhere: `before` (the
    /// text it is made on) stays as it is.
    pub(crate) fn is_identity(&self, before: &str) -> bool {
        self.replaced.iter().all(|edit| {
            before.get(Range::<usize>::from(TextRange::at(edit.at, edit.delete)))
                == Some(edit.insert.as_str())
        })
    }

    /// Bytes it holds: the text it writes, and its pieces (the undo
    /// history's budget).
    pub(crate) fn held_bytes(&self) -> usize {
        self.replaced
            .iter()
            .map(|edit| edit.insert.len() + std::mem::size_of::<Replace<TextSize>>())
            .sum::<usize>()
            + self.moved.len() * std::mem::size_of::<Moved>()
    }

    /// The change made by replacing `selection` of `before` (bytes) such
    /// that the text becomes `after`, when `after` is that: the texts agree
    /// before it and after it.
    pub(crate) fn replacing_selection(
        before: &str,
        after: &str,
        selection: TextRange,
    ) -> Option<Self> {
        let (start, end) = (usize::from(selection.start()), usize::from(selection.end()));
        let kept = before.len().checked_sub(usize::from(selection.len()))?;
        let inserted_end = start + after.len().checked_sub(kept)?;
        let agree = before.get(..start)? == after.get(..start)?
            && before.get(end..)? == after.get(inserted_end..)?;
        agree
            .then(|| {
                Self::replacing(
                    TextSize::of(before),
                    [(selection, after[start..inserted_end].to_string())],
                )
            })
            .flatten()
    }

    /// The same change without the text each replacement writes back as
    /// it was at either of its ends (`before` is the text it is made on): a
    /// whole text pasted with one change is that change, a placeholder
    /// written over itself no change at all. A change moving stretches is
    /// kept as it is: its moves say where the text went.
    pub(crate) fn narrowed(self, before: &str) -> Option<Self> {
        if !self.moved.is_empty() {
            return Some(self);
        }
        let mut edits = Vec::with_capacity(self.replaced.len());
        for edit in &self.replaced {
            let removed = before.get(Range::<usize>::from(TextRange::at(edit.at, edit.delete)))?;
            let inserted = edit.insert.as_str();
            let mut prefix = removed
                .bytes()
                .zip(inserted.bytes())
                .take_while(|(left, right)| left == right)
                .count();
            while !removed.is_char_boundary(prefix) || !inserted.is_char_boundary(prefix) {
                prefix -= 1;
            }
            let (removed_rest, inserted_rest) = (&removed[prefix..], &inserted[prefix..]);
            let mut suffix = removed_rest
                .bytes()
                .rev()
                .zip(inserted_rest.bytes().rev())
                .take_while(|(left, right)| left == right)
                .count();
            while !removed_rest.is_char_boundary(removed_rest.len() - suffix)
                || !inserted_rest.is_char_boundary(inserted_rest.len() - suffix)
            {
                suffix -= 1;
            }
            let start = edit.at + text_size(prefix);
            edits.push((
                TextRange::at(start, text_size(removed_rest.len() - suffix)),
                inserted_rest[..inserted_rest.len() - suffix].to_string(),
            ));
        }
        Self::replacing(self.len, edits)
    }

    /// `text` with the change made; `None` when it is not the text the
    /// change was made for.
    pub(crate) fn apply(&self, text: &str) -> Option<String> {
        if TextSize::try_from(text.len()).ok()? != self.len {
            return None;
        }
        let mut out = String::with_capacity(usize::from(self.len_after()));
        let mut at = 0;
        for edit in &self.replaced {
            out.push_str(text.get(at..usize::from(edit.at))?);
            out.push_str(&edit.insert);
            at = usize::from(edit.at + edit.delete);
        }
        out.push_str(text.get(at..)?);
        Some(out)
    }

    /// The change undoing this one: made on the text after, giving back
    /// `before` (the text this one was made on).
    pub(crate) fn invert(&self, before: &str) -> Option<Self> {
        let mut shift = Shift::default();
        let mut edits = Vec::with_capacity(self.replaced.len());
        for edit in &self.replaced {
            let at = shift.of(edit.at);
            let inserted = TextSize::of(edit.insert.as_str());
            edits.push((
                TextRange::at(at, inserted),
                before
                    .get(Range::<usize>::from(TextRange::at(edit.at, edit.delete)))?
                    .to_string(),
            ));
            shift.add(inserted, edit.delete);
        }
        let moved: Vec<Moved> = self
            .moved
            .iter()
            .map(|moved| Moved {
                from: moved.to,
                len: moved.len,
                // Its place in the text before, which the inverse writes
                // back.
                to: moved.from,
            })
            .collect();
        Some(Self::replacing(self.len_after(), edits)?.with_moved(moved))
    }

    /// The change making `next` after this one, as one change of this
    /// one's text before. Moved stretches are not carried (a composed
    /// change maps positions by its replacements).
    pub(crate) fn compose(&self, next: &Self) -> Option<Self> {
        if next.len != self.len_after() {
            return None;
        }
        let mut out = Pieces::default();
        let mut first = self.pieces().into_iter();
        let mut second = next.pieces().into_iter();
        let mut a = first.next();
        let mut b = second.next();
        loop {
            match (a.take(), b.take()) {
                (None, None) => break,
                (Some(Piece::Delete(n)), other) => {
                    out.delete(n);
                    b = other;
                    a = first.next();
                }
                (other, Some(Piece::Insert(text))) => {
                    out.insert(&text);
                    a = other;
                    b = second.next();
                }
                (Some(left), Some(right)) => {
                    let len = left.len().min(right.len());
                    let (head, rest_left) = left.split(len);
                    let (_, rest_right) = right.clone().split(len);
                    match (&head, &right) {
                        (Piece::Retain(_), Piece::Retain(_)) => out.retain(len),
                        (Piece::Retain(_), Piece::Delete(_)) => out.delete(len),
                        (Piece::Insert(text), Piece::Retain(_)) => out.insert(text),
                        _ => {}
                    }
                    a = rest_left.or_else(|| first.next());
                    b = rest_right.or_else(|| second.next());
                }
                // A change shorter than its text: not made for it.
                (Some(_), None) | (None, Some(_)) => return None,
            }
        }
        out.into_change(self.len)
    }

    /// This change made after `other` (both made on the same text), as a
    /// change of the text `other` makes: what `other` deleted is no more
    /// to replace, and of two insertions at one place, this one's goes
    /// first when `first`.
    pub(crate) fn map(&self, other: &Self, first: bool) -> Option<Self> {
        if other.len != self.len {
            return None;
        }
        let mut out = Pieces::default();
        let mut mine = self.pieces().into_iter();
        let mut theirs = other.pieces().into_iter();
        let mut a = mine.next();
        let mut b = theirs.next();
        loop {
            match (a.take(), b.take()) {
                (None, None) => break,
                (Some(Piece::Insert(text)), Some(Piece::Insert(their))) => {
                    if first {
                        out.insert(&text);
                        a = mine.next();
                        b = Some(Piece::Insert(their));
                    } else {
                        out.retain(TextSize::of(their.as_str()));
                        a = Some(Piece::Insert(text));
                        b = theirs.next();
                    }
                }
                (Some(Piece::Insert(text)), other) => {
                    out.insert(&text);
                    a = mine.next();
                    b = other;
                }
                (other, Some(Piece::Insert(their))) => {
                    out.retain(TextSize::of(their.as_str()));
                    a = other;
                    b = theirs.next();
                }
                (Some(left), Some(right)) => {
                    let len = left.len().min(right.len());
                    let (head, rest_left) = left.split(len);
                    let (_, rest_right) = right.clone().split(len);
                    match (&head, &right) {
                        (Piece::Retain(_), Piece::Retain(_)) => out.retain(len),
                        (Piece::Delete(_), Piece::Retain(_)) => out.delete(len),
                        _ => {}
                    }
                    a = rest_left.or_else(|| mine.next());
                    b = rest_right.or_else(|| theirs.next());
                }
                (Some(_), None) | (None, Some(_)) => return None,
            }
        }
        out.into_change(other.len_after())
    }

    /// The same change counted in UTF-16 code units of `before` (the text
    /// it is made on).
    pub(crate) fn in_utf16(&self, before: &str) -> Option<Change<CodeUnits>> {
        let mut units = Utf16::default();
        let mut at = 0;
        let mut to_units = |byte: TextSize| -> Option<Utf16> {
            let byte = usize::from(byte);
            units += Utf16::of(before.get(at..byte)?);
            at = byte;
            Some(units)
        };
        let mut replaced = Vec::with_capacity(self.replaced.len());
        for edit in &self.replaced {
            let start = to_units(edit.at)?;
            let end = to_units(edit.at + edit.delete)?;
            replaced.push(Replace {
                at: start,
                delete: end - start,
                insert: edit.insert.clone(),
            });
        }
        let len = to_units(TextSize::of(before))?;
        let after = self.apply(before)?;
        let moved = self
            .moved
            .iter()
            .map(|moved| {
                let from = Utf16::of_byte(before, moved.from)?;
                Some(Moved {
                    from,
                    len: Utf16::of_byte(before, moved.from + moved.len)? - from,
                    to: Utf16::of_byte(&after, moved.to)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Change {
            len,
            replaced,
            moved,
            unit: PhantomData,
        })
    }

    /// The change as retain, delete and insert pieces over the text.
    fn pieces(&self) -> Vec<Piece> {
        let mut pieces = Vec::with_capacity(self.replaced.len() * 3 + 1);
        let mut at = TextSize::default();
        for edit in &self.replaced {
            if edit.at > at {
                pieces.push(Piece::Retain(edit.at - at));
            }
            if edit.delete > TextSize::default() {
                pieces.push(Piece::Delete(edit.delete));
            }
            if !edit.insert.is_empty() {
                pieces.push(Piece::Insert(edit.insert.clone()));
            }
            at = edit.at + edit.delete;
        }
        if self.len > at {
            pieces.push(Piece::Retain(self.len - at));
        }
        pieces
    }
}

/// A piece of a change, read over the text before from its start.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Retain(TextSize),
    Delete(TextSize),
    Insert(String),
}

impl Piece {
    /// How much of the text the piece reads (an insertion: what it writes).
    fn len(&self) -> TextSize {
        match self {
            Piece::Retain(len) | Piece::Delete(len) => *len,
            Piece::Insert(text) => TextSize::of(text.as_str()),
        }
    }

    /// The first `len` of it, and the rest when any is left.
    fn split(self, len: TextSize) -> (Piece, Option<Piece>) {
        match self {
            Piece::Retain(all) => (
                Piece::Retain(len),
                (all > len).then(|| Piece::Retain(all - len)),
            ),
            Piece::Delete(all) => (
                Piece::Delete(len),
                (all > len).then(|| Piece::Delete(all - len)),
            ),
            Piece::Insert(text) => {
                let rest = text[usize::from(len)..].to_string();
                (
                    Piece::Insert(text[..usize::from(len)].to_string()),
                    (!rest.is_empty()).then_some(Piece::Insert(rest)),
                )
            }
        }
    }
}

/// Pieces gathered into a change.
#[derive(Default)]
struct Pieces {
    at: TextSize,
    edits: Vec<(TextRange, String)>,
}

impl Pieces {
    fn retain(&mut self, len: TextSize) {
        self.at += len;
    }

    fn delete(&mut self, len: TextSize) {
        self.edit(len, "");
    }

    fn insert(&mut self, text: &str) {
        self.edit(TextSize::default(), text);
    }

    fn edit(&mut self, delete: TextSize, insert: &str) {
        match self.edits.last_mut() {
            Some((range, text)) if range.end() == self.at => {
                *range = TextRange::new(range.start(), range.end() + delete);
                text.push_str(insert);
            }
            _ => self
                .edits
                .push((TextRange::at(self.at, delete), insert.to_string())),
        }
        self.at += delete;
    }

    fn into_change(self, len: TextSize) -> Option<Change<Bytes>> {
        Change::replacing(len, self.edits)
    }
}
