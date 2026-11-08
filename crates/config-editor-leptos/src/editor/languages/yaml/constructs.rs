//! Nodes written over several lines: quoted scalars, plain scalars going
//! on below their key, and flow collections whose entries go on lines of
//! their own. The editor colors their lines past the first from these, the
//! inside of a flow collection by [`flow_tokens`].

use std::ops::Range;

use super::super::super::highlight::{MultilineConstruct, TokenKind, Tokens};
use super::colors::scalar_kind;
use super::editing::code_before_comment;
use super::parse::plain_scalar_bodies;

/// The quoted scalars, plain scalars and flow collections `source` writes
/// over several lines, in order. `string_bodies` are the lines inside block
/// and quoted scalars (`ConfigLanguage::string_body_lines`).
pub(super) fn multiline_constructs(
    source: &str,
    string_bodies: &[Range<usize>],
) -> Vec<MultilineConstruct> {
    let mut found = Vec::new();
    let flow_lines = scan(source, string_bodies, &mut found);
    let mut taken = string_bodies.to_vec();
    taken.extend(flow_lines);
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    for lines in plain_scalar_bodies(source, &taken) {
        // From the end of the key's line through the text of the last line.
        let start = line_starts[lines.start] - 1;
        let last_start = line_starts[lines.end - 1];
        let last_end = line_starts.get(lines.end).map_or(source.len(), |next| next - 1);
        let end = last_start + code_before_comment(&source[last_start..last_end]).trim_end().len();
        found.push(MultilineConstruct::colored(start..end, TokenKind::Value));
    }
    found.sort_by_key(|construct| construct.range.start);
    found.dedup_by_key(|construct| construct.range.start);
    found
}

/// Whether a node can start after `previous`, the last character before it
/// on its line other than blanks (`None`: none).
fn node_starts_after(previous: Option<char>) -> bool {
    previous.is_none_or(|previous| matches!(previous, '[' | '{' | ',' | ':' | '-' | '?'))
}

/// Reads `source` for quoted scalars and flow collections over lines,
/// pushing them to `found`, skipping block and quoted scalars' `bodies`
/// outside flow collections; returns the lines of the flow collections.
fn scan(
    source: &str,
    bodies: &[Range<usize>],
    found: &mut Vec<MultilineConstruct>,
) -> Vec<Range<usize>> {
    let mut flow_lines = Vec::new();
    let (mut depth, mut outer_start, mut outer_line) = (0usize, 0, 0);
    let (mut line, mut line_start) = (0usize, 0);
    let mut previous: Option<char> = None;
    let mut index = 0;
    while index < source.len() {
        if index == line_start && depth == 0 && bodies.iter().any(|lines| lines.contains(&line)) {
            index = source[index..].find('\n').map_or(source.len(), |at| index + at);
            continue;
        }
        let character = source[index..].chars().next().unwrap_or('\0');
        match character {
            '\n' => {
                line += 1;
                line_start = index + 1;
                if depth == 0 {
                    previous = None;
                }
                index += 1;
                continue;
            }
            '#' if index == line_start
                || source[..index].ends_with(|before: char| before.is_whitespace()) =>
            {
                index = source[index..].find('\n').map_or(source.len(), |at| index + at);
                continue;
            }
            '"' | '\'' if node_starts_after(previous) => {
                let end = quoted_end(source, index);
                let text = &source[index..end];
                if text.contains('\n') {
                    found.push(MultilineConstruct::colored(index..end, TokenKind::String));
                    line += text.matches('\n').count();
                    line_start = index + text.rfind('\n').map_or(0, |at| at + 1);
                }
                previous = Some(character);
                index = end;
                continue;
            }
            '[' | '{' if depth > 0 || node_starts_after(previous) => {
                if depth == 0 {
                    (outer_start, outer_line) = (index, line);
                }
                depth += 1;
            }
            ']' | '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 && line > outer_line {
                    found.push(MultilineConstruct::structure(outer_start..index + 1));
                    flow_lines.push(outer_line..line + 1);
                }
            }
            _ => {}
        }
        if !character.is_whitespace() {
            previous = Some(character);
        }
        index += character.len_utf8();
    }
    if depth > 0 && line > outer_line {
        found.push(MultilineConstruct::structure(outer_start..source.len()));
        flow_lines.push(outer_line..line + 1);
    }
    flow_lines
}

/// The end of the quoted scalar opening at `start` (through its closing
/// quote, over lines), or the end of `text` when it stays open.
fn quoted_end(text: &str, start: usize) -> usize {
    let quote = text.as_bytes()[start];
    let bytes = text.as_bytes();
    let mut index = start + 1;
    while index < text.len() {
        match bytes[index] {
            b'\\' if quote == b'"' => index += 2,
            b'\'' if quote == b'\'' && bytes.get(index + 1) == Some(&b'\'') => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    text.len()
}

/// Colors `text`, a flow collection or the inside of one, starting at
/// byte `offset` of its line: keys, quoted scalars (one left open runs to
/// the end), plain scalars, brackets, commas and colons, comments.
pub(super) fn flow_tokens(text: &str, offset: usize, tokens: &mut Tokens) {
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let character = rest.chars().next().unwrap_or('\0');
        let start = index;
        let after_blank = index == 0 || text[..index].ends_with(|before: char| before.is_whitespace());
        match character {
            '#' if after_blank => {
                tokens.push(offset + start, offset + text.len(), TokenKind::Comment);
                return;
            }
            '"' | '\'' => {
                index = quoted_end(text, index).min(text.len());
                // A quoted key: `{"a b": 1}`.
                let after = text[index..].trim_start_matches([' ', '\t']);
                let kind = match after.strip_prefix(':') {
                    Some(rest) if separates(rest) => TokenKind::Key,
                    _ => TokenKind::String,
                };
                tokens.push(offset + start, offset + index, kind);
                continue;
            }
            '[' | ']' | '{' | '}' | ',' => {
                tokens.push(offset + start, offset + start + 1, TokenKind::Punctuation);
            }
            ':' if separates(&rest[1..]) => {
                tokens.push(offset + start, offset + start + 1, TokenKind::Punctuation);
            }
            _ if character.is_whitespace() => {}
            _ => {
                let mut end = index;
                while end < text.len() {
                    let next = text[end..].chars().next().unwrap_or('\0');
                    let ends = matches!(next, '[' | ']' | '{' | '}' | ',')
                        || (next == ':' && separates(&text[end + 1..]))
                        || (next == '#' && text[..end].ends_with(|before: char| before.is_whitespace()));
                    if ends {
                        break;
                    }
                    end += next.len_utf8();
                }
                let word = text[start..end].trim_end();
                let word_end = start + word.len();
                let kind = if text[end..].starts_with(':') {
                    TokenKind::Key
                } else {
                    scalar_kind(word)
                };
                tokens.push(offset + start, offset + word_end, kind);
                index = end;
                continue;
            }
        }
        index += character.len_utf8();
    }
}

/// Whether a `:` followed by `after` separates a key from its value in a
/// flow collection.
fn separates(after: &str) -> bool {
    after.is_empty() || after.starts_with(|next: char| next.is_whitespace() || matches!(next, ',' | ']' | '}'))
}
