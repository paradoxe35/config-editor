//! Writing a value in place of one the host wrote, in the host's style:
//! plain stays plain, quotes keep their kind, block scalars stay blocks.

use super::*;

use super::super::super::codec::non_finite;

/// `value` written where `written` was, after `prefix` on its line (its key
/// or dash, which a block scalar's indentation indicator counts from; empty
/// when unknown), inside a flow collection or not. Anything the style
/// cannot hold is written as [`yaml_value`] does.
pub(super) fn yaml_value_like(language: &YamlBehavior, value: &Value, written: &str, prefix: &str, in_flow: bool) -> String {
    // A value made empty again is written as YAML writes an empty value
    // (`key:`), unless the host spells its nulls out.
    if value.is_null() && !matches!(written.trim(), "null" | "Null" | "NULL" | "~") {
        return String::new();
    }
    let Value::String(text) = value else {
        return yaml_value(value);
    };
    if non_finite::name(value).is_some() {
        return yaml_value(value);
    }
    // The text as written (a string holding a non-finite number's form
    // carries one more mark in the model).
    let text = non_finite::written(text);
    let written = written.trim_start();
    let reads_back = |candidate: &str| {
        // As a block value, and inside a flow collection where it is in one.
        language.parse(&format!("k: {candidate}\n")).ok() == Some(serde_json::json!({ "k": value }))
            && (!in_flow || language.parse(&format!("k: [{candidate}]\n")).ok() == Some(serde_json::json!({ "k": [value] })))
    };
    // Line separators and what YAML takes only escaped are written in
    // double quotes.
    let breaks = text.chars().any(super::suggestions::needs_yaml_escape);
    match written.chars().next() {
        _ if breaks && !written.starts_with('"') => yaml_value(value),
        Some('\'') if !text.contains('\n') => format!("'{}'", text.replace('\'', "''")),
        Some('"') => super::super::escapes::like_written(
            super::suggestions::yaml_quoted(text),
            written,
            super::super::escapes::Astral::Long,
        ),
        Some('|' | '>') => super::block_scalar::block_scalar(language, prefix, written, text).unwrap_or_else(|| yaml_value(value)),
        Some(_) if in_flow && !super::plain::can_be_plain_in_flow(text) => yaml_value(value),
        Some(_) if super::plain::can_be_plain(text) && reads_back(text) => text.to_string(),
        _ => yaml_value(value),
    }
}

/// Whether the value at `range` of `source` is inside a flow collection:
/// after an opening bracket on its line, before a `,` or closing bracket,
/// or on a line after one ending with `[`, `{` or `,` (a collection over
/// lines).
pub(super) fn in_flow_collection(source: &str, range: std::ops::Range<usize>) -> bool {
    let line_start = source[..range.start].rfind('\n').map_or(0, |index| index + 1);
    let before = &source[line_start..range.start];
    let (mut depth, mut quote) = (0i32, None);
    for character in before.chars() {
        match (quote, character) {
            (Some(open), _) if open == character => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '[' | '{') => depth += 1,
            (None, ']' | '}') => depth -= 1,
            _ => {}
        }
    }
    let after = code_before_comment(source[range.end..].split('\n').next().unwrap_or("")).trim_start();
    let previous_line = source[..line_start]
        .trim_end_matches('\n')
        .rsplit('\n')
        .map(|line| code_before_comment(line).trim_end())
        .find(|line| !line.is_empty());
    depth > 0 || after.starts_with([',', ']', '}']) || previous_line.is_some_and(|line| line.ends_with(['[', '{', ',']))
}

/// Whether `source` writes a sequence inside a sequence item on the item's
/// line (`- - a`).
pub(super) fn writes_compact_sequences(source: &str) -> bool {
    source.lines().any(|line| line.trim_start().starts_with("- - ") || line.trim_start() == "- -")
}

/// `text` (as the YAML writer writes it) with each sequence item that holds
/// a sequence written compact: a dash alone (`  -`) followed by its first
/// item one level in (`    - a`) is `  - - a`.
pub(super) fn compact_sequences(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut index = 0;
    while index < lines.len() {
        let mut line = lines[index].to_string();
        index += 1;
        // Dashes alone (`  -`, `  - -`) take the next line's item when it
        // sits one level past the last dash.
        while !line.trim().is_empty()
            && line.trim().chars().all(|character| matches!(character, '-' | ' '))
            && let Some(next) = lines.get(index)
            && next.len() - next.trim_start().len() == line.trim_end().len() + 1
            && next.trim_start().starts_with('-')
        {
            line = format!("{} {}", line.trim_end(), next.trim_start());
            index += 1;
        }
        out.push(line);
    }
    out.join("\n")
}

/// `code` with written forms YAML reads the same made equal: a list item's
/// dash however far its text is (`-   a`), an anchor or tag on a value
/// (`pool: &p`, `n: !!int "7"`), an empty value and its null (`key: ~`).
pub(super) fn comparable_yaml_line(code: &str) -> std::borrow::Cow<'_, str> {
    let mut line = code.to_string();
    if let Some(rest) = line.strip_prefix('-').filter(|rest| rest.starts_with([' ', '\t'])) {
        line = format!("- {}", rest.trim_start());
    }
    if let Some((at, length)) = [": &", "- &", ": !", "- !"].iter().find_map(|marker| line.find(marker).map(|at| (at, marker.len()))) {
        let name_end = line[at + length..]
            .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-' | '!' | ':' | '.' | '/')))
            .map_or(line.len(), |offset| at + length + offset);
        if name_end > at + length {
            line = format!("{}{}", &line[..at + length - 1], line[name_end..].trim_start()).trim_end().to_string();
        }
    }
    for null in [": null", ": ~"] {
        if let Some(key) = line.strip_suffix(null) {
            line = format!("{key}:");
            break;
        }
    }
    if line == code { std::borrow::Cow::Borrowed(code) } else { std::borrow::Cow::Owned(line) }
}

/// The length of the anchors and tags (`&p `, `!!map `) before a value.
pub(super) fn node_properties_len(text: &str) -> usize {
    let mut length = 0;
    while text[length..].starts_with(['&', '!']) {
        let token = text[length..].find([' ', '\t']).unwrap_or(text.len() - length);
        let rest = &text[length + token..];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if spaces == 0 {
            return 0;
        }
        length += token + spaces;
    }
    length
}
