//! Syntax highlighting as data: a language reports [`Token`]s for a line and
//! the core renders them, so languages stay free of view code.

use leptos::prelude::*;

/// What a highlighted run of text is. Each kind maps to one CSS class
/// (`config-editor__key`, `config-editor__string`, …), so every theme colors
/// every language the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// A mapping key, attribute name, or element name.
    Key,
    /// A quoted string.
    String,
    /// An unquoted scalar value.
    Value,
    /// A number, boolean, or null.
    Literal,
    /// A comment.
    Comment,
    /// Structural punctuation: brackets, separators, markup delimiters.
    Punctuation,
}

impl TokenKind {
    pub(super) fn class(self) -> &'static str {
        match self {
            TokenKind::Key => "config-editor__key",
            TokenKind::String => "config-editor__string",
            TokenKind::Value => "config-editor__value",
            TokenKind::Literal => "config-editor__literal",
            TokenKind::Comment => "config-editor__comment",
            TokenKind::Punctuation => "config-editor__punctuation",
        }
    }
}

/// A highlighted run of one line: byte offsets `start..end` into the line.
/// Text no token covers renders plain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Byte offset where the run starts.
    pub start: usize,
    /// Byte offset just past the run.
    pub end: usize,
    /// How the run is colored.
    pub kind: TokenKind,
}

impl Token {
    /// A token over `start..end`.
    pub fn new(start: usize, end: usize, kind: TokenKind) -> Self {
        Token { start, end, kind }
    }
}

/// Collects tokens while a highlighter walks a line, in order and without
/// overlaps.
#[derive(Debug, Default)]
pub struct Tokens {
    tokens: Vec<Token>,
}

impl Tokens {
    /// An empty collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a run; empty runs are ignored.
    pub fn push(&mut self, start: usize, end: usize, kind: TokenKind) {
        if start < end {
            self.tokens.push(Token::new(start, end, kind));
        }
    }

    /// The collected tokens.
    pub fn finish(self) -> Vec<Token> {
        self.tokens
    }
}

/// Renders `line` with its tokens, and each byte range of `marks` (a
/// read-only value) wrapped in a span of its class. Tokens are applied in
/// order; anything out of order, overlapping, or out of range is rendered
/// plain rather than dropped, so the overlay always shows every character.
pub(super) fn render(
    line: &str,
    tokens: &[Token],
    marks: &[(std::ops::Range<usize>, &'static str)],
) -> AnyView {
    let runs = runs_of(line, tokens);
    let mut parts = Vec::with_capacity(runs.len() + marks.len() * 2);
    // Plain text gets no class attribute at all: rows are many.
    let piece = |from: usize, to: usize, class: Option<&'static str>| match class {
        Some(class) => view! { <span class=class>{line[from..to].to_string()}</span> }.into_any(),
        None => view! { <span>{line[from..to].to_string()}</span> }.into_any(),
    };
    for (start, end, class) in runs {
        if marks.is_empty() {
            parts.push(piece(start, end, class));
            continue;
        }
        let mut cuts = vec![start, end];
        for (range, _) in marks {
            for at in [range.start, range.end] {
                if start < at && at < end && line.is_char_boundary(at) {
                    cuts.push(at);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        if cuts.len() == 1 {
            cuts.push(end);
        }
        for pair in cuts.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            let mark = marks
                .iter()
                .find(|(range, _)| from < to && range.start <= from && to <= range.end)
                .map(|(_, mark)| *mark);
            parts.push(match mark {
                Some(mark) => view! { <span class=mark>{piece(from, to, class)}</span> }.into_any(),
                None => piece(from, to, class),
            });
        }
    }
    view! { <span>{parts}</span> }.into_any()
}

/// `line` cut into runs, each with its token's class (`None` for plain
/// text); always at least one run.
fn runs_of(line: &str, tokens: &[Token]) -> Vec<(usize, usize, Option<&'static str>)> {
    let mut runs = Vec::with_capacity(tokens.len() * 2 + 1);
    let mut at = 0;
    for token in tokens {
        let valid = token.start >= at
            && token.end <= line.len()
            && line.is_char_boundary(token.start)
            && line.is_char_boundary(token.end);
        if !valid {
            continue;
        }
        if token.start > at {
            runs.push((at, token.start, None));
        }
        runs.push((token.start, token.end, Some(token.kind.class())));
        at = token.end;
    }
    if at < line.len() || runs.is_empty() {
        runs.push((at, line.len(), None));
    }
    runs
}

/// Test helper for language highlighters: the line split into `(kind, text)`
/// runs, with plain text as `None`, after checking the tokens are ordered and
/// in range.
#[cfg(test)]
#[allow(dead_code, reason = "used by the language test modules")]
pub(super) fn runs(line: &str, tokens: &[Token]) -> Vec<(Option<TokenKind>, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    for token in tokens {
        assert!(
            token.start >= at,
            "tokens overlap or are unordered: {tokens:?}"
        );
        assert!(
            token.end <= line.len(),
            "token past the line end: {token:?}"
        );
        if token.start > at {
            out.push((None, line[at..token.start].to_string()));
        }
        out.push((Some(token.kind), line[token.start..token.end].to_string()));
        at = token.end;
    }
    if at < line.len() {
        out.push((None, line[at..].to_string()));
    }
    out
}
