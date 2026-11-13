//! A leading byte order mark (`U+FEFF`) is no part of the document: places
//! in the text are read in the text after it, then moved one column on in
//! the first line, and a save keeps it in front of what it writes.
#![cfg_attr(
    not(all(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )),
    allow(dead_code, reason = "not every language reads through each helper")
)]

use super::language::Span;

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

#[cfg_attr(
    not(all(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )),
    allow(dead_code, reason = "not every language reads through each helper")
)]
/// `span` of the text after the mark, placed in the text with it.
fn placed(span: Span, marked: bool) -> Span {
    if marked && span.line == 1 {
        Span {
            line: 1,
            start_column: span.start_column + 1,
            end_column: span.end_column + 1,
        }
    } else {
        span
    }
}

/// `read` of the text after a leading mark, placed in `source`.
pub(crate) fn span(source: &str, read: impl FnOnce(&str) -> Option<Span>) -> Option<Span> {
    let (text, marked) = split(source);
    read(text).map(|span| placed(span, marked))
}

#[cfg_attr(
    not(all(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )),
    allow(dead_code, reason = "not every language reads through each helper")
)]
/// [`span`] for spans read at once.
pub(crate) fn spans(
    source: &str,
    read: impl FnOnce(&str) -> Vec<Option<Span>>,
) -> Vec<Option<Span>> {
    let (text, marked) = split(source);
    read(text)
        .into_iter()
        .map(|span| span.map(|span| placed(span, marked)))
        .collect()
}
