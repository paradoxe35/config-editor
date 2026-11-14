//! A leading byte order mark (`U+FEFF`) is no part of the document. The
//! editor's text never holds it (`line_endings` keeps it for the host), and
//! every public entry point that reads a text (`Language`'s and `Schema`'s
//! methods, `check_syntax`, the save gate `Language::saves` that a
//! never-shown `ConfigDocuments` document is read by) reads it through
//! [`read`]: the text after the mark is what each language sees, and what
//! comes back is placed in the text given (one column on in the first line,
//! one UTF-16 unit on for an offset). No language and no shared code below the entry points knows of
//! the mark, so no offset is ever taken in one text and applied to the
//! other (QA 31: XML element text with an emoji after a mark panicked a
//! conversion).

use serde_json::Value;

use super::codec::{Conversion, ConvertError};
use super::completion::Suggestion;
use super::diagnostics::Diagnostic;
use super::language::ParseError;
use super::paths::{FieldPath, TextPosition, TextRange};

const MARK: char = '\u{FEFF}';

/// The text after a leading mark, and whether there was one.
pub(crate) fn split(source: &str) -> (&str, bool) {
    match source.strip_prefix(MARK) {
        Some(rest) => (rest, true),
        None => (source, false),
    }
}

/// `text` with the mark `source` starts with, if it does.
pub(crate) fn restore(source: &str, text: String) -> String {
    if split(source).1 { marked(text) } else { text }
}

/// `text` starting with the mark, once. An emptied document keeps it: the
/// mark belongs to the file's encoding, not to what was typed, so undoing
/// back to nothing is the file the host started with.
pub(crate) fn marked(text: String) -> String {
    if split(&text).1 {
        text
    } else {
        format!("{MARK}{text}")
    }
}

/// `read` of the document in `source` (the text after a leading mark),
/// handed `To` placing a place of `source` in the document (see
/// [`Place::in_document`]); its answer placed in `source`.
pub(crate) fn read<T: Placed>(source: &str, read: impl FnOnce(&str, To) -> T) -> T {
    let (text, marked) = split(source);
    let answer = read(text, To(marked));
    if marked { answer.placed() } else { answer }
}

/// Whether the text given starts with a mark: moves a place in it to the
/// document after the mark.
#[derive(Clone, Copy)]
pub(crate) struct To(bool);

impl To {
    /// `place` of the text given, in the document; the mark itself is the
    /// document's start.
    pub(crate) fn document<P: Place>(self, place: P) -> P {
        if self.0 { place.in_document() } else { place }
    }
}

/// A place a caller names in the text given.
pub(crate) trait Place {
    /// The same place in the text after the mark.
    fn in_document(self) -> Self;
}

impl Place for TextPosition {
    fn in_document(self) -> Self {
        let column = if self.line == 1 {
            self.column.saturating_sub(1).max(1)
        } else {
            self.column
        };
        TextPosition::new(self.line, column)
    }
}

/// A UTF-16 offset.
impl Place for usize {
    fn in_document(self) -> Self {
        self.saturating_sub(1)
    }
}

/// An answer read in the document, placed in the text with the mark.
pub(crate) trait Placed {
    fn placed(self) -> Self;
}

/// A first-line column, one on.
fn column_on(line: Option<usize>, column: Option<usize>) -> Option<usize> {
    match line {
        Some(1) => column.map(|column| column + 1),
        _ => column,
    }
}

impl Placed for TextPosition {
    fn placed(self) -> Self {
        TextPosition::new(
            self.line,
            column_on(Some(self.line), Some(self.column)).unwrap_or(self.column),
        )
    }
}

impl Placed for TextRange {
    fn placed(self) -> Self {
        TextRange::new(self.start.placed(), self.end.placed())
    }
}

impl Placed for ParseError {
    fn placed(self) -> Self {
        ParseError {
            column: column_on(self.line, self.column),
            ..self
        }
    }
}

impl Placed for ConvertError {
    fn placed(self) -> Self {
        match self {
            ConvertError::Parse(error) => ConvertError::Parse(error.placed()),
            other => other,
        }
    }
}

impl Placed for Diagnostic {
    fn placed(mut self) -> Self {
        self.end_column = column_on(self.end_line.or(self.line), self.end_column);
        self.column = column_on(self.line, self.column);
        self
    }
}

impl Placed for Suggestion {
    fn placed(self) -> Self {
        self.moved(MARK.len_utf16())
    }
}

/// Answers without a place in the text.
macro_rules! unplaced {
    ($($answer:ty),+) => {$(
        impl Placed for $answer {
            fn placed(self) -> Self {
                self
            }
        }
    )+};
}

unplaced!(
    bool,
    Value,
    Conversion,
    FieldPath,
    super::completion::HoverDocs
);
#[cfg(feature = "folding")]
unplaced!(super::fold_ranges::FoldRange);

impl<T: Placed> Placed for Option<T> {
    fn placed(self) -> Self {
        self.map(Placed::placed)
    }
}

impl<T: Placed> Placed for Vec<T> {
    fn placed(self) -> Self {
        self.into_iter().map(Placed::placed).collect()
    }
}

impl<T: Placed, E: Placed> Placed for Result<T, E> {
    fn placed(self) -> Self {
        self.map(Placed::placed).map_err(Placed::placed)
    }
}
