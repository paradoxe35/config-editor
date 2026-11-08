//! Values written over several lines: `"""` / `'''` strings, and arrays
//! (and inline tables) whose elements go on lines of their own. The editor
//! colors their lines past the first from these, the inside of an array by
//! [`value_tokens`].

use super::super::super::highlight::{MultilineConstruct, TokenKind, Tokens};
use super::colors::scalar_kind;

/// The strings and arrays (or inline tables) `source` writes over several
/// lines, in order.
pub(super) fn multiline_constructs(source: &str) -> Vec<MultilineConstruct> {
    let mut found = Vec::new();
    // The open brackets of the value being read, the outermost's start.
    let mut depth = 0usize;
    let mut outer_start = 0;
    // Whether a `[` or `{` here opens a value: after `=`, or in one.
    let mut after_equals = false;
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        let character = rest.chars().next().unwrap_or('\0');
        let start = index;
        match character {
            '#' => {
                index = line_end(source, index);
                continue;
            }
            '"' | '\'' => {
                index = string_end(source, index);
                if source[start..index].contains('\n') {
                    found.push(MultilineConstruct::colored(start..index, TokenKind::String));
                }
                after_equals = false;
                continue;
            }
            '=' => after_equals = true,
            '[' | '{' if depth > 0 || after_equals => {
                if depth == 0 {
                    outer_start = start;
                }
                depth += 1;
            }
            ']' | '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 && source[outer_start..=start].contains('\n') {
                    found.push(MultilineConstruct::structure(outer_start..start + 1));
                }
            }
            '\n' if depth == 0 => after_equals = false,
            _ if character.is_whitespace() => {}
            _ => after_equals = false,
        }
        index += character.len_utf8();
    }
    if depth > 0 && source[outer_start..].contains('\n') {
        found.push(MultilineConstruct::structure(outer_start..source.len()));
    }
    found.sort_by_key(|construct| construct.range.start);
    found
}

/// The end of the line holding `index`, without its line break.
fn line_end(source: &str, index: usize) -> usize {
    source[index..]
        .find('\n')
        .map_or(source.len(), |offset| index + offset)
}

/// The end of the string opening at `start` (a `"""` / `'''` one through
/// its closing delimiter, over lines; a one-line one through its closing
/// quote or to the line end), or the end of `source` when it stays open.
pub(super) fn string_end(source: &str, start: usize) -> usize {
    let rest = &source[start..];
    let quote = rest.as_bytes()[0];
    let basic = quote == b'"';
    let triple = if basic { "\"\"\"" } else { "'''" };
    let (from, multiline) = if rest.starts_with(triple) {
        (start + 3, true)
    } else {
        (start + 1, false)
    };
    let mut index = from;
    let bytes = source.as_bytes();
    while index < source.len() {
        match bytes[index] {
            b'\\' if basic => index += 2,
            b'\n' if !multiline => return index,
            byte if byte == quote && (!multiline || source[index..].starts_with(triple)) => {
                if !multiline {
                    return index + 1;
                }
                // Up to two quotes more belong to the string (`""""`).
                let run = source[index..].bytes().take_while(|next| *next == quote).count();
                return index + run.min(5);
            }
            _ => index += 1,
        }
    }
    source.len()
}

/// Colors `text`, a value or the inside of one, starting at byte `offset`
/// of its line: strings (one left open runs to the end), numbers and
/// booleans, brackets and commas, keys of inline tables, comments.
pub(super) fn value_tokens(text: &str, offset: usize, tokens: &mut Tokens) {
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let character = rest.chars().next().unwrap_or('\0');
        let start = index;
        match character {
            '#' => {
                tokens.push(offset + start, offset + text.len(), TokenKind::Comment);
                return;
            }
            '"' | '\'' => {
                index = string_end(text, index).min(text.len());
                // A quoted key of an inline table: `{ "a b" = 1 }`.
                let kind = if text[index..].trim_start().starts_with('=') {
                    TokenKind::Key
                } else {
                    TokenKind::String
                };
                tokens.push(offset + start, offset + index, kind);
                continue;
            }
            '[' | ']' | '{' | '}' | ',' => {
                tokens.push(offset + start, offset + start + 1, TokenKind::Punctuation);
            }
            _ if character.is_whitespace() || character == '=' => {}
            _ => {
                let end = rest
                    .find(|next: char| {
                        next.is_whitespace() || matches!(next, '[' | ']' | '{' | '}' | ',' | '=' | '#' | '"' | '\'')
                    })
                    .map_or(text.len(), |at| index + at);
                let word = &text[start..end];
                let kind = if text[end..].trim_start().starts_with('=') {
                    TokenKind::Key
                } else {
                    scalar_kind(word)
                };
                tokens.push(offset + start, offset + end, kind);
                index = end;
                continue;
            }
        }
        index += character.len_utf8();
    }
}
