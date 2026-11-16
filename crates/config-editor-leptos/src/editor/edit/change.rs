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
//! Positions are counted in a [`Unit`]: bytes for the text itself, UTF-16
//! code units for what follows the browser's offsets (the protected
//! spans); [`Change::in_utf16`] converts one to the other.
//!
//! - [`Change::map_pos`] maps a position, with an [`Assoc`] saying which
//!   side of an insertion made right at it the position keeps to, and
//!   whether the text around it was deleted;
//! - [`Change::invert`] undoes it, [`Change::compose`] makes one change of
//!   two in turn, [`Change::map`] rebases one change over another made on
//!   the same text (`a.compose(&b.map(&a, false))` reads as
//!   `b.compose(&a.map(&b, true))`).

use std::marker::PhantomData;
use std::ops::Range;

use super::super::text::{TextRange, TextSize};

/// How positions are counted.
pub(crate) trait Unit: Copy + std::fmt::Debug + PartialEq + Eq {
    /// The length of `text` in this unit.
    fn len(text: &str) -> usize;
}

/// Positions in bytes of UTF-8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bytes;

/// Positions in UTF-16 code units (the browser's offsets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeUnits;

impl Unit for Bytes {
    fn len(text: &str) -> usize {
        text.len()
    }
}

impl Unit for CodeUnits {
    fn len(text: &str) -> usize {
        text.encode_utf16().count()
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
pub(crate) struct Mapped {
    pub(crate) pos: usize,
    /// The text on both sides of it was deleted (it lay strictly inside a
    /// replaced range).
    pub(crate) deleted: bool,
}

/// One replacement: `delete` units at `at` of the text before replaced by
/// `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Replace {
    at: usize,
    delete: usize,
    insert: String,
}

/// A stretch of the text before written whole at `to` of the text after
/// (its own place deleted): positions inside it go with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Moved {
    pub(crate) from: usize,
    pub(crate) len: usize,
    pub(crate) to: usize,
}

/// The replacements an edit made to a text of `len` units (see the
/// module).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change<U: Unit = Bytes> {
    len: usize,
    replaced: Vec<Replace>,
    moved: Vec<Moved>,
    unit: PhantomData<U>,
}

impl<U: Unit> Change<U> {
    /// The change replacing each of `edits` (ranges of a text of `len`
    /// units, sorted and apart) by its text; `None` when they do not fit.
    pub(crate) fn replacing(
        len: usize,
        edits: impl IntoIterator<Item = (Range<usize>, String)>,
    ) -> Option<Self> {
        let mut replaced: Vec<Replace> = Vec::new();
        let mut end = 0;
        for (range, insert) in edits {
            if range.start < end || range.end < range.start || range.end > len {
                return None;
            }
            end = range.end;
            if range.is_empty() && insert.is_empty() {
                continue;
            }
            // Touching replacements are one.
            match replaced.last_mut() {
                Some(last) if last.at + last.delete == range.start => {
                    last.delete += range.len();
                    last.insert.push_str(&insert);
                }
                _ => replaced.push(Replace {
                    at: range.start,
                    delete: range.len(),
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
    pub(crate) fn with_moved(mut self, moved: impl IntoIterator<Item = Moved>) -> Self {
        let mut kept: Vec<Moved> = moved
            .into_iter()
            .filter(|moved| moved.len > 0 && self.deleted_whole(moved.from..moved.from + moved.len))
            .collect();
        kept.sort_by_key(|moved| moved.from);
        self.moved = kept;
        self
    }

    /// Whether the change deletes all of `range` (of the text before).
    fn deleted_whole(&self, range: Range<usize>) -> bool {
        self.replaced
            .iter()
            .any(|edit| edit.at <= range.start && range.end <= edit.at + edit.delete)
    }

    /// The length of the text after.
    pub(crate) fn len_after(&self) -> usize {
        self.replaced.iter().fold(self.len, |len, edit| {
            len + U::len(&edit.insert) - edit.delete
        })
    }

    /// From the start of the first replacement to the end of the last, in
    /// the text before, and the same in the text after; `None` for a change
    /// that replaces nothing.
    pub(crate) fn hull(&self) -> Option<(Range<usize>, Range<usize>)> {
        let first = self.replaced.first()?;
        let last = self.replaced.last()?;
        let before = first.at..last.at + last.delete;
        let shift = |edit: &Replace| U::len(&edit.insert) as isize - edit.delete as isize;
        let until_last: isize = self.replaced[..self.replaced.len() - 1]
            .iter()
            .map(shift)
            .sum();
        let start = first.at;
        let end = (last.at as isize + until_last) as usize + U::len(&last.insert);
        Some((before, start..end))
    }

    /// The replacements, as ranges of the text before and their texts.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the tests of the change")
    )]
    pub(crate) fn edits(&self) -> impl Iterator<Item = (Range<usize>, &str)> {
        self.replaced
            .iter()
            .map(|edit| (edit.at..edit.at + edit.delete, edit.insert.as_str()))
    }

    /// Where `pos` (of the text before) is in the text after (see
    /// [`Assoc`]). A position inside a stretch the change moved goes with
    /// it.
    pub(crate) fn map_pos(&self, pos: usize, assoc: Assoc) -> Mapped {
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
        let mut shift: isize = 0;
        for edit in &self.replaced {
            let end = edit.at + edit.delete;
            let inserted = U::len(&edit.insert);
            let at = (edit.at as isize + shift) as usize;
            if pos < edit.at {
                break;
            }
            if pos <= end {
                let deleted = edit.at < pos && pos < end;
                // Its own end, past what was deleted: after the insertion.
                let past = pos == end && edit.delete > 0;
                let pos = match assoc {
                    _ if past => at + inserted,
                    Assoc::Before => at,
                    Assoc::After => at + inserted,
                };
                return Mapped { pos, deleted };
            }
            shift += inserted as isize - edit.delete as isize;
        }
        Mapped {
            pos: (pos as isize + shift) as usize,
            deleted: false,
        }
    }

    /// `range` (of the text before) in the text after: its start keeps
    /// after text inserted at it, its end before; `None` when the change
    /// deleted text inside it or wrote inside it.
    pub(crate) fn map_range(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let start = self.map_pos(range.start, Assoc::After);
        let end = self.map_pos(range.end, Assoc::Before);
        let inside = self.replaced.iter().any(|edit| {
            let end = edit.at + edit.delete;
            (range.start < edit.at && edit.at < range.end)
                || (range.start < end && end < range.end)
                || (edit.at <= range.start && range.end <= end && edit.delete > 0)
        });
        let moved = |at: usize| {
            self.moved
                .iter()
                .any(|moved| moved.from <= at && at <= moved.from + moved.len)
        };
        let whole_moved = !range.is_empty()
            && self
                .moved
                .iter()
                .any(|moved| moved.from <= range.start && range.end <= moved.from + moved.len);
        if whole_moved {
            return Some(start.pos..end.pos);
        }
        (!inside && !start.deleted && !end.deleted && !(moved(range.start) ^ moved(range.end)))
            .then_some(start.pos..end.pos.max(start.pos))
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
        Self::replacing(
            usize::from(len),
            edits
                .into_iter()
                .map(|(range, text)| (usize::from(range.start())..usize::from(range.end()), text)),
        )
    }

    /// Where byte `at` of the text before is in the text after.
    #[cfg_attr(not(test), expect(dead_code, reason = "byte positions move onto it"))]
    pub(crate) fn map_offset(&self, at: TextSize, assoc: Assoc) -> TextSize {
        let mapped = self.map_pos(usize::from(at), assoc).pos;
        TextSize::try_from(mapped).unwrap_or(at)
    }

    /// The change made by replacing `selection` of `before` (bytes) such
    /// that the text becomes `after`, when `after` is that: the texts agree
    /// before it and after it.
    pub(crate) fn replacing_selection(
        before: &str,
        after: &str,
        selection: Range<usize>,
    ) -> Option<Self> {
        let kept = before.len().checked_sub(selection.len())?;
        let inserted_end = selection.start + after.len().checked_sub(kept)?;
        let agree = before.get(..selection.start)? == after.get(..selection.start)?
            && before.get(selection.end..)? == after.get(inserted_end..)?;
        agree
            .then(|| {
                Self::replacing(
                    before.len(),
                    [(
                        selection.clone(),
                        after[selection.start..inserted_end].to_string(),
                    )],
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
            let removed = before.get(edit.at..edit.at + edit.delete)?;
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
            let start = edit.at + prefix;
            edits.push((
                start..start + removed_rest.len() - suffix,
                inserted_rest[..inserted_rest.len() - suffix].to_string(),
            ));
        }
        Self::replacing(self.len, edits)
    }

    /// `text` with the change made; `None` when it is not the text the
    /// change was made for.
    pub(crate) fn apply(&self, text: &str) -> Option<String> {
        if text.len() != self.len {
            return None;
        }
        let mut out = String::with_capacity(self.len_after());
        let mut at = 0;
        for edit in &self.replaced {
            out.push_str(text.get(at..edit.at)?);
            out.push_str(&edit.insert);
            at = edit.at + edit.delete;
        }
        out.push_str(text.get(at..)?);
        Some(out)
    }

    /// The change undoing this one: made on the text after, giving back
    /// `before` (the text this one was made on).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "undo steps that keep changes build on it")
    )]
    pub(crate) fn invert(&self, before: &str) -> Option<Self> {
        let mut shift: isize = 0;
        let mut edits = Vec::with_capacity(self.replaced.len());
        for edit in &self.replaced {
            let at = (edit.at as isize + shift) as usize;
            edits.push((
                at..at + edit.insert.len(),
                before.get(edit.at..edit.at + edit.delete)?.to_string(),
            ));
            shift += edit.insert.len() as isize - edit.delete as isize;
        }
        let moved: Vec<Moved> = self
            .moved
            .iter()
            .map(|moved| Moved {
                from: moved.to,
                len: moved.len,
                to: self.unmoved_place(moved),
            })
            .collect();
        Some(Self::replacing(self.len_after(), edits)?.with_moved(moved))
    }

    /// Where `moved`'s source is in the text the inverse makes: its place
    /// in the text before, which the inverse writes back.
    fn unmoved_place(&self, moved: &Moved) -> usize {
        moved.from
    }

    /// The change making `next` after this one, as one change of this
    /// one's text before. Moved stretches are not carried (a composed
    /// change maps positions by its replacements).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "undo steps that keep changes build on it")
    )]
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "a host value rebases unconfirmed changes with it")
    )]
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
                        out.retain(their.len());
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
                    out.retain(their.len());
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
        let mut units = 0;
        let mut at = 0;
        let mut to_units = |byte: usize| -> Option<usize> {
            units += before.get(at..byte)?.encode_utf16().count();
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
        let len = to_units(before.len())?;
        let after = self.apply(before)?;
        let units_of =
            |text: &str, byte: usize| text.get(..byte).map(|head| head.encode_utf16().count());
        let moved = self
            .moved
            .iter()
            .map(|moved| {
                let from = units_of(before, moved.from)?;
                Some(Moved {
                    from,
                    len: units_of(before, moved.from + moved.len)? - from,
                    to: units_of(&after, moved.to)?,
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
    #[cfg_attr(not(test), expect(dead_code, reason = "read by `compose` and `map`"))]
    fn pieces(&self) -> Vec<Piece> {
        let mut pieces = Vec::with_capacity(self.replaced.len() * 3 + 1);
        let mut at = 0;
        for edit in &self.replaced {
            if edit.at > at {
                pieces.push(Piece::Retain(edit.at - at));
            }
            if edit.delete > 0 {
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
#[cfg_attr(not(test), expect(dead_code, reason = "read by `compose` and `map`"))]
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Retain(usize),
    Delete(usize),
    Insert(String),
}

#[cfg_attr(not(test), expect(dead_code, reason = "read by `compose` and `map`"))]
impl Piece {
    /// How much of the text the piece reads (an insertion: what it writes).
    fn len(&self) -> usize {
        match self {
            Piece::Retain(len) | Piece::Delete(len) => *len,
            Piece::Insert(text) => text.len(),
        }
    }

    /// The first `len` of it, and the rest when any is left.
    fn split(self, len: usize) -> (Piece, Option<Piece>) {
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
                let rest = text[len..].to_string();
                (
                    Piece::Insert(text[..len].to_string()),
                    (!rest.is_empty()).then_some(Piece::Insert(rest)),
                )
            }
        }
    }
}

/// Pieces gathered into a change.
#[cfg_attr(not(test), expect(dead_code, reason = "made by `compose` and `map`"))]
#[derive(Default)]
struct Pieces {
    at: usize,
    edits: Vec<(Range<usize>, String)>,
}

#[cfg_attr(not(test), expect(dead_code, reason = "made by `compose` and `map`"))]
impl Pieces {
    fn retain(&mut self, len: usize) {
        self.at += len;
    }

    fn delete(&mut self, len: usize) {
        self.edit(len, "");
    }

    fn insert(&mut self, text: &str) {
        self.edit(0, text);
    }

    fn edit(&mut self, delete: usize, insert: &str) {
        match self.edits.last_mut() {
            Some((range, text)) if range.end == self.at => {
                range.end += delete;
                text.push_str(insert);
            }
            _ => self
                .edits
                .push((self.at..self.at + delete, insert.to_string())),
        }
        self.at += delete;
    }

    fn into_change(self, len: usize) -> Option<Change<Bytes>> {
        Change::replacing(len, self.edits)
    }
}
