//! Comments and strings written over several lines (XML `<!-- … -->`, HCL
//! `/* … */`, a custom language's `'''` strings). A language highlights one
//! line at a time and colors a comment from its opening to the line end;
//! the lines after are colored here, from the comments
//! ([`ConfigLanguage::comments`]) and strings
//! ([`ConfigLanguage::multiline_strings`]) the language finds in the whole
//! document, so a comment stays a comment on every line, folded or
//! scrolled out of view or not.
//!
//! [`ConfigLanguage::comments`]: super::super::language::ConfigLanguage::comments
//! [`ConfigLanguage::multiline_strings`]: super::super::language::ConfigLanguage::multiline_strings

use std::ops::Range;

use super::super::languages::Language;
use super::{Token, TokenKind, Tokens};

/// The lines after the first of a comment or string written over several
/// lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentLines {
    /// The lines (0-based) past the comment's first, its last included.
    pub(crate) lines: Range<usize>,
    /// The byte of its last line where the comment ends.
    pub(crate) end: usize,
    /// How it is colored: [`TokenKind::Comment`] or [`TokenKind::String`].
    pub(crate) kind: TokenKind,
}

/// The lines `comments` (byte ranges of `source`, in order) cover past
/// their first; a comment on one line covers none. One pass over the text,
/// however many comments.
#[cfg(test)]
pub(crate) fn comment_lines(source: &str, comments: &[Range<usize>]) -> Vec<CommentLines> {
    spanning_lines(source, comments, &[])
}

/// [`comment_lines`] for the `comments` and the multi-line `strings` of
/// `source` (byte ranges, each list in order): one list in order, a range
/// starting inside an earlier one left out.
pub(crate) fn spanning_lines(
    source: &str,
    comments: &[Range<usize>],
    strings: &[Range<usize>],
) -> Vec<CommentLines> {
    let mut ranges: Vec<(&Range<usize>, TokenKind)> = comments
        .iter()
        .map(|range| (range, TokenKind::Comment))
        .chain(strings.iter().map(|range| (range, TokenKind::String)))
        .collect();
    ranges.sort_by_key(|(range, _)| range.start);
    let mut reach = 0;
    ranges.retain(|(range, _)| {
        let keep = range.start >= reach;
        if keep {
            reach = range.end;
        }
        keep
    });
    let bytes = source.as_bytes();
    // The line and line start at `at`, moved forward only.
    let (mut at, mut line, mut line_start) = (0, 0, 0);
    let mut advance = |to: usize| {
        let to = to.min(bytes.len());
        if to < at {
            return (line, line_start);
        }
        for (offset, byte) in bytes[at..to].iter().enumerate() {
            if *byte == b'\n' {
                line += 1;
                line_start = at + offset + 1;
            }
        }
        at = to;
        (line, line_start)
    };
    let mut out = Vec::new();
    for (comment, kind) in ranges {
        let (first, _) = advance(comment.start);
        let (last, last_start) = advance(comment.end);
        if last > first {
            out.push(CommentLines {
                lines: first + 1..last + 1,
                end: comment.end - last_start,
                kind,
            });
        }
    }
    out
}

/// How many bytes from its start of line `index` a comment (or string)
/// opened on an earlier line covers (`usize::MAX`: all of it), and its kind.
pub(crate) fn covered(comments: &[CommentLines], index: usize) -> Option<(usize, TokenKind)> {
    let at = comments.partition_point(|comment| comment.lines.end <= index);
    let comment = comments
        .get(at)
        .filter(|comment| comment.lines.contains(&index))?;
    let end = if index + 1 == comment.lines.end {
        comment.end
    } else {
        usize::MAX
    };
    Some((end, comment.kind))
}

/// The tokens of `line` when its first `end` bytes close a comment (or a
/// string: `kind`) opened on an earlier line: those are `kind`, the rest is
/// highlighted by `language`.
pub(crate) fn tokens_after_comment(
    language: Language,
    line: &str,
    end: usize,
    kind: TokenKind,
) -> Vec<Token> {
    let mut end = end.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut tokens = Tokens::new();
    tokens.push(0, end, kind);
    let mut tokens = tokens.finish();
    tokens.extend(
        language
            .behavior()
            .highlight(&line[end..])
            .into_iter()
            .map(|token| Token::new(token.start + end, token.end + end, token.kind)),
    );
    tokens
}

#[cfg(test)]
mod tests;
