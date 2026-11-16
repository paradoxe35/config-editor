//! The text the textarea shows with folds closed, and the mapping between
//! its offsets and rows and the document's.
//!
//! A closed fold removes the text from the end of its first line to the end
//! of its last hidden line (`"\n  b: 1\n  c: 2"`), so the line after the
//! fold follows the fold's first line in the textarea. Inside the editor
//! offsets are bytes of the document and of the textarea's text; the
//! textarea's own UTF-16 offsets are mapped for the browser edge
//! (`source_area`) and its copy, by the same cuts counted in code units.

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::ops::{Add, Sub};

use super::edit::Change;
use super::fold_ranges::FoldRange;
use crate::editor::text::{TextRange, TextSize, Utf16, slice, text_size};

/// Which document offset a textarea offset at a fold's first line end
/// stands for: before the hidden text (`Before`, where the caret shows) or
/// after it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    Before,
    After,
}

/// What the cuts are counted in: bytes ([`TextSize`]) or the textarea's
/// code units ([`Utf16`]).
trait Measure: Copy + Ord + Add<Output = Self> + Sub<Output = Self> {}

impl<T: Copy + Ord + Add<Output = T> + Sub<Output = T>> Measure for T {}

/// Where a region's hidden text was cut, in one measure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cut<T> {
    /// Where the hidden text starts in the document (the end of the line
    /// before the region).
    doc_start: T,
    /// Its length.
    len: T,
    /// Where it was cut out of the textarea's text.
    view_at: T,
}

impl<T: Measure> Cut<T> {
    /// Where the hidden text ends in the document.
    fn doc_end(&self) -> T {
        self.doc_start + self.len
    }

    /// The text hidden up to this region's end.
    fn hidden_through(&self) -> T {
        self.doc_start - self.view_at + self.len
    }
}

/// Hidden lines `first..=last`, removed from the textarea.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Region {
    first: usize,
    last: usize,
    /// The cut in bytes.
    bytes: Cut<TextSize>,
    /// The same cut in UTF-16 code units, as the textarea counts.
    units: Cut<Utf16>,
    /// Lines hidden by the regions before this one.
    lines_before: usize,
}

impl Region {
    fn lines(&self) -> usize {
        self.last + 1 - self.first
    }

    /// The textarea row of the fold's first line.
    fn row(&self) -> usize {
        self.first - 1 - self.lines_before
    }
}

/// The document as the textarea shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Projection {
    /// The textarea's text; `None` while nothing is folded (the document
    /// itself).
    view: Option<String>,
    regions: Vec<Region>,
    /// Rows in the textarea.
    rows: usize,
}

impl Projection {
    /// `doc` with the lines `folds` hide removed.
    pub(crate) fn new(doc: &str, folds: &[FoldRange]) -> Self {
        let line_count = doc.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let mut hidden: Vec<(usize, usize)> = folds
            .iter()
            .filter(|fold| fold.start + 1 < line_count)
            .map(|fold| (fold.start + 1, fold.end.min(line_count - 1)))
            .filter(|(first, last)| last >= first)
            .collect();
        if hidden.is_empty() {
            return Self {
                view: None,
                regions: Vec::new(),
                rows: line_count,
            };
        }
        hidden.sort_unstable();
        // Overlapping or touching ranges hide one run of lines.
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hidden.len());
        for (first, last) in hidden {
            match merged.last_mut() {
                Some((_, end)) if first <= *end + 1 => *end = (*end).max(last),
                _ => merged.push((first, last)),
            }
        }
        // Each line's end in bytes and in UTF-16 units.
        let mut ends: Vec<(TextSize, Utf16)> = Vec::with_capacity(line_count);
        let mut units = 0;
        for (byte, character) in doc.char_indices() {
            if character == '\n' {
                ends.push((text_size(byte), Utf16::count(units)));
            }
            units += character.len_utf16();
        }
        ends.push((TextSize::of(doc), Utf16::count(units)));
        let mut view = String::with_capacity(doc.len());
        let mut regions = Vec::with_capacity(merged.len());
        let mut copied = TextSize::default();
        let mut lines_before = 0;
        let mut hidden_bytes = TextSize::default();
        let mut hidden_units = Utf16::default();
        for (first, last) in merged {
            let (start_byte, start_units) = ends[first - 1];
            let (end_byte, end_units) = ends[last];
            // Both ends are line ends of `doc`: character boundaries.
            view.push_str(slice::slice(doc, TextRange::new(copied, start_byte)).unwrap_or(""));
            copied = end_byte;
            regions.push(Region {
                first,
                last,
                bytes: Cut {
                    doc_start: start_byte,
                    len: end_byte - start_byte,
                    view_at: start_byte - hidden_bytes,
                },
                units: Cut {
                    doc_start: start_units,
                    len: end_units - start_units,
                    view_at: start_units - hidden_units,
                },
                lines_before,
            });
            lines_before += last + 1 - first;
            hidden_bytes += end_byte - start_byte;
            hidden_units += end_units - start_units;
        }
        view.push_str(slice::after(doc, copied).unwrap_or(""));
        Self {
            view: Some(view),
            regions,
            rows: line_count - lines_before,
        }
    }

    /// The textarea's text for `doc`.
    pub(crate) fn view<'a>(&'a self, doc: &'a str) -> &'a str {
        self.view.as_deref().unwrap_or(doc)
    }

    /// Whether a fold hides text inside `start..end` (UTF-16 units of the
    /// document), so the textarea's own copy of that selection would miss
    /// it.
    pub(crate) fn hides_within(&self, start: Utf16, end: Utf16) -> bool {
        let first = self
            .regions
            .partition_point(|region| region.units.doc_start < start);
        self.regions
            .get(first)
            .is_some_and(|region| region.units.doc_start < end)
    }

    /// Rows in the textarea.
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// The textarea offset for document offset `offset` (both counted by
    /// `cut`); a hidden offset maps to the end of its fold's first line.
    fn to_view_in<T: Measure>(&self, cut: impl Fn(&Region) -> Cut<T>, offset: T) -> T {
        let after = self
            .regions
            .partition_point(|region| cut(region).doc_start < offset);
        let Some(region) = after.checked_sub(1).map(|index| cut(&self.regions[index])) else {
            return offset;
        };
        if offset <= region.doc_end() {
            region.view_at
        } else {
            offset - region.hidden_through()
        }
    }

    /// The document offset for textarea offset `offset` (both counted by
    /// `cut`), on `side` of the hidden text when it is at a fold's first
    /// line end.
    fn to_doc_in<T: Measure>(&self, cut: impl Fn(&Region) -> Cut<T>, offset: T, side: Side) -> T {
        let after = match side {
            Side::Before => self
                .regions
                .partition_point(|region| cut(region).view_at < offset),
            Side::After => self
                .regions
                .partition_point(|region| cut(region).view_at <= offset),
        };
        match after.checked_sub(1) {
            Some(index) => offset + cut(&self.regions[index]).hidden_through(),
            None => offset,
        }
    }

    /// The byte of the textarea's text for byte `at` of the document; a
    /// hidden byte maps to the end of its fold's first line.
    #[cfg(all(test, feature = "folding"))]
    pub(crate) fn to_view(&self, at: TextSize) -> TextSize {
        self.to_view_in(|region| region.bytes, at)
    }

    /// The byte of the document for byte `at` of the textarea's text, on
    /// `side` of the hidden text when it is at a fold's first line end.
    pub(crate) fn to_doc(&self, at: TextSize, side: Side) -> TextSize {
        self.to_doc_in(|region| region.bytes, at, side)
    }

    /// The textarea's UTF-16 offset for the document's UTF-16 `offset` (see
    /// [`to_view`](Self::to_view)): the browser edge's mapping.
    pub(crate) fn view_units(&self, offset: Utf16) -> Utf16 {
        self.to_view_in(|region| region.units, offset)
    }

    /// The document's UTF-16 offset for the textarea's UTF-16 `offset` (see
    /// [`to_doc`](Self::to_doc)): the browser edge's mapping.
    pub(crate) fn doc_units(&self, offset: Utf16, side: Side) -> Utf16 {
        self.to_doc_in(|region| region.units, offset, side)
    }

    /// The hidden byte range of the document holding byte `at`, if a fold
    /// hides it.
    #[cfg(feature = "folding")]
    pub(crate) fn hidden_at(&self, at: TextSize) -> Option<TextRange> {
        let after = self
            .regions
            .partition_point(|region| region.bytes.doc_start < at);
        let cut = self.regions[after.checked_sub(1)?].bytes;
        (at <= cut.doc_end()).then(|| TextRange::new(cut.doc_start, cut.doc_end()))
    }

    /// The end (a byte of the document) of the text a fold hides from
    /// `line_end`, the end of its first line, if one does.
    pub(crate) fn hidden_from(&self, line_end: TextSize) -> Option<TextSize> {
        let at = self
            .regions
            .partition_point(|region| region.bytes.doc_start < line_end);
        let cut = self.regions.get(at)?.bytes;
        (cut.doc_start == line_end).then(|| cut.doc_end())
    }

    /// The textarea row of document line `line` (0-based); a hidden line is
    /// on its fold's first row.
    pub(crate) fn row_of_line(&self, line: usize) -> usize {
        let after = self.regions.partition_point(|region| region.first <= line);
        let Some(region) = after.checked_sub(1).map(|index| self.regions[index]) else {
            return line;
        };
        if line <= region.last {
            region.row()
        } else {
            line - region.lines_before - region.lines()
        }
    }

    /// The document line (0-based) on textarea row `row`.
    pub(crate) fn line_of_row(&self, row: usize) -> usize {
        let after = self.regions.partition_point(|region| region.row() < row);
        match after.checked_sub(1) {
            Some(index) => {
                let region = self.regions[index];
                row + region.lines_before + region.lines()
            }
            None => row,
        }
    }

    /// Whether a fold hides document line `line`.
    pub(crate) fn is_hidden(&self, line: usize) -> bool {
        let after = self.regions.partition_point(|region| region.first <= line);
        after
            .checked_sub(1)
            .is_some_and(|index| line <= self.regions[index].last)
    }

    /// The document after the textarea's text went from this projection's
    /// to `edited` (an edit the browser made), the browser's caret left at
    /// byte `caret` of `edited`: the change on screen, placed so the text
    /// it wrote ends at the caret (see [`Change::diffed_at`]), mapped into
    /// `doc`, never taking hidden text with it. An insertion at a fold's
    /// first line end goes before the hidden text.
    pub(crate) fn unproject(&self, doc: &str, edited: &str, caret: TextSize) -> String {
        let Some(view) = self.view.as_deref() else {
            return edited.to_string();
        };
        let on_screen = Change::diffed_at(view, edited, caret);
        let in_doc = on_screen.edits().map(|(range, inserted)| {
            let side = if range.is_empty() {
                Side::Before
            } else {
                Side::After
            };
            let from = self.to_doc(range.start(), side);
            let to = self.to_doc(range.end(), Side::Before).max(from);
            (TextRange::new(from, to), inserted.to_string())
        });
        let change: Option<Change> = Change::replacing(TextSize::of(doc), in_doc);
        change
            .and_then(|change| change.apply(doc))
            .unwrap_or_else(|| doc.to_string())
    }
}
