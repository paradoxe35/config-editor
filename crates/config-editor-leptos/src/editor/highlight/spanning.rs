//! Constructs written over several lines: comments (XML `<!-- … -->`, HCL
//! `/* … */`), strings (TOML `"""`, a YAML quoted scalar, a custom
//! language's `'''`), a YAML plain scalar's continuation lines, and
//! structure (a TOML array, a YAML flow collection, an XML start tag with
//! its attributes over lines). A language highlights one line at a time;
//! the lines after a construct's first are colored here, from the comments
//! ([`ConfigLanguage::comments`]) and constructs
//! ([`ConfigLanguage::multiline_constructs`]) the language finds in the
//! whole document, so every line of one is colored as what it is, folded
//! or scrolled out of view or not.
//!
//! [`ConfigLanguage::comments`]: super::super::language::ConfigLanguage::comments
//! [`ConfigLanguage::multiline_constructs`]: super::super::language::ConfigLanguage::multiline_constructs

use std::ops::Range;

use super::super::languages::Language;
use super::{ConstructKind, MultilineConstruct, Token, TokenKind, Tokens};

/// The lines after the first of a construct written over several lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpanLines {
    /// The lines (0-based) past the construct's first, its last included.
    pub(crate) lines: Range<usize>,
    /// The byte of its last line where the construct ends.
    pub(crate) end: usize,
    /// How it is colored: one kind of token (a comment, a string), or as
    /// structure (`kind` is then unused).
    pub(crate) kind: TokenKind,
}

/// The multi-line constructs of a document, by line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Spanning {
    /// Comments and strings: colored as one kind of token, none starting
    /// inside an earlier one, in order.
    pub(crate) colored: Vec<SpanLines>,
    /// Structure, colored by [`ConfigLanguage::highlight_continued`]: the
    /// outermost ones, in order.
    ///
    /// [`ConfigLanguage::highlight_continued`]: super::super::language::ConfigLanguage::highlight_continued
    pub(crate) structures: Vec<SpanLines>,
}

/// How a line starts when constructs opened on earlier lines reach it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineStart {
    /// How many bytes from its start (`usize::MAX`: all) a comment or
    /// string covers, and its kind.
    pub(crate) colored: Option<(usize, TokenKind)>,
    /// How many bytes from its start (`usize::MAX`: all) are inside
    /// structure.
    pub(crate) structure: Option<usize>,
}

impl LineStart {
    /// Whether nothing opened on an earlier line reaches the line.
    pub(crate) fn is_free(self) -> bool {
        self.colored.is_none() && self.structure.is_none()
    }
}

impl Spanning {
    /// The constructs of `source`: its `comments` and `constructs` (byte
    /// ranges, each list in order). One pass over the text, however many.
    pub(crate) fn of(
        source: &str,
        comments: &[Range<usize>],
        constructs: &[MultilineConstruct],
    ) -> Self {
        let mut colored: Vec<(Range<usize>, TokenKind)> = comments
            .iter()
            .map(|range| (range.clone(), TokenKind::Comment))
            .collect();
        let mut structures = Vec::new();
        for construct in constructs {
            match construct.kind {
                ConstructKind::Colored(kind) => colored.push((construct.range.clone(), kind)),
                ConstructKind::Structure => {
                    structures.push((construct.range.clone(), TokenKind::Punctuation));
                }
            }
        }
        Spanning {
            colored: span_lines(source, colored),
            structures: span_lines(source, structures),
        }
    }

    /// How line `index` starts.
    pub(crate) fn at(&self, index: usize) -> LineStart {
        LineStart {
            colored: covered(&self.colored, index),
            structure: covered(&self.structures, index).map(|(end, _)| end),
        }
    }
}

/// The lines `ranges` cover past their first, a range starting inside an
/// earlier one left out; a range on one line covers none.
fn span_lines(source: &str, mut ranges: Vec<(Range<usize>, TokenKind)>) -> Vec<SpanLines> {
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
    for (range, kind) in ranges {
        let (first, _) = advance(range.start);
        let (last, last_start) = advance(range.end);
        if last > first {
            out.push(SpanLines {
                lines: first + 1..last + 1,
                end: range.end - last_start,
                kind,
            });
        }
    }
    out
}

/// How many bytes from its start of line `index` a construct opened on an
/// earlier line covers (`usize::MAX`: all of it), and its kind.
pub(crate) fn covered(spans: &[SpanLines], index: usize) -> Option<(usize, TokenKind)> {
    let at = spans.partition_point(|span| span.lines.end <= index);
    let span = spans.get(at).filter(|span| span.lines.contains(&index))?;
    let end = if index + 1 == span.lines.end {
        span.end
    } else {
        usize::MAX
    };
    Some((end, span.kind))
}

/// The tokens of `line` when constructs opened on earlier lines reach it
/// (`start`): the bytes a comment or string covers are of its kind, those
/// inside structure are colored by the language's
/// [`highlight_continued`](super::super::language::ConfigLanguage::highlight_continued),
/// and the rest by its `highlight`.
pub(crate) fn tokens_after(language: Language, line: &str, start: LineStart) -> Vec<Token> {
    let boundary = |end: usize| {
        let mut end = end.min(line.len());
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        end
    };
    let mut tokens = Tokens::new();
    let colored_end = match start.colored {
        Some((end, kind)) => {
            let end = boundary(end);
            tokens.push(0, end, kind);
            end
        }
        None => 0,
    };
    let mut tokens = tokens.finish();
    let behavior = language.behavior();
    let mut shifted = |from: usize, found: Vec<Token>| {
        tokens.extend(
            found
                .into_iter()
                .map(|token| Token::new(token.start + from, token.end + from, token.kind)),
        );
    };
    let rest_start = match start.structure.map(boundary) {
        Some(end) if end > colored_end => {
            shifted(
                colored_end,
                behavior.highlight_continued(&line[colored_end..end]),
            );
            end
        }
        _ => colored_end,
    };
    if rest_start < line.len() {
        shifted(rest_start, behavior.highlight(&line[rest_start..]));
    }
    tokens
}

#[cfg(test)]
mod tests;
