//! What an edit replaced, as the links follow it: the ranges of the text
//! before it rewrote, each with the text written there, and the parts of
//! that text that are text the edit moved or copied (a line command), as
//! the edit says it, or the selection it replaced, or else the change read
//! at the caret it left.

use super::super::super::keyboard::utf16_to_byte_index;
use super::super::super::text_change::edit_range;

/// `removed` bytes at byte `at` of the text before, replaced by
/// `inserted`.
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
    fn replacing(before: &str, after: &str, (start, end): (usize, usize)) -> Option<Self> {
        let kept = before.len().checked_sub(end.checked_sub(start)?)?;
        let inserted_end = start + after.len().checked_sub(kept)?;
        (before.get(..start)? == after.get(..start)?
            && before.get(end..)? == after.get(inserted_end..)?)
        .then(|| Splice {
            at: start,
            removed: end - start,
            inserted: after[start..inserted_end].to_string(),
            moved: Vec::new(),
        })
    }

    /// The same edit of `before` without the text it writes back as it
    /// was at either end (a whole text pasted with one change is that
    /// change).
    pub(crate) fn narrowed(&self, before: &str) -> Splice {
        let removed = &before[self.at..self.at + self.removed];
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
        Splice {
            at: self.at + prefix,
            removed: removed_rest.len() - suffix,
            inserted: inserted_rest[..inserted_rest.len() - suffix].to_string(),
            moved: Vec::new(),
        }
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

    /// The edit `before` → `after` that replaced the UTF-16 `selection`
    /// when the texts agree it did, else the change that left the caret at
    /// UTF-16 `caret`.
    pub(crate) fn of(
        before: &str,
        after: &str,
        selection: (usize, usize),
        caret: usize,
    ) -> Vec<Splice> {
        let bytes = |(start, end): (usize, usize)| {
            (
                utf16_to_byte_index(before, start),
                utf16_to_byte_index(before, end),
            )
        };
        let (start, end) = selection;
        let selected = (start <= end && end <= before.len())
            .then(|| Splice::replacing(before, after, bytes(selection)))
            .flatten();
        let splice = selected
            .or_else(|| Splice::replacing(before, after, bytes(edit_range(before, after, caret))))
            // The whole text, when nothing else reads (no item kept).
            .unwrap_or_else(|| Splice {
                at: 0,
                removed: before.len(),
                inserted: after.to_string(),
                moved: Vec::new(),
            });
        vec![splice]
    }

    /// The edit `before` → `after` line by line, when it keeps the number
    /// of lines (an indent, a Replace all within lines): each changed line
    /// one splice, from its first changed byte to its last.
    pub(crate) fn per_line(before: &str, after: &str) -> Option<Vec<Splice>> {
        let (old, new) = (starts(before), starts(after));
        if old.len() != new.len() {
            return None;
        }
        let mut out = Vec::new();
        for line in 0..old.len() - 1 {
            let (a, b) = (
                &before[old[line]..old[line + 1] - 1],
                &after[new[line]..new[line + 1] - 1],
            );
            if a == b {
                continue;
            }
            let prefix = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
            let prefix = (0..=prefix)
                .rev()
                .find(|at| a.is_char_boundary(*at) && b.is_char_boundary(*at))
                .unwrap_or(0);
            let suffix = a[prefix..]
                .bytes()
                .rev()
                .zip(b[prefix..].bytes().rev())
                .take_while(|(x, y)| x == y)
                .count();
            let suffix = (0..=suffix)
                .rev()
                .find(|at| a.is_char_boundary(a.len() - at) && b.is_char_boundary(b.len() - at))
                .unwrap_or(0);
            out.push(Splice {
                at: old[line] + prefix,
                removed: a.len() - prefix - suffix,
                inserted: b[prefix..b.len() - suffix].to_string(),
                moved: Vec::new(),
            });
        }
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
