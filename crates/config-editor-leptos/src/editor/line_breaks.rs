//! One line-break rule for every position the API gives or takes: LF,
//! CRLF and a lone CR each end a line, as the editor reads a host's text.
//! A text from outside is read with LF breaks ([`as_lf`]): its lines and
//! columns are the same, so a position found there is one in the text
//! given, and [`offset_at`] turns it into that text's own UTF-16 offset.
//! Whether the language accepts a lone CR is still its reader's call (TOML
//! and HCL do not: their reader's error then marks the first line one
//! ends).

use std::borrow::Cow;

use super::diagnostics::{Diagnostic, Severity};
use super::language::{ConfigLanguage, ParseError};
use super::paths::TextPosition;

/// `text` with every CRLF and lone CR read as LF.
pub(super) fn as_lf(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// The UTF-16 offset in `text` of one-based `line` and `column`, its lines
/// ended by LF, CRLF or a lone CR: a column past the line's end is the
/// line's end (before its break), a line past the text the text's end.
pub(super) fn offset_at(text: &str, line: usize, column: usize) -> usize {
    let mut offset = 0;
    let mut rest = text;
    for _ in 1..line.max(1) {
        let Some(at) = rest.find(['\r', '\n']) else {
            return text.encode_utf16().count();
        };
        let ending = if rest[at..].starts_with("\r\n") { 2 } else { 1 };
        offset += rest[..at].encode_utf16().count() + ending;
        rest = &rest[at + ending..];
    }
    let content = &rest[..rest.find(['\r', '\n']).unwrap_or(rest.len())];
    offset
        + content
            .chars()
            .take(column.saturating_sub(1))
            .map(char::len_utf16)
            .sum::<usize>()
}

/// The error of `original` when it holds a lone CR its language does not
/// read as a line break (it parses with LF there): the reader's message,
/// at the end of the first line a lone CR ends.
pub(super) fn rejected_lone_cr(
    behavior: &dyn ConfigLanguage,
    original: &str,
) -> Option<ParseError> {
    let bytes = original.as_bytes();
    let at =
        (0..bytes.len()).find(|at| bytes[*at] == b'\r' && bytes.get(at + 1) != Some(&b'\n'))?;
    let error = behavior.parse(&original.replace("\r\n", "\n")).err()?;
    behavior.parse(&as_lf(original)).ok()?;
    let position = TextPosition::from_offset(original, original[..at].encode_utf16().count());
    Some(ParseError {
        message: error.message,
        line: Some(position.line),
        column: Some(position.column),
    })
}

/// [`rejected_lone_cr`] as a diagnostic.
pub(super) fn rejected_lone_cr_problem(
    behavior: &dyn ConfigLanguage,
    original: &str,
) -> Option<Diagnostic> {
    let error = rejected_lone_cr(behavior, original)?;
    Some(Diagnostic {
        line: error.line,
        column: error.column,
        end_column: error.column.map(|column| column + 1),
        message: error.message,
        severity: Severity::Error,
        ..Diagnostic::default()
    })
}

#[cfg(test)]
mod tests {
    use super::{as_lf, offset_at};

    #[test]
    fn every_break_reads_as_lf_and_offsets_count_the_text_given() {
        assert_eq!(as_lf("a\rb\r\nc\r"), "a\nb\nc\n");
        assert!(matches!(as_lf("a\nb"), std::borrow::Cow::Borrowed(_)));
        let text = "ab\rcd\r\nef\ngh";
        assert_eq!(offset_at(text, 2, 2), 4);
        assert_eq!(offset_at(text, 3, 1), 7);
        assert_eq!(offset_at(text, 3, 9), 9, "past the end: the line's end");
        assert_eq!(offset_at(text, 4, 3), 12);
        assert_eq!(offset_at(text, 9, 1), 12);
    }
}
