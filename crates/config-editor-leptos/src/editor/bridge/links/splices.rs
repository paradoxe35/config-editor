//! What an edit did, as the links follow it: the pieces of the text before
//! it replaced, each with the text written there and where that text came
//! from ([`Inserted`]). The pieces are the edit's own record
//! ([`Edit::change`]); an edit with none is read off its two texts at the
//! caret it left ([`unrecorded`]), the one place the links compare texts.

use super::super::super::edit::{Edit, Origin};
use super::super::super::text_change::edit_range;
use super::Clipboard;

/// `removed` bytes at byte `at` of the text before, replaced by
/// `inserted`: an edit's record, as the command making it gives it
/// ([`Edit::splicing`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(crate) at: usize,
    pub(crate) removed: usize,
    pub(crate) inserted: String,
    /// Text of the text before written again in `inserted` (moved or
    /// copied): its links go with it.
    pub(crate) moved: Vec<Moved>,
}

/// `len` bytes at `from` of the text before, written at `to` of a
/// splice's inserted text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Moved {
    pub(crate) from: usize,
    pub(crate) len: usize,
    pub(crate) to: usize,
}

/// Where each line of `text` starts, and its end.
fn starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .chain(std::iter::once(text.len() + 1))
        .collect()
}

impl Splice {
    /// `at..at + removed` replaced by `inserted`.
    pub(crate) fn replace(at: usize, removed: usize, inserted: impl Into<String>) -> Self {
        Splice {
            at,
            removed,
            inserted: inserted.into(),
            moved: Vec::new(),
        }
    }

    /// The splices of a command that knows each range it replaced (byte
    /// ranges of the text before, sorted and apart): an indent's lines, a
    /// Replace all's matches.
    pub(crate) fn each(
        edits: impl IntoIterator<Item = (std::ops::Range<usize>, String)>,
    ) -> Vec<Splice> {
        edits
            .into_iter()
            .filter(|(range, inserted)| !range.is_empty() || !inserted.is_empty())
            .map(|(range, inserted)| Splice::replace(range.start, range.len(), inserted))
            .collect()
    }

    /// `before` with `splices` (sorted, apart) made; `None` when they do
    /// not fit it.
    pub(crate) fn make(before: &str, splices: &[Splice]) -> Option<String> {
        let mut out = String::with_capacity(before.len());
        let mut at = 0;
        for splice in splices {
            out.push_str(before.get(at..splice.at)?);
            out.push_str(&splice.inserted);
            at = splice.at.checked_add(splice.removed)?;
        }
        out.push_str(before.get(at..)?);
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
        let byte = |starts: &[usize], text: &str, line: usize| {
            starts.get(line).map(|at| (*at).min(text.len()))
        };
        let (at, end) = (byte(&old, before, first)?, byte(&old, before, old_end)?);
        let (from, to) = (byte(&new, after, first)?, byte(&new, after, new_end)?);
        let moved = pieces
            .iter()
            .filter_map(|&(line, count, place)| {
                let start = byte(&old, before, line)?;
                // To the end of its last line's text.
                let end = old.get(line + count)?.saturating_sub(1).min(before.len());
                Some(Moved {
                    from: start,
                    len: end.checked_sub(start)?,
                    to: byte(&new, after, place)?.checked_sub(from)?,
                })
            })
            .collect();
        Some(Splice {
            at,
            removed: end - at,
            inserted: after.get(from..to)?.to_string(),
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
    /// command): the links of that text go with it.
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

/// One piece of an edit: `removed` bytes at `at` of the text before,
/// replaced by `inserted`, which came from `source`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub(crate) at: usize,
    pub(crate) removed: usize,
    pub(crate) inserted: String,
    pub(crate) source: Inserted,
}

impl Piece {
    /// The end of what it removes.
    pub(crate) fn end(&self) -> usize {
        self.at + self.removed
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
    debug_assert!(
        edit.splices.as_ref().is_none_or(
            |splices| Splice::make(before, splices).as_deref() == Some(edit.text.as_str())
        ),
        "an edit's splices do not make its text"
    );
    let Some(change) = edit.recorded().or_else(|| unrecorded(edit)) else {
        return Vec::new();
    };
    let moved = change.moved();
    let pasting = pastes(edit);
    let typed = matches!(edit.origin, Origin::Input { .. }) && !pasting;
    let mut shift: isize = 0;
    let mut out = Vec::new();
    for (range, inserted) in change.edits() {
        let at_after = range.start.wrapping_add_signed(shift);
        shift += inserted.len() as isize - range.len() as isize;
        let carried: Vec<Moved> = moved
            .iter()
            .filter(|stretch| at_after <= stretch.to && stretch.to < at_after + inserted.len())
            .map(|stretch| Moved {
                from: stretch.from,
                len: stretch.len,
                to: stretch.to - at_after,
            })
            .collect();
        let removed = &before[range.clone()];
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
            at: range.start,
            removed: range.len(),
            inserted: inserted.to_string(),
            source,
        };
        let piece = match piece.source {
            Inserted::Typed | Inserted::Generated | Inserted::External => piece.narrowed(before),
            _ => piece,
        };
        if piece.removed > 0 || !piece.inserted.is_empty() {
            out.push(piece);
        }
    }
    out
}

impl Piece {
    /// The same piece of `before` without the text it writes back as it
    /// was at either end.
    fn narrowed(self, before: &str) -> Piece {
        let removed = &before[self.at..self.end()];
        let inserted = self.inserted.as_str();
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
        Piece {
            at: self.at + prefix,
            removed: removed_rest.len() - suffix,
            inserted: inserted_rest[..inserted_rest.len() - suffix].to_string(),
            source: self.source,
        }
    }
}

/// The change of an edit that recorded none (see [`Edit::change`]): its
/// two texts' one differing stretch, placed by the caret it left. Edits
/// with no record are an undo or redo (whose links come from its step,
/// not from here), input the browser gave no selection for, and commands
/// that rewrite text around the caret without saying how (a comment
/// toggle, a suggestion rewriting its line, Revert, joined lines).
fn unrecorded(edit: &Edit) -> Option<super::super::super::edit::Change> {
    let (before, after) = (edit.before.as_str(), edit.text.as_str());
    let (start, end) = edit_range(before, after, edit.caret());
    let byte = |at: usize| super::super::super::keyboard::utf16_to_byte_index(before, at);
    let (start, end) = (byte(start), byte(end));
    let inserted = after.get(start..after.len().checked_sub(before.len() - end)?)?;
    super::super::super::edit::Change::replacing(before.len(), [(start..end, inserted.to_string())])
}
