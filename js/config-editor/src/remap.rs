//! Ranges a host gave for one text, moved to another: `readOnlyRanges` and
//! host diagnostics computed for a text the user has typed past (an echo
//! skipped, an update's `rangesFor`).
//!
//! The two texts differ in stretches, any number of them (see
//! `stretches`: their lines diffed, each changed run narrowed to the
//! characters that differ). A place between stretches moves by what the
//! stretches before it changed, and one inside a stretch goes to its start
//! (a range's start) or end (a range's end): a range only ever grows, so no
//! character it covered and the edits kept is left out of it (a lock never
//! lets a locked character be edited). A range clear of every stretch
//! covers the same text: typing in several places maps every range away
//! from them exactly.

// By path: the native tests (`tests/remap.rs`) load this file by path too.
#[path = "remap/stretches.rs"]
mod stretches;

use config_editor_leptos::{DiagnosticTarget, HostDiagnostic, TextPosition, TextRange};

use crate::text_lines::TextLines;
use stretches::{Stretch, stretches};

/// Moves positions in `from` to `to`.
pub(crate) struct Remap<'a> {
    from: TextLines<'a>,
    to: TextLines<'a>,
    /// Where the two differ, in order.
    stretches: Vec<Stretch>,
}

impl<'a> Remap<'a> {
    pub(crate) fn new(from: &'a str, to: &'a str) -> Self {
        Self {
            stretches: stretches(from, to),
            from: TextLines::new(from),
            to: TextLines::new(to),
        }
    }

    /// Where offset `offset` of `from` is in `to` for a range's start
    /// (`at_end` false) or end: inside a stretch, at its start or end; at
    /// a text inserted there, before it for an end and after it for a
    /// start (a range does not take in what is typed at its edge).
    fn offset(&self, offset: usize, at_end: bool) -> usize {
        // The stretches wholly before the offset (in order, so found by
        // halves: thousands of ranges over thousands of stretches).
        let passed = self.stretches.partition_point(|stretch| {
            if at_end {
                offset > stretch.from.start && offset >= stretch.from.end
            } else {
                offset >= stretch.from.end
            }
        });
        // Where the unchanged text after the last stretch passed starts, in
        // each text.
        let (from_start, to_start) = passed.checked_sub(1).map_or((0, 0), |last| {
            (self.stretches[last].from.end, self.stretches[last].to.end)
        });
        match self.stretches.get(passed) {
            Some(stretch) if offset > stretch.from.start => {
                if at_end {
                    stretch.to.end
                } else {
                    stretch.to.start
                }
            }
            _ => to_start + (offset - from_start),
        }
    }

    fn position(&self, position: TextPosition, at_end: bool) -> TextPosition {
        self.to
            .position(self.offset(self.from.offset(position), at_end))
    }

    /// `range` of `from`, in `to`: never smaller than what is left of it.
    pub(crate) fn range(&self, range: TextRange) -> TextRange {
        let start = self.position(range.start, false);
        // An empty range where text was inserted: after it, as its start.
        TextRange::new(start, self.position(range.end, true).max(start))
    }

    pub(crate) fn ranges(&self, ranges: Vec<TextRange>) -> Vec<TextRange> {
        ranges.into_iter().map(|range| self.range(range)).collect()
    }

    /// Host diagnostics given for `from`, in `to` (by path or for the whole
    /// document: as they are).
    pub(crate) fn diagnostics(&self, diagnostics: Vec<HostDiagnostic>) -> Vec<HostDiagnostic> {
        diagnostics
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.target = match diagnostic.target {
                    DiagnosticTarget::Range(range) => DiagnosticTarget::Range(self.range(range)),
                    DiagnosticTarget::Position(at) => {
                        DiagnosticTarget::Position(self.position(at, false))
                    }
                    target => target,
                };
                diagnostic
            })
            .collect()
    }
}

/// `text` as the editor reads positions in it: without a leading byte
/// order mark (no part of the document) and with every line ending one
/// line feed (CRLF, a lone CR and LF are each one line break). The text a
/// host gives ranges for (`rangesFor`) may be the document as stored or as
/// the editor shows it (`text`): they name the same places.
fn as_read(text: &str) -> std::borrow::Cow<'_, str> {
    let text = crate::text_lines::document(text);
    if text.contains('\r') {
        std::borrow::Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// Whether `a` and `b` are one text as the editor reads it (see
/// [`as_read`]): a host store writing LF or dropping the byte order mark
/// hands back the document it was given. Read again only when they differ.
pub(crate) fn reads_as(a: &str, b: &str) -> bool {
    a == b || as_read(a) == as_read(b)
}

/// Host `ranges` and `diagnostics` given for `from`, moved to `to` (each
/// read as the editor reads positions: see [`as_read`]); as they are when
/// the two name the same places.
pub(crate) fn moved_between(
    from: &str,
    to: &str,
    ranges: Option<Vec<TextRange>>,
    diagnostics: Option<Vec<HostDiagnostic>>,
) -> (Option<Vec<TextRange>>, Option<Vec<HostDiagnostic>>) {
    let (from, to) = (as_read(from), as_read(to));
    if from == to {
        return (ranges, diagnostics);
    }
    let remap = Remap::new(&from, &to);
    (
        ranges.map(|list| remap.ranges(list)),
        diagnostics.map(|list| remap.diagnostics(list)),
    )
}

#[cfg(test)]
#[path = "remap/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "remap/scale_tests.rs"]
mod scale_tests;
