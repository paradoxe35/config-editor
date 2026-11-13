//! A long row of the code layer drawn in pieces, so a keystroke draws again
//! only the pieces it changed.
//!
//! The browser lays a line out whole: a row of a million characters
//! changed anywhere is shaped and measured again from its start, however
//! its text is split into elements. A piece is an inline block, which the
//! browser lays out on its own and keeps while its content stays the same:
//! a row's pieces are cut once and kept through edits (each cut moves with
//! the text around it), and the ones an edit leaves as they were keep
//! their elements.
//!
//! A piece lays its text out apart from its neighbours, so cuts go only
//! where that draws the same: before an ASCII character (no combining mark
//! or joined sequence starts a piece), outside the marked ranges, and only
//! in lines without tab stops (counted from each piece's start) or
//! right-to-left text (ordered within each piece).

use std::ops::Range;
use std::sync::Arc;

use super::{Mark, Run};
use crate::editor::work_count::{Work, note_many};

/// Rows up to this many bytes are drawn whole.
const LONG_ROW: usize = 8 * 1024;
/// The size a new piece is cut at.
const PIECE: usize = 4 * 1024;
/// A piece grown past this is cut again.
const MAX_PIECE: usize = 2 * PIECE;
/// A piece shrunk below this joins the next one.
const MIN_PIECE: usize = PIECE / 4;

/// What one piece draws: its text, its runs and its marks, in bytes of its
/// own text.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::editor) struct PieceText {
    pub(in crate::editor) text: String,
    pub(in crate::editor) runs: Vec<Run>,
    pub(in crate::editor) marks: Vec<Mark>,
}

/// One piece of a row, with the id its element is kept by.
#[derive(Clone, Debug)]
pub(in crate::editor) struct Piece {
    pub(in crate::editor) id: u64,
    pub(in crate::editor) content: Arc<PieceText>,
    /// Where it starts in the row.
    start: usize,
}

/// A long row as pieces.
#[derive(Debug)]
pub(in crate::editor) struct RowPieces {
    line: String,
    pieces: Vec<Piece>,
    next_id: u64,
}

/// Whether `line` is drawn in pieces.
pub(in crate::editor) fn splits(line: &str) -> bool {
    line.len() > LONG_ROW && !line.contains('\t') && !writes_right_to_left(line)
}

/// Whether `line` holds right-to-left text or a mark that orders text so.
fn writes_right_to_left(line: &str) -> bool {
    !line.is_ascii()
        && line.chars().any(|character| {
            matches!(
                u32::from(character),
                0x0590..=0x08FF
                    | 0xFB1D..=0xFDFF
                    | 0xFE70..=0xFEFF
                    | 0x10800..=0x10FFF
                    | 0x1E800..=0x1EFFF
                    | 0x200F
                    | 0x202B
                    | 0x202E
                    | 0x2067
            )
        })
}

impl RowPieces {
    /// The pieces of `line` (cut into `runs`, with `marks`), kept from
    /// `previous` where the line is as it was; and whether they changed.
    pub(in crate::editor) fn next(
        previous: Option<RowPieces>,
        line: &str,
        runs: &[Run],
        marks: &[Mark],
    ) -> (RowPieces, bool) {
        let shift = previous
            .as_ref()
            .map(|previous| Shift::of(&previous.line, line));
        let kept: Vec<usize> = match (&previous, &shift) {
            (Some(previous), Some(shift)) => previous
                .pieces
                .iter()
                .skip(1)
                .filter_map(|piece| shift.forward(piece.start))
                .collect(),
            _ => Vec::new(),
        };
        let cuts = cuts(line, kept, marks);
        let mut next_id = previous.as_ref().map_or(0, |previous| previous.next_id);
        let pieces: Vec<Piece> = cuts
            .windows(2)
            .map(|pair| {
                let (start, end) = (pair[0], pair[1]);
                let content = piece_text(line, start..end, runs, marks);
                let kept = previous
                    .as_ref()
                    .zip(shift.as_ref())
                    .and_then(|(previous, shift)| previous.at(shift.back(start)?))
                    .filter(|old| *old.content == content);
                match kept {
                    Some(old) => Piece {
                        start,
                        ..old.clone()
                    },
                    None => {
                        note_many(Work::RowText, end - start);
                        next_id += 1;
                        Piece {
                            id: next_id,
                            content: Arc::new(content),
                            start,
                        }
                    }
                }
            })
            .collect();
        let changed = previous.as_ref().is_none_or(|previous| {
            let ids = |pieces: &[Piece]| pieces.iter().map(|piece| piece.id).collect::<Vec<_>>();
            ids(&previous.pieces) != ids(&pieces)
        });
        (
            RowPieces {
                line: line.to_string(),
                pieces,
                next_id,
            },
            changed,
        )
    }

    /// The piece starting at byte `start`.
    fn at(&self, start: usize) -> Option<&Piece> {
        let found = self
            .pieces
            .binary_search_by_key(&start, |piece| piece.start)
            .ok()?;
        Some(&self.pieces[found])
    }

    /// The pieces, in order.
    pub(in crate::editor) fn pieces(&self) -> &[Piece] {
        &self.pieces
    }
}

/// How an edit moved a line's bytes: the bytes before `prefix` stay, the
/// ones from `old_end` on move to `new_end`, the ones between are new.
struct Shift {
    prefix: usize,
    old_end: usize,
    new_end: usize,
}

impl Shift {
    fn of(old: &str, new: &str) -> Self {
        let (old, new) = (old.as_bytes(), new.as_bytes());
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let room = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        Shift {
            prefix,
            old_end: old.len() - suffix,
            new_end: new.len() - suffix,
        }
    }

    /// Where old byte `at` is now, unless the edit replaced it.
    fn forward(&self, at: usize) -> Option<usize> {
        if at <= self.prefix {
            Some(at)
        } else if at >= self.old_end {
            Some(at - self.old_end + self.new_end)
        } else {
            None
        }
    }

    /// Where new byte `at` was, unless the edit wrote it.
    fn back(&self, at: usize) -> Option<usize> {
        if at <= self.prefix {
            Some(at)
        } else if at >= self.new_end {
            Some(at - self.new_end + self.old_end)
        } else {
            None
        }
    }
}

/// The cuts of `line` (its start and end included): the `kept` ones that
/// still can be, more where a piece grew too long, fewer where one shrank.
fn cuts(line: &str, kept: Vec<usize>, marks: &[Mark]) -> Vec<usize> {
    let mut cuts = vec![0];
    for end in kept
        .into_iter()
        .filter(|at| *at < line.len() && can_cut(line, *at, marks))
        .chain([line.len()])
    {
        let mut from = *cuts.last().expect("starts with 0");
        if end <= from {
            continue;
        }
        while end - from > MAX_PIECE {
            match cut_after(line, from + PIECE, end, marks) {
                Some(at) => {
                    cuts.push(at);
                    from = at;
                }
                None => break,
            }
        }
        if end - from >= MIN_PIECE || end == line.len() {
            cuts.push(end);
        }
    }
    if *cuts.last().expect("starts with 0") != line.len() {
        cuts.push(line.len());
    }
    cuts
}

/// The first place from `at` (before `before`) a piece can start.
fn cut_after(line: &str, at: usize, before: usize, marks: &[Mark]) -> Option<usize> {
    (at..before).find(|at| can_cut(line, *at, marks))
}

/// Whether a piece can start at byte `at` of `line`, drawing as the line
/// drawn whole: before an ASCII character, outside every mark.
fn can_cut(line: &str, at: usize, marks: &[Mark]) -> bool {
    at > 0
        && line.as_bytes().get(at).is_some_and(u8::is_ascii)
        && !marks
            .iter()
            .any(|(range, _)| range.start < at && at < range.end)
}

/// What `line`'s `range` draws, in its own bytes.
fn piece_text(line: &str, range: Range<usize>, runs: &[Run], marks: &[Mark]) -> PieceText {
    let clip = |start: usize, end: usize| {
        let (start, end) = (start.max(range.start), end.min(range.end));
        (start < end).then(|| (start - range.start, end - range.start))
    };
    let first = runs.partition_point(|run| run.1 <= range.start);
    PieceText {
        text: line[range.clone()].to_string(),
        runs: runs[first..]
            .iter()
            .take_while(|run| run.0 < range.end)
            .filter_map(|&(start, end, class)| {
                clip(start, end).map(|(start, end)| (start, end, class))
            })
            .collect(),
        marks: marks
            .iter()
            .filter_map(|(mark, class)| {
                clip(mark.start, mark.end).map(|(start, end)| (start..end, *class))
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
