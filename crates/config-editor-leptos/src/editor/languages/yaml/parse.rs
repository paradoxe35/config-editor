//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

pub(super) fn yaml_parse(source: &str) -> Result<Value, ParseError> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    // Read through `Parsed`, which keeps `.inf` and `.nan` (a plain `Value`
    // would turn them into `null`).
    noyalib::from_str_with_config(source, &parser_config)
        .map(|parsed: super::super::super::codec::non_finite::Parsed| parsed.0)
        .map_err(|error| {
        let line_count = source.split('\n').count().max(1);
        let location = error.location();
        let line = location
            .as_ref()
            .map(|location| location.line().min(line_count));
        let column = location.as_ref().map(|location| location.column().max(1));
        let mut message = error.to_string();
        if let Some(location) = location
            && location.line() > line_count
        {
            message = message.replace(
                &format!("at line {} column {}", location.line(), location.column()),
                "at end of input",
            );
        }
        ParseError {
            message,
            line,
            column,
        }
    })
}

/// Maps every mapping key and sequence item in the document to its source line.
/// Paths are the same JSON-pointer segments the schema validator reports, so a
/// nested `listen` is never confused with a root `listen`.
pub(super) fn yaml_path_lines(source: &str) -> Vec<(Vec<String>, usize, usize, usize)> {
    let mut entries = Vec::new();
    let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
    let mut counts: std::collections::HashMap<(Vec<String>, usize), usize> =
        std::collections::HashMap::new();
    // The indentation a block scalar's text runs past: its lines are text,
    // not keys or items.
    let mut block_owner: Option<usize> = None;
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some(owner) = block_owner {
            if trimmed.is_empty() || indent > owner {
                continue;
            }
            block_owner = None;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut rest = trimmed;
        let mut content_indent = indent;

        // Each dash opens an item, several on one line in a compact nested
        // sequence (`- - a`).
        let mut array_item_path = None;
        let mut item_indent = indent;
        while rest == "-" || rest.starts_with("- ") {
            if let Some(path) = array_item_path.take() {
                // An outer item of a compact sequence: on this line.
                let column = line[..item_indent].chars().count() + 1;
                entries.push((path, index + 1, column, column + 1));
            }
            while stack.last().is_some_and(|(level, _)| *level >= item_indent) {
                stack.pop();
            }
            let parent = stack
                .last()
                .map(|(_, path)| path.clone())
                .unwrap_or_default();
            let count = counts.entry((parent.clone(), item_indent)).or_default();
            let mut item_path = parent;
            item_path.push(count.to_string());
            *count += 1;
            let after_dash = rest.strip_prefix('-').unwrap_or_default();
            let content = after_dash.trim_start();
            content_indent = item_indent + 1 + after_dash.len() - content.len();
            array_item_path = Some(item_path.clone());
            stack.push((item_indent, item_path));
            rest = content;
            if is_block_scalar(without_node_properties(code_before_comment(rest).trim())) {
                block_owner = Some(item_indent);
            }
            item_indent = content_indent;
        }

        if rest.is_empty() {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                entries.push((path, index + 1, column, column + 1));
            }
            continue;
        }
        let Some(colon) = key_colon(rest) else {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                let end_column = column + rest.chars().count();
                entries.push((path, index + 1, column, end_column));
            }
            continue;
        };
        let key_raw = &rest[..colon];
        let key = key_raw.trim().trim_matches(['\'', '"']);
        if key.is_empty() {
            continue;
        }
        while stack
            .last()
            .is_some_and(|(level, _)| *level >= content_indent)
        {
            stack.pop();
        }
        let mut path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        path.push(key.to_string());
        let column = line[..content_indent].chars().count() + 1;
        let end_column = column + key_raw.trim_end().chars().count();
        entries.push((path.clone(), index + 1, column, end_column));
        let value = without_node_properties(rest[colon + 1..].trim());
        if value.is_empty() || value.starts_with('#') {
            stack.push((content_indent, path));
        } else if is_block_scalar(code_before_comment(value).trim()) {
            block_owner = Some(content_indent);
        }
    }
    entries
}

/// The colon after a mapping key in `rest` (a line's content after its
/// dashes): the first one followed by a space or the line's end, after a
/// quoted key's closing quote, before a comment; none in a scalar such as
/// `12:30` or a flow collection.
fn key_colon(rest: &str) -> Option<usize> {
    if without_node_properties(rest).starts_with(['[', '{']) {
        return None;
    }
    let from = match rest.chars().next() {
        Some(quote @ ('"' | '\'')) => {
            let mut escaped = false;
            let close = rest[1..].char_indices().find(|&(_, character)| {
                let closes = character == quote && !escaped;
                escaped = quote == '"' && character == '\\' && !escaped;
                closes
            })?;
            1 + close.0 + 1
        }
        _ => 0,
    };
    let code = code_before_comment(rest);
    code.get(from..)?
        .match_indices(':')
        .map(|(at, _)| from + at)
        .find(|&at| code[at + 1..].chars().next().is_none_or(char::is_whitespace))
}

/// `value` without the anchors and tags before it (`&base`, `!!map`): a
/// key whose value is only those opens a block below it.
fn without_node_properties(mut value: &str) -> &str {
    while value.starts_with(['&', '!']) {
        let end = value.find([' ', '\t']).unwrap_or(value.len());
        value = value[end..].trim_start();
    }
    value
}

/// Span of a key written inside an inline `{ ... }` mapping. The key is found
/// on its parent mapping's line as a whole word after the parent's colon.
pub(super) fn flow_key_span(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let (key, parent) = wanted.split_last()?;
    if parent.is_empty() || key.is_empty() {
        return None;
    }
    let (_, parent_line, _, _) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| candidate == parent)?;
    let text = source.lines().nth(parent_line.checked_sub(1)?)?;
    let value_begin = text.find(':')? + 1;
    let is_word = |character: char| character.is_alphanumeric() || matches!(character, '_' | '-');
    let mut search = value_begin;
    while let Some(relative) = text[search..].find(key.as_str()) {
        let start = search + relative;
        let end = start + key.len();
        let whole_word = !text[..start].chars().next_back().is_some_and(is_word)
            && !text[end..].chars().next().is_some_and(is_word);
        if whole_word {
            let column = text[..start].chars().count() + 1;
            return Some((parent_line, column, column + key.chars().count()));
        }
        search = end;
    }
    None
}

pub(super) fn yaml_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = crate::editor::language::pointer_segments(path);
    let key = wanted.last()?.clone();
    if key.is_empty() {
        return None;
    }

    if let Some((_, line, column, end_column)) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
    {
        return Some((line, column, end_column));
    }

    if let Some(span) = flow_key_span(source, &wanted) {
        return Some(span);
    }

    // Last resort for paths the scanner cannot represent: first line with this key.
    source.lines().enumerate().find_map(|(index, line)| {
        let indent = line.len() - line.trim_start().len();
        let (candidate, _) = line.trim_start().split_once(':')?;
        if candidate.trim_matches(['\'', '"']) != key {
            return None;
        }
        let candidate_offset = line[indent..].find(candidate)?;
        let column = line[..indent].chars().count()
            + line[indent..indent + candidate_offset].chars().count()
            + 1;
        Some((index + 1, column, column + candidate.chars().count()))
    })
}

pub(super) fn yaml_value_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = crate::editor::language::pointer_segments(path);
    if let Some(span) = sequence_item_span(source, &wanted) {
        return Some(span);
    }
    // Only block-structured entries have a value slot on their line. Inline
    // mapping keys resolve through `yaml_span_for_path`, which points at the key.
    let (line_number, key_column) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
        .map(|(_, line, column, _)| (line, column))?;
    let line = source.lines().nth(line_number.saturating_sub(1))?;
    // The key starts after the line's indentation and dashes (ASCII), so its
    // column is its byte offset; the value ends before a trailing comment.
    let key_start = line.char_indices().nth(key_column - 1)?.0;
    let colon = key_start + key_colon(&line[key_start..])?;
    let line = line_comment_start(line).map_or(line, |start| &line[..start]);
    let raw_value = line.get(colon + 1..)?;
    let value_start = raw_value.len() - raw_value.trim_start().len();
    let value = raw_value.trim();
    let column = line[..colon + 1 + value_start].chars().count() + 1;
    if value.is_empty() || value.starts_with('#') {
        return Some((line_number, column, column + 1));
    }
    // A block's text is indented past its key.
    if let Some(end) = block_scalar_end(source, line_number, value, key_start) {
        return Some((line_number, column, end));
    }
    Some((line_number, column, column + value.chars().count()))
}

/// The end column of a block scalar (`|`, `>-`, `&a |2`) opened by `value`
/// on line `line_number`: it runs through its lines below indented past
/// `owner_indent` (its key's or its dash's column), the column counting on
/// past this line, as a heredoc's does. `None` when `value` opens none.
fn block_scalar_end(source: &str, line_number: usize, value: &str, owner_indent: usize) -> Option<usize> {
    if !is_block_scalar(&value[node_properties_len(value)..]) {
        return None;
    }
    let full_line = source.lines().nth(line_number - 1)?;
    let mut end_chars = full_line.chars().count() + 1;
    let mut text_end = full_line.chars().count();
    for next in source.lines().skip(line_number) {
        let indent = next.len() - next.trim_start().len();
        if !next.trim().is_empty() && indent <= owner_indent {
            break;
        }
        end_chars += next.chars().count();
        if !next.trim().is_empty() {
            text_end = end_chars;
        }
        end_chars += 1;
    }
    Some(text_end + 1)
}

/// The value of block sequence item `wanted` (`.../certificates/0`): the text
/// after its `- `, on its own line or after an outer item's dash (`- - a`).
pub(super) fn sequence_item_span(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let (last, _) = wanted.split_last()?;
    last.parse::<usize>().ok()?;
    let (line_number, column) = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| candidate.as_slice() == wanted)
        .map(|(_, line, column, _)| (line, column))?;
    let line = source.lines().nth(line_number - 1)?;
    // After the line's indentation and dashes (ASCII): the column is a byte
    // offset. An item holding a map (`- k: v`) has a key there.
    let start = line.char_indices().nth(column - 1).map_or(line.len(), |(at, _)| at);
    let content = &line[start..];
    if key_colon(content).is_some() || content.starts_with("- ") || content == "-" {
        return None;
    }
    let dash = line[..start].trim_end().len().checked_sub(1)?;
    let value = line_comment_start(content).map_or(content, |comment| &content[..comment]).trim_end();
    if let Some(end) = block_scalar_end(source, line_number, value, dash) {
        return Some((line_number, column, end));
    }
    Some((line_number, column, column + value.chars().count().max(1)))
}

pub(super) fn first_yaml_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}

/// `value` with YAML merge keys applied (`<<: *defaults`, or a list of
/// them): the merged maps' keys added where the map does not write its own,
/// the `<<` entry gone.
pub(super) fn with_merge_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let own: Vec<String> = map.keys().filter(|key| *key != "<<").cloned().collect();
            let mut merged = serde_json::Map::new();
            for (key, child) in map {
                if key != "<<" {
                    merged.insert(key, with_merge_keys(child));
                    continue;
                }
                // The merged keys go where `<<` is, so the map reads in the
                // order it is written.
                let sources = match child {
                    Value::Object(source) => vec![Value::Object(source)],
                    Value::Array(items) => items,
                    other => {
                        merged.insert(key, other);
                        continue;
                    }
                };
                for source in sources {
                    if let Value::Object(source) = with_merge_keys(source) {
                        for (name, value) in source {
                            if !own.contains(&name) && !merged.contains_key(&name) {
                                merged.insert(name, value);
                            }
                        }
                    }
                }
            }
            Value::Object(merged)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(with_merge_keys).collect()),
        other => other,
    }
}
