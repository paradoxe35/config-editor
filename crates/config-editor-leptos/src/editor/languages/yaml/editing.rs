//! Enter, comments, auto-closing, lexical state, and syntax colors.

use super::*;

/// Finds the start of a YAML comment on a single line, ignoring `#` characters
/// that appear inside quoted scalars.
pub(super) fn line_comment_start(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            (Some('"'), '\\') => escaped = true,
            (Some(active), character) if active == character => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, '#')
                if index == 0
                    || line[..index]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_whitespace) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

pub(super) fn code_before_comment(line: &str) -> &str {
    line_comment_start(line).map_or(line, |comment| &line[..comment])
}

/// YAML forbids tabs in indentation, though the parser accepts some. Block
/// scalar content (after `key: |`) is text and may hold tabs.
pub(super) fn tab_indentation(source: &str) -> Option<ParseError> {
    let mut scalar_indent: Option<usize> = None;
    for (index, line) in source.lines().enumerate() {
        let content = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - content.len();
        if let Some(parent) = scalar_indent {
            if content.is_empty() || indent > parent {
                continue;
            }
            scalar_indent = None;
        }
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        if let Some(tab) = line[..indent].find('\t') {
            return Some(ParseError {
                message: "tabs are not allowed in YAML indentation; use spaces".to_string(),
                line: Some(index + 1),
                column: Some(tab + 1),
            });
        }
        let code = code_before_comment(content);
        let value = code.rsplit_once(": ").map_or(code.trim_start_matches("- "), |(_, value)| value);
        if is_block_scalar(value) {
            scalar_indent = Some(indent);
        }
    }
    None
}

pub(super) fn is_block_scalar(value: &str) -> bool {
    let mut characters = value.trim().chars();
    if !matches!(characters.next(), Some('|' | '>')) {
        return false;
    }
    characters.all(|character| character.is_ascii_digit() || matches!(character, '+' | '-'))
}

/// Whether a node's `value` (after `key:` or a list dash) continues on the
/// lines below: nothing yet, a block scalar (`|`, `>-`), node properties
/// alone or before a block scalar (`&anchor`, `!!map`, `&run |`), or an
/// unclosed flow collection.
fn opens_children(value: &str) -> bool {
    let tokens: Vec<&str> = value.split_whitespace().collect();
    let Some((last, properties)) = tokens.split_last() else {
        return true;
    };
    let is_property = |token: &&str| token.starts_with(['&', '!']);
    properties.iter().all(is_property)
        && (is_property(last) || is_block_scalar(last))
        || value.ends_with(['[', '{'])
}

pub(super) fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte_index = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte_index]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line = &source[line_start..byte_index];
    let indent: String = line
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let content = line.trim_start();
    let unit = indent_unit(source);

    if content.is_empty() {
        return format!("\n{indent}");
    }
    let code = code_before_comment(content).trim_end();
    if code.is_empty() {
        return format!("\n{indent}");
    }
    if code == "-" {
        return format!("\n{indent}- ");
    }
    if let Some(rest) = code.strip_prefix('-')
        && (rest.is_empty() || rest.starts_with([' ', '\t']))
    {
        let after_dash = rest.trim_end();
        if after_dash.trim().is_empty() {
            return format!("\n{indent}- ");
        }
        // Keep the document's dash spacing so nested lines align with the key.
        let content_offset =
            1 + after_dash.len() - after_dash.trim_start_matches([' ', '\t']).len();
        let item_indent = format!(" {}", &code[1..content_offset]);
        let content = &code[content_offset..];
        // A nested sequence (`- - a`) continues at its inner dash.
        if content == "-" || content.starts_with("- ") {
            return format!("\n{indent}{item_indent}- ");
        }
        // The item's content is below it (`- |`, `- &anchor`).
        if opens_children(content) && !content.ends_with(['[', '{']) {
            return format!("\n{indent}{item_indent}");
        }
        if let Some(colon) = mapping_colon(content) {
            let value = content[colon + 1..].trim();
            if opens_children(value) {
                return format!("\n{indent}{item_indent}{unit}");
            }
            // Mapping inside a sequence item: the next key stays in the same item.
            return format!("\n{indent}{item_indent}");
        }
        return format!("\n{indent}- ");
    }
    if let Some(colon) = mapping_colon(code) {
        let value = code[colon + 1..].trim();
        if opens_children(value) {
            return format!("\n{indent}{unit}");
        }
    }
    format!("\n{indent}")
}

pub(super) fn highlight(line: &str) -> Vec<Token> {
    let mut tokens = Tokens::new();
    let indent_len = line.len() - line.trim_start().len();
    let content = &line[indent_len..];
    if content.starts_with('#') {
        tokens.push(indent_len, line.len(), TokenKind::Comment);
        return tokens.finish();
    }

    let (dash_len, content) = content
        .strip_prefix("- ")
        .map_or((0, content), |content| (2, content));
    let content_start = indent_len + dash_len;
    if let Some((key, value)) = content.split_once(':')
        && !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        && (value.is_empty() || value.starts_with(' ') || value.starts_with('\t'))
    {
        // The dash before a key stays plain; only a bare item dash is punctuation.
        let key_end = content_start + key.len();
        tokens.push(content_start, key_end, TokenKind::Key);
        tokens.push(key_end, key_end + 1, TokenKind::Punctuation);
        highlight_value(&mut tokens, key_end + 1, value);
        return tokens.finish();
    }

    if dash_len > 0 {
        tokens.push(indent_len, content_start, TokenKind::Punctuation);
        highlight_value(&mut tokens, content_start, content);
    }
    tokens.finish()
}

/// Tokens for `value`, which starts at byte `offset` of the line.
pub(super) fn highlight_value(tokens: &mut Tokens, offset: usize, value: &str) {
    let scalar_start = offset + value.len() - value.trim_start().len();
    let (scalar, comment) = split_yaml_comment(value.trim_start());
    let scalar_end = scalar_start + scalar.len();
    tokens.push(scalar_start, scalar_end, scalar_kind(scalar));
    if let Some(comment) = comment {
        tokens.push(scalar_end, scalar_end + comment.len(), TokenKind::Comment);
    }
}

pub(super) fn scalar_kind(scalar: &str) -> TokenKind {
    let scalar = scalar.trim_end();
    if matches!(scalar, "true" | "false" | "null" | "~") || scalar.parse::<f64>().is_ok() {
        TokenKind::Literal
    } else if scalar.starts_with('\'') || scalar.starts_with('"') {
        TokenKind::String
    } else {
        TokenKind::Value
    }
}

pub(super) fn split_yaml_comment(value: &str) -> (&str, Option<&str>) {
    let characters: Vec<_> = value.char_indices().collect();
    let mut quote = None;
    let mut escaped = false;
    let mut position = 0;
    while position < characters.len() {
        let (index, character) = characters[position];
        if let Some(active_quote) = quote {
            if active_quote == '\'' && character == '\'' {
                if characters
                    .get(position + 1)
                    .is_some_and(|(_, next)| *next == '\'')
                {
                    position += 2;
                    continue;
                }
                quote = None;
            } else if active_quote == '"' && escaped {
                escaped = false;
            } else if active_quote == '"' && character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#'
            && (index == 0
                || value[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (&value[..index], Some(&value[index..]));
        }
        position += 1;
    }
    (value, None)
}

pub(super) fn yaml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let cursor = utf16_to_byte_index(source, cursor);
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..cursor];
    let characters: Vec<_> = line.char_indices().collect();
    let mut quote = None;
    let mut comment = false;
    let mut escaped = false;
    let mut index = 0;
    while index < characters.len() {
        let (byte_index, character) = characters[index];
        if comment {
            break;
        }
        if let Some(active_quote) = quote {
            if active_quote == '\'' && character == '\'' {
                if characters
                    .get(index + 1)
                    .is_some_and(|(_, next)| *next == '\'')
                {
                    index += 2;
                    continue;
                }
                quote = None;
            } else if active_quote == '"' && escaped {
                escaped = false;
            } else if active_quote == '"' && character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#'
            && (byte_index == 0
                || line[..byte_index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            comment = true;
        }
        index += 1;
    }
    (quote, comment)
}


// Conversion

/// Lines inside block scalars' bodies (`key: |` then its indented text).
pub(super) fn block_scalar_bodies(source: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut bodies = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(base) = block_scalar_opener(lines[index]) else {
            index += 1;
            continue;
        };
        let mut last_body = None;
        let mut next = index + 1;
        while next < lines.len() {
            let line = lines[next];
            let indent = line.len() - line.trim_start().len();
            if !line.trim().is_empty() {
                if indent <= base {
                    break;
                }
                last_body = Some(next);
            } else if indent > base {
                // Indentation alone, past the opener's: the body goes on.
                last_body = Some(next);
            }
            next += 1;
        }
        if let Some(last) = last_body {
            bodies.push(index + 1..last + 1);
            index = last + 1;
        } else {
            index += 1;
        }
    }
    bodies
}

/// The indentation a block scalar opened on `line` must exceed: its key's
/// column (`  key: |`, `- key: &a |`) or its dash's (`  - |`).
fn block_scalar_opener(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let code = code_before_comment(line.trim_start()).trim_end();
    let tokens: Vec<&str> = code.split_whitespace().collect();
    let (last, before) = tokens.split_last()?;
    if !is_block_scalar(last) {
        return None;
    }
    let properties = before.iter().rev().take_while(|token| token.starts_with(['&', '!'])).count();
    let owner = *before.get(before.len().checked_sub(properties + 1)?)?;
    if owner == "-" {
        return Some(indent);
    }
    if !owner.ends_with(':') {
        return None;
    }
    // The key's column: after any leading dashes (`- key: |`).
    let key_column = indent + code.len() - code.trim_start_matches(['-', ' ']).len();
    Some(key_column)
}
