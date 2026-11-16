//! What an edit did, as the links follow it: the pieces of the text before
//! it replaced, each with the text written there and where that text came
//! from ([`Inserted`]). The pieces are the edit's own change
//! ([`Edit::made`]): said by the code making it, or read off the two texts
//! at the edge that made it (input the browser did not say enough of).

use super::super::super::edit::{Edit, Moved, Origin};
use super::super::super::text::{TextRange, TextSize, slice, text_size};
use super::Clipboard;

/// `range` of the text before (bytes), replaced by `inserted`: an edit's
/// record, as the command making it gives it ([`Edit::splicing`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) range: TextRange,
    pub(crate) inserted: String,
    /// Text of the text before written again in `inserted` (moved or
    /// copied), each stretch's `to` from the start of `inserted`: its
    /// links go with it.
    pub(crate) moved: Vec<Moved>,
}

/// Where each line of `text` starts, and its end.
fn starts(text: &str) -> Vec<TextSize> {
    std::iter::once(TextSize::default())
        .chain(text.match_indices('\n').map(|(at, _)| text_size(at + 1)))
        .chain(std::iter::once(TextSize::of(text) + TextSize::from(1)))
        .collect()
}

impl Splice {
    /// `range` replaced by `inserted`.
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of some builds only")]
    pub(crate) fn replace(range: TextRange, inserted: impl Into<String>) -> Self {
        Splice {
            range,
            inserted: inserted.into(),
            moved: Vec::new(),
        }
    }

    /// The splices of a command that knows each range it replaced (byte
    /// ranges of the text before as the tests find them, sorted and
    /// apart): an indent's lines, a Replace all's matches.
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of some builds only")]
    pub(crate) fn each(
        edits: impl IntoIterator<Item = (std::ops::Range<usize>, String)>,
    ) -> Vec<Splice> {
        edits
            .into_iter()
            .filter(|(range, inserted)| !range.is_empty() || !inserted.is_empty())
            .map(|(range, inserted)| {
                Splice::replace(super::super::super::text::byte_range(range), inserted)
            })
            .collect()
    }

    /// `before` with `splices` (sorted, apart) made; `None` when they do
    /// not fit it.
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of some builds only")]
    pub(crate) fn make(before: &str, splices: &[Splice]) -> Option<String> {
        let mut out = String::with_capacity(before.len());
        let mut at = TextSize::default();
        for splice in splices {
            out.push_str(slice::slice(
                before,
                TextRange::new(at, splice.range.start()),
            )?);
            out.push_str(&splice.inserted);
            at = splice.range.end();
        }
        out.push_str(slice::after(before, at)?);
        Some(out)
    }

    /// A line command's edit: `pieces` (the first line in `before`, how
    /// many lines, the first line in `after`) are the lines it moved or
    /// copied; the lines from the first piece's to the last are rewritten.
    pub(crate) fn lines(
        before: &str,
        after: &str,
        pieces: &[(usize, usize, usize)],
    ) -> Option<Splice> {
        let (old, new) = (starts(before), starts(after));
        let first = pieces.iter().map(|(from, _, to)| (*from).min(*to)).min()?;
        let old_end = pieces.iter().map(|(from, count, _)| from + count).max()?;
        let new_end = pieces.iter().map(|(_, count, to)| to + count).max()?;
        let byte = |starts: &[TextSize], text: &str, line: usize| {
            starts.get(line).map(|at| (*at).min(TextSize::of(text)))
        };
        let (at, end) = (byte(&old, before, first)?, byte(&old, before, old_end)?);
        let (from, to) = (byte(&new, after, first)?, byte(&new, after, new_end)?);
        let moved = pieces
            .iter()
            .filter_map(|&(line, count, place)| {
                let start = byte(&old, before, line)?;
                // To the end of its last line's text.
                let end = old
                    .get(line + count)?
                    .checked_sub(TextSize::from(1))
                    .unwrap_or_default()
                    .min(TextSize::of(before));
                Some(Moved {
                    from: start,
                    len: end.checked_sub(start)?,
                    to: byte(&new, after, place)?.checked_sub(from)?,
                })
            })
            .collect();
        if end < at || to < from {
            return None;
        }
        Some(Splice {
            range: TextRange::new(at, end),
            inserted: slice::slice(after, TextRange::new(from, to))?.to_string(),
            moved,
        })
    }
}

/// Where a piece's inserted text came from, which says which links it
/// takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Inserted {
    /// Typed by the user (a key, IME, dictation, a deletion): no item's.
    Typed,
    /// Written by a command (a suggestion, Enter's indent, a typed closer,
    /// Replace): no item's.
    Generated,
    /// Text of the text before, moved or copied by the edit (a line
    /// command), each stretch's `to` from the start of the piece's
    /// inserted text: the links of that text go with it.
    Moved(Vec<Moved>),
    /// The editor's own record of copied or removed text, pasted or
    /// dropped: the record's links.
    Pasted,
    /// Pasted or dropped from outside the editor: new items.
    External,
    /// The removed text written again, its lines indented otherwise (a
    /// selection pasted over at another indent): its links.
    Reindented,
}

/// One piece of an edit: `range` of the text before (bytes), replaced by
/// `inserted`, which came from `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub(crate) range: TextRange,
    pub(crate) inserted: String,
    pub(crate) source: Inserted,
}

impl Piece {
    /// The length of what it writes.
    pub(crate) fn inserted_len(&self) -> TextSize {
        TextSize::of(self.inserted.as_str())
    }
}

/// Whether `edit` puts text from a clipboard or a drag (the browser's
/// paste or drop, or a command putting a copied line back).
fn pastes(edit: &Edit) -> bool {
    match &edit.origin {
        Origin::Input { kind, .. } => {
            matches!(kind.as_str(), "insertFromPaste" | "insertFromDrop")
        }
        Origin::Command => true,
        _ => false,
    }
}

/// What `edit` (an edit of the user's: a command or input) did to its
/// text before, as pieces with their sources. A paste is matched against
/// the editor's own records of copied text (`clipboard`): the browser
/// gives only the text, which is the one comparison it allows. A piece
/// that takes no links is narrowed to what it changes (a whole text
/// pasted with a few changes keeps the items of the lines it writes as
/// they were).
pub(crate) fn pieces(edit: &Edit, clipboard: &Clipboard) -> Vec<Piece> {
    let before = edit.before.as_str();
    let change = edit.made();
    let moved = change.moved();
    let pasting = pastes(edit);
    let typed = matches!(edit.origin, Origin::Input { .. }) && !pasting;
    // Where each replacement's text lands in the text after: what the ones
    // before it wrote, less what they removed (kept as two sums, so no
    // step goes below zero).
    let (mut grown, mut shrunk) = (TextSize::default(), TextSize::default());
    let mut out = Vec::new();
    for (range, inserted) in change.edits() {
        let Some(at_after) = (range.start() + grown).checked_sub(shrunk) else {
            continue;
        };
        let written = TextRange::at(at_after, TextSize::of(inserted));
        grown += TextSize::of(inserted);
        shrunk += range.len();
        let carried: Vec<Moved> = moved
            .iter()
            .filter(|stretch| written.contains(stretch.to))
            .map(|stretch| Moved {
                to: stretch.to - at_after,
                ..*stretch
            })
            .collect();
        // The change's ranges are of `before`, on its characters.
        let removed = slice::slice(before, range).unwrap_or_default();
        let source = if !carried.is_empty() {
            Inserted::Moved(carried)
        } else if pasting && clipboard.holds(inserted) {
            Inserted::Pasted
        } else if !range.is_empty() && super::clipboard::same_shape(removed, inserted) {
            Inserted::Reindented
        } else if pasting && matches!(edit.origin, Origin::Input { .. }) {
            Inserted::External
        } else if typed {
            Inserted::Typed
        } else {
            Inserted::Generated
        };
        let piece = Piece {
            range,
            inserted: inserted.to_string(),
            source,
        };
        let piece = match piece.source {
            Inserted::Typed | Inserted::Generated | Inserted::External => piece.narrowed(before),
            _ => piece,
        };
        if !piece.range.is_empty() || !piece.inserted.is_empty() {
            out.push(piece);
        }
    }
    out
}

impl Piece {
    /// The same piece of `before` without the text it writes back as it
    /// was at either end.
    fn narrowed(self, before: &str) -> Piece {
        let Some(removed) = slice::slice(before, self.range) else {
            return self;
        };
        let inserted = self.inserted.as_str();
        let mut prefix = removed
            .bytes()
            .zip(inserted.bytes())
            .take_while(|(left, right)| left == right)
            .count();
        while !removed.is_char_boundary(prefix) || !inserted.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let prefix = text_size(prefix);
        let (Some(removed_rest), Some(inserted_rest)) = (
            slice::after(removed, prefix),
            slice::after(inserted, prefix),
        ) else {
            return self;
        };
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
        let suffix = text_size(suffix);
        let kept = TextSize::of(inserted_rest) - suffix;
        Piece {
            range: TextRange::at(
                self.range.start() + prefix,
                TextSize::of(removed_rest) - suffix,
            ),
            inserted: slice::before(inserted_rest, kept)
                .unwrap_or_default()
                .to_string(),
            source: self.source,
        }
    }
}
