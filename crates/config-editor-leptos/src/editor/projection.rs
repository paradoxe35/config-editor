//! The text the textarea shows with folds closed, and the mapping between
//! its offsets and rows and the document's.
//!
//! A closed fold removes the text from the end of its first line to the end
//! of its last hidden line (`"\n  b: 1\n  c: 2"`), so the line after the
//! fold follows the fold's first line in the textarea. Offsets are UTF-16,
//! as the textarea counts them.

use super::fold_ranges::FoldRange;
use super::keyboard::utf16_to_byte_index;

/// Which document offset a textarea offset at a fold's first line end
/// stands for: before the hidden text (`Before`, where the caret shows) or
/// after it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    Before,
    After,
}

/// Hidden lines `first..=last`, removed from the textarea.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Region {
    first: usize,
    last: usize,
    /// Where the hidden text starts in the document (the end of line
    /// `first - 1`).
    doc_start: usize,
    /// Its UTF-16 length.
    len: usize,
    /// Where it was cut out of the textarea's text.
    view_at: usize,
    /// Lines hidden by the regions before this one.
    lines_before: usize,
}

impl Region {
    /// UTF-16 units hidden up to this region's end.
    fn hidden_through(&self) -> usize {
        self.doc_start - self.view_at + self.len
    }

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
        let line_count = doc.split('\n').count();
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
        // Byte and UTF-16 offsets of each line's end.
        let mut ends: Vec<(usize, usize)> = Vec::with_capacity(line_count);
        let mut units = 0;
        for (byte, character) in doc.char_indices() {
            if character == '\n' {
                ends.push((byte, units));
            }
            units += character.len_utf16();
        }
        ends.push((doc.len(), units));
        let mut view = String::with_capacity(doc.len());
        let mut regions = Vec::with_capacity(merged.len());
        let (mut copied, mut lines_before, mut hidden_units) = (0, 0, 0);
        for (first, last) in merged {
            let (start_byte, start) = ends[first - 1];
            let (end_byte, end) = ends[last];
            view.push_str(&doc[copied..start_byte]);
            copied = end_byte;
            regions.push(Region {
                first,
                last,
                doc_start: start,
                len: end - start,
                view_at: start - hidden_units,
                lines_before,
            });
            lines_before += last + 1 - first;
            hidden_units += end - start;
        }
        view.push_str(&doc[copied..]);
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

    /// Whether a fold hides text inside UTF-16 `start..end`, so the
    /// textarea's own copy of that selection would miss it.
    pub(crate) fn hides_within(&self, start: usize, end: usize) -> bool {
        let first = self
            .regions
            .partition_point(|region| region.doc_start < start);
        self.regions
            .get(first)
            .is_some_and(|region| region.doc_start < end)
    }

    /// Rows in the textarea.
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// The textarea offset for document offset `offset`; a hidden offset
    /// maps to the end of its fold's first line.
    pub(crate) fn to_view(&self, offset: usize) -> usize {
        let after = self
            .regions
            .partition_point(|region| region.doc_start < offset);
        let Some(region) = after.checked_sub(1).map(|index| self.regions[index]) else {
            return offset;
        };
        if offset <= region.doc_start + region.len {
            region.view_at
        } else {
            offset - region.hidden_through()
        }
    }

    /// The document offset for textarea offset `offset`, on `side` of the
    /// hidden text when it is at a fold's first line end.
    pub(crate) fn to_doc(&self, offset: usize, side: Side) -> usize {
        let after = match side {
            Side::Before => self
                .regions
                .partition_point(|region| region.view_at < offset),
            Side::After => self
                .regions
                .partition_point(|region| region.view_at <= offset),
        };
        match after.checked_sub(1) {
            Some(index) => offset + self.regions[index].hidden_through(),
            None => offset,
        }
    }

    /// The hidden UTF-16 range (`start..end` in the document) holding
    /// `offset`, if a fold hides it.
    #[cfg(feature = "folding")]
    pub(crate) fn hidden_at(&self, offset: usize) -> Option<(usize, usize)> {
        let after = self
            .regions
            .partition_point(|region| region.doc_start < offset);
        let region = self.regions[after.checked_sub(1)?];
        (offset <= region.doc_start + region.len)
            .then_some((region.doc_start, region.doc_start + region.len))
    }

    /// The end (UTF-16, in the document) of the text a fold hides from
    /// `line_end`, the end of its first line, if one does.
    pub(crate) fn hidden_from(&self, line_end: usize) -> Option<usize> {
        let at = self
            .regions
            .partition_point(|region| region.doc_start < line_end);
        let region = self.regions.get(at)?;
        (region.doc_start == line_end).then_some(region.doc_start + region.len)
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
    /// to `edited` (an edit the browser made): the replaced text mapped
    /// into `doc`, never taking hidden text with it. An insertion at a fold's
    /// first line end goes before the hidden text.
    pub(crate) fn unproject(&self, doc: &str, edited: &str) -> String {
        let Some(view) = self.view.as_deref() else {
            return edited.to_string();
        };
        let prefix = common_prefix(view, edited);
        let suffix = common_suffix(&view[prefix..], &edited[prefix..]);
        let (start, end) = (prefix, view.len() - suffix);
        let inserted = &edited[prefix..edited.len() - suffix];
        let start_units = view[..start].encode_utf16().count();
        let end_units = start_units + view[start..end].encode_utf16().count();
        let side = if start == end {
            Side::Before
        } else {
            Side::After
        };
        let from = utf16_to_byte_index(doc, self.to_doc(start_units, side));
        let to = utf16_to_byte_index(doc, self.to_doc(end_units, Side::Before)).max(from);
        let mut result = String::with_capacity(doc.len() + inserted.len());
        result.push_str(&doc[..from]);
        result.push_str(inserted);
        result.push_str(&doc[to..]);
        result
    }
}

/// Bytes `a` and `b` share at the start, on a character boundary.
pub(crate) fn common_prefix(a: &str, b: &str) -> usize {
    let mut length = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    while !a.is_char_boundary(length) || !b.is_char_boundary(length) {
        length -= 1;
    }
    length
}

/// Bytes `a` and `b` share at the end, on a character boundary.
pub(crate) fn common_suffix(a: &str, b: &str) -> usize {
    let mut length = a
        .bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(x, y)| x == y)
        .count();
    while !a.is_char_boundary(a.len() - length) || !b.is_char_boundary(b.len() - length) {
        length -= 1;
    }
    length
}
