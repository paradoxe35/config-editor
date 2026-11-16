//! Problem markers between checks: found on the text last checked, they
//! follow the changes made since onto the text on screen.

use super::super::edit::{Assoc, Change};
use super::Diagnostic;

/// Where each line of a text starts, to turn (line, character column)
/// places into byte offsets and back.
struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, starts }
    }

    /// The byte offset of one-based `line` and `column`, and the columns
    /// past the line's end it asks for (a marker just past a line's last
    /// character); `None` for a line the text does not have.
    fn offset(&self, line: usize, column: usize) -> Option<(usize, usize)> {
        let start = *self.starts.get(line.checked_sub(1)?)?;
        let end = self
            .starts
            .get(line)
            .map_or(self.text.len(), |next| next - 1);
        let wanted = column.saturating_sub(1);
        let text = &self.text[start..end];
        Some(match text.char_indices().nth(wanted) {
            Some((at, _)) => (start + at, 0),
            None => (end, wanted - text.chars().count()),
        })
    }

    /// The one-based line and column of byte `at`.
    fn place(&self, at: usize) -> (usize, usize) {
        let at = at.min(self.text.len());
        let index = self.starts.partition_point(|start| *start <= at) - 1;
        let start = self.starts[index];
        (index + 1, self.text[start..at].chars().count() + 1)
    }
}

/// A marker's end mapped through a change: its place, and whether the
/// change deleted the text on both sides of it.
struct Moved {
    line: usize,
    column: usize,
    deleted: bool,
}

/// `diagnostics` found in `checked`, moved onto `current` through `change`
/// (made of `checked`, giving `current`): a marker's start keeps after
/// text written right at it, its end before, so it stays on the text it
/// marks. One whose text the change took away goes until the next check;
/// one without a place, or with a place `checked` does not have, stays.
pub(in crate::editor) fn follow_diagnostics(
    change: &Change,
    checked: &str,
    current: &str,
    diagnostics: &[Diagnostic],
) -> Vec<Diagnostic> {
    if diagnostics.is_empty() {
        return Vec::new();
    }
    let (before, after) = (Lines::new(checked), Lines::new(current));
    let moved = |(at, past_end): (usize, usize), assoc: Assoc| {
        let mapped = change.map_pos(at, assoc);
        let (line, column) = after.place(mapped.pos);
        Moved {
            line,
            column: column + past_end,
            deleted: mapped.deleted,
        }
    };
    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) else {
                return Some(diagnostic.clone());
            };
            let Some(from) = before.offset(line, column) else {
                return Some(diagnostic.clone());
            };
            let to = diagnostic.end_column.and_then(|end_column| {
                before.offset(diagnostic.last_line().unwrap_or(line), end_column)
            });
            let start = moved(from, Assoc::After);
            match to {
                // Over text: gone with the last of it.
                Some(to) if to.0 > from.0 => {
                    let end = moved(to, Assoc::Before);
                    ((end.line, end.column) > (start.line, start.column)).then(|| Diagnostic {
                        line: Some(start.line),
                        column: Some(start.column),
                        end_column: Some(end.column),
                        end_line: (end.line > start.line).then_some(end.line),
                        ..diagnostic.clone()
                    })
                }
                // Over no text (a point, or columns past a line's end):
                // gone with the text around it, as wide as it was.
                _ => (!start.deleted).then(|| Diagnostic {
                    line: Some(start.line),
                    column: Some(start.column),
                    end_column: diagnostic
                        .end_column
                        .map(|end_column| start.column + end_column.saturating_sub(column)),
                    end_line: None,
                    ..diagnostic.clone()
                }),
            }
        })
        .collect()
}
