//! Comments written over several lines (XML `<!-- … -->`, HCL `/* … */`).
//! A language highlights one line at a time and colors a comment from its
//! opening to the line end; the lines after are colored here, from the
//! comments the language finds in the whole document, so a comment stays a
//! comment on every line, folded or scrolled out of view or not.

use std::ops::Range;

use super::super::languages::Language;
use super::{Token, TokenKind, Tokens};

/// The lines after the first of a comment written over several lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentLines {
    /// The lines (0-based) past the comment's first, its last included.
    pub(crate) lines: Range<usize>,
    /// The byte of its last line where the comment ends.
    pub(crate) end: usize,
}

/// The lines `comments` (byte ranges of `source`, in order) cover past
/// their first; a comment on one line covers none. One pass over the text,
/// however many comments.
pub(crate) fn comment_lines(source: &str, comments: &[Range<usize>]) -> Vec<CommentLines> {
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
    for comment in comments {
        let (first, _) = advance(comment.start);
        let (last, last_start) = advance(comment.end);
        if last > first {
            out.push(CommentLines {
                lines: first + 1..last + 1,
                end: comment.end - last_start,
            });
        }
    }
    out
}

/// How many bytes from its start of line `index` a comment opened on an
/// earlier line covers (`usize::MAX`: all of it).
pub(crate) fn covered(comments: &[CommentLines], index: usize) -> Option<usize> {
    let at = comments.partition_point(|comment| comment.lines.end <= index);
    let comment = comments
        .get(at)
        .filter(|comment| comment.lines.contains(&index))?;
    Some(if index + 1 == comment.lines.end {
        comment.end
    } else {
        usize::MAX
    })
}

/// The tokens of `line` when its first `end` bytes close a comment opened on
/// an earlier line: those are comment, the rest is highlighted by
/// `language`.
pub(crate) fn tokens_after_comment(language: Language, line: &str, end: usize) -> Vec<Token> {
    let mut end = end.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut tokens = Tokens::new();
    tokens.push(0, end, TokenKind::Comment);
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
