//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// The unclosed string quote at `cursor` on its line, if the caret is inside a
/// string. Drives the auto-pairing guard.
pub(super) fn open_string_quote(source: &str, cursor: usize) -> Option<char> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let mut in_string = false;
    let mut escaped = false;
    for character in source[line_start..byte].chars() {
        if !in_string {
            in_string = character == '"';
        } else if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            in_string = false;
        }
    }
    in_string.then_some('"')
}

pub(super) fn highlight(line: &str) -> Vec<highlight::Token> {
    let mut tokens = Tokens::new();
    for token in tokenize(line) {
        let kind = match token.kind {
            Kind::Punct(_) => TokenKind::Punctuation,
            Kind::Str { .. } if line[token.end..].trim_start().starts_with(':') => TokenKind::Key,
            Kind::Str { .. } => TokenKind::String,
            Kind::Word => {
                let text = &line[token.start..token.end];
                if matches!(text, "true" | "false" | "null") || text.parse::<f64>().is_ok() {
                    TokenKind::Literal
                } else {
                    TokenKind::Value
                }
            }
        };
        tokens.push(token.start, token.end, kind);
    }
    tokens.finish()
}
