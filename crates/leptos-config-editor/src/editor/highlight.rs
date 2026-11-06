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

/// Renders `line` with its tokens. Tokens are applied in order; anything out
/// of order, overlapping, or out of range is rendered plain rather than
/// dropped, so the overlay always shows every character.
pub(super) fn render(line: &str, tokens: &[Token]) -> AnyView {
    let mut parts = Vec::with_capacity(tokens.len() * 2 + 1);
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
            parts.push(view! { <span>{line[at..token.start].to_string()}</span> }.into_any());
        }
        let class = token.kind.class();
        parts.push(
            view! { <span class=class>{line[token.start..token.end].to_string()}</span> }
                .into_any(),
        );
        at = token.end;
    }
    if at < line.len() || parts.is_empty() {
        parts.push(view! { <span>{line[at..].to_string()}</span> }.into_any());
    }
    view! { <span>{parts}</span> }.into_any()
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
