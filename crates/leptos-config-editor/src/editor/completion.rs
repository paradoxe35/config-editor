use super::commands::indent_unit;
use super::diagnostics::SchemaBundle;
use super::keyboard::{replace_selection, utf16_to_byte_index};
use super::metrics::read_editor_metrics;
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq, Eq)]
enum SchemaPathSegment {
    Key(String),
    ArrayItem(usize),
}

#[derive(Clone, PartialEq)]
pub(super) struct Completion {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) type_label: String,
    pub(super) insert: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) cursor_offset: usize,
    pub(super) is_value: bool,
    /// Character-index ranges of `label` covered by the typed query, for
    /// highlighting the match in the suggestion row. Empty when the query is
    /// empty or when scoring fell through without tracking positions.
    pub(super) matched: Vec<(usize, usize)>,
}

#[derive(Clone, PartialEq, Eq)]
struct CompletionContext {
    path: Vec<SchemaPathSegment>,
    typed: String,
    replace_start: usize,
    replace_end: usize,
    value_key: Option<String>,
    value_needs_space: bool,
    is_array_item: bool,
    base_indent: String,
}

fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root, 0)
        .into_iter()
        .filter_map(|branch| {
            if branch.get("enum").is_some() {
                return Some("enum".to_string());
            }
            match branch.get("type") {
                Some(Value::String(value)) => Some(value.clone()),
                Some(Value::Array(values)) => {
                    let value = values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    (!value.is_empty()).then_some(value)
                }
                _ => None,
            }
        })
        .collect();
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

fn type_includes(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(schema_type)) => schema_type == expected,
        Some(Value::Array(types)) => types.iter().any(|schema_type| schema_type == expected),
        _ => false,
    }
}

fn yaml_key_for_completion(key: &str) -> Option<String> {
    let plain_key = !key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
        && !matches!(key.to_ascii_lowercase().as_str(), "true" | "false" | "null");
    if plain_key {
        Some(key.to_string())
    } else {
        serde_json::to_string(key).ok()
    }
}

pub(super) fn yaml_value(value: &Value) -> String {
    match value {
        Value::String(value) if is_plain_identifier(value) => value.clone(),
        Value::String(value) => serde_json::to_string(value).unwrap_or_else(|_| value.clone()),
        _ => value.to_string(),
    }
}

/// Enum-style words (`udp`, `least_connections`) read as plain YAML scalars, the way
/// the YAML language server writes them. Anything that YAML could read as a number,
/// boolean, or null, or that contains punctuation, stays quoted.
fn is_plain_identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && text.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
        && !matches!(
            text.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n"
        )
}

fn schema_default<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("default"))
}

fn schema_const<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("const"))
}

fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("properties").is_some()
                || branch.get("patternProperties").is_some()
                || branch.get("additionalProperties").is_some()
                || type_includes(branch, "object")
        })
}

fn array_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("items").is_some()
                || branch.get("prefixItems").is_some()
                || type_includes(branch, "array")
        })
}

fn append_required_properties(
    schema: &Value,
    root: &Value,
    child_indent: &str,
    unit: &str,
    base_offset: usize,
    output: &mut String,
    cursor_offset: &mut Option<usize>,
) -> bool {
    let Some(branch) = object_branch(schema, root) else {
        return false;
    };
    let required = branch.get("required").and_then(Value::as_array);
    let properties = branch.get("properties").and_then(Value::as_object);
    let Some(required) = required else {
        return false;
    };
    let nested_indent = format!("{child_indent}{unit}");
    let mut appended = false;
    for required_key in required.iter().filter_map(Value::as_str) {
        let Some(property) = properties.and_then(|properties| properties.get(required_key)) else {
            continue;
        };
        let Some(yaml_key) = yaml_key_for_completion(required_key) else {
            continue;
        };
        output.push_str(child_indent);
        output.push_str(&yaml_key);
        output.push(':');
        if let Some(default) = schema_default(property, root) {
            output.push(' ');
            output.push_str(&yaml_value(default));
            output.push('\n');
        } else if object_branch(property, root).is_some() {
            let mut children = String::new();
            if append_required_properties(
                property,
                root,
                &nested_indent,
                unit,
                base_offset + output.encode_utf16().count() + 1,
                &mut children,
                cursor_offset,
            ) {
                output.push('\n');
                output.push_str(&children);
            } else {
                output.push_str(" {}\n");
            }
        } else {
            output.push(' ');
            if cursor_offset.is_none() {
                *cursor_offset = Some(base_offset + output.encode_utf16().count());
            }
            output.push('\n');
        }
        appended = true;
    }
    appended
}

fn property_completion_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    let yaml_key = yaml_key_for_completion(key)?;
    let child_indent = format!("{base_indent}{unit}");
    let mut insert = format!("{yaml_key}:");
    if let Some(value) = schema_const(property, root) {
        insert.push(' ');
        insert.push_str(&yaml_value(value));
    } else if array_branch(property, root).is_some() {
        let default = schema_default(property, root);
        if let Some(default) = default.filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            insert.push(' ');
            insert.push_str(&yaml_value(default));
        } else {
            insert.push('\n');
            insert.push_str(&child_indent);
            insert.push_str("- ");
        }
    } else if let Some(branch) = object_branch(property, root) {
        let default = schema_default(property, root);
        if let Some(default) =
            default.filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
        {
            insert.push(' ');
            insert.push_str(&yaml_value(default));
            let cursor = insert.encode_utf16().count();
            return Some((insert, cursor));
        }
        let mut cursor_offset = None;
        let mut children = String::new();
        if append_required_properties(
            branch,
            root,
            &child_indent,
            unit,
            insert.encode_utf16().count() + 1,
            &mut children,
            &mut cursor_offset,
        ) {
            insert.push('\n');
            insert.push_str(&children);
        } else if branch.get("properties").is_some()
            || branch.get("patternProperties").is_some()
            || branch.get("additionalProperties").is_some()
        {
            insert.push('\n');
            insert.push_str(&child_indent);
        } else {
            insert.push_str(" {}");
        }
        let cursor = cursor_offset.unwrap_or_else(|| insert.encode_utf16().count());
        return Some((insert, cursor));
    } else if let Some(default) = schema_default(property, root) {
        insert.push(' ');
        insert.push_str(&yaml_value(default));
    } else {
        insert.push(' ');
    }
    let cursor = insert.encode_utf16().count();
    Some((insert, cursor))
}

fn mapping_colon(line: &str) -> Option<usize> {
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
            (None, ':')
                if line[index + 1..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

/// Length of the scalar at the start of `text`, stopping before an inline comment.
fn value_token_len(text: &str) -> usize {
    let mut end = text.len();
    for (index, character) in text.char_indices() {
        let comment_start =
            character == '#' && (index == 0 || text[..index].ends_with([' ', '\t']));
        if comment_start {
            end = index;
            break;
        }
    }
    text[..end].trim_end().len()
}

fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let before_cursor = &source[..cursor];
    let current_line_start = before_cursor.rfind('\n').map_or(0, |index| index + 1);
    let current_line = &source[current_line_start..];
    let leading_whitespace =
        current_line.len() - current_line.trim_start_matches([' ', '\t']).len();
    let indent = leading_whitespace;
    let prefix = &current_line[indent..];

    let mut ancestors: Vec<(usize, SchemaPathSegment)> = Vec::new();
    let mut item_counts = std::collections::BTreeMap::<usize, usize>::new();
    for line in source[..current_line_start].lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let line_indent = line.len() - trimmed.len();
        let is_array_item = trimmed == "-" || trimmed.starts_with("- ");
        let (mapping, mapping_indent) = if is_array_item {
            while ancestors
                .last()
                .is_some_and(|(level, _)| *level >= line_indent)
            {
                ancestors.pop();
            }
            item_counts.retain(|level, _| *level <= line_indent);
            let index = item_counts.entry(line_indent).or_default();
            ancestors.push((line_indent, SchemaPathSegment::ArrayItem(*index)));
            *index += 1;
            (
                trimmed.strip_prefix('-').unwrap_or_default().trim_start(),
                line_indent + 2,
            )
        } else {
            item_counts.retain(|level, _| *level <= line_indent);
            (trimmed, line_indent)
        };
        let Some(colon) = mapping_colon(mapping) else {
            continue;
        };
        let key = mapping[..colon].trim().trim_matches(['\'', '"']);
        if key.is_empty() {
            continue;
        }
        while ancestors
            .last()
            .is_some_and(|(level, _)| *level >= mapping_indent)
        {
            ancestors.pop();
        }
        if mapping[colon + 1..].trim().is_empty() {
            ancestors.push((mapping_indent, SchemaPathSegment::Key(key.to_string())));
        }
    }
    while ancestors.last().is_some_and(|(level, _)| *level >= indent) {
        ancestors.pop();
    }
    let mut path: Vec<_> = ancestors.into_iter().map(|(_, key)| key).collect();
    let current_is_array_item = prefix == "-" || prefix.starts_with("- ");
    let (line_content, content_offset) = if current_is_array_item {
        let index = item_counts.get(&indent).copied().unwrap_or_default();
        path.push(SchemaPathSegment::ArrayItem(index));
        let after_dash = prefix.strip_prefix('-').unwrap_or_default();
        let content = after_dash.trim_start();
        let offset = 1 + after_dash.len() - content.len();
        (content, offset)
    } else {
        (prefix, 0)
    };
    let candidate_colon = mapping_colon(line_content);
    let content_start = indent + content_offset;
    let cursor_in_content = cursor
        .saturating_sub(current_line_start)
        .saturating_sub(content_start)
        .min(line_content.len());
    // A colon to the right of the caret still belongs to the key being edited;
    // treating it as a value separator can produce a reversed string slice.
    let colon = candidate_colon.filter(|colon| *colon < cursor_in_content);
    // Only the text before the caret decides the value position. A trailing
    // newline (for example from an expanded required property) must not move the
    // insertion point past the end of the value.
    let value_start = colon.map(|colon| {
        let after_colon = &line_content[colon + 1..];
        let before_caret = &after_colon[..cursor_in_content
            .saturating_sub(colon + 1)
            .min(after_colon.len())];
        before_caret.len() - before_caret.trim_start().len()
    });
    let value_key = colon
        .map(|colon| {
            line_content[..colon]
                .trim()
                .trim_matches(['\'', '"'])
                .to_string()
        })
        .filter(|key| !key.is_empty());
    let typed_start_byte = if let Some(colon) = colon {
        current_line_start + content_start + colon + 1 + value_start.unwrap_or_default()
    } else {
        current_line_start + content_start
    };
    // Suggestions match the text before the caret. Accepting one replaces the
    // whole scalar on this line, so editing in the middle of a value works too.
    let line_end = source[cursor..]
        .find('\n')
        .map_or(source.len(), |i| cursor + i);
    let (typed, replace_end_byte) = if let Some(colon) = colon {
        let value_begin = colon + 1 + value_start.unwrap_or_default();
        let value_begin_abs = current_line_start + content_start + value_begin;
        let typed_end = cursor_in_content.clamp(value_begin, line_content.len());
        let token_len = value_token_len(&source[value_begin_abs.min(line_end)..line_end]);
        (
            line_content[value_begin..typed_end].trim_end().to_string(),
            value_begin_abs + token_len,
        )
    } else {
        let key_end = candidate_colon.unwrap_or_else(|| value_token_len(line_content));
        let typed_end = cursor_in_content.min(key_end);
        let replace_end = candidate_colon.map_or(key_end, |colon| {
            let after_colon = &line_content[colon + 1..];
            colon + 1 + after_colon.len() - after_colon.trim_start().len()
        });
        (
            line_content[..typed_end].trim_end().to_string(),
            current_line_start + content_start + replace_end,
        )
    };
    let base_indent = format!("{}{}", &current_line[..indent], " ".repeat(content_offset));
    let value_needs_space = colon.is_some_and(|colon| cursor_in_content == colon + 1);
    CompletionContext {
        path,
        typed,
        replace_start: source[..typed_start_byte].encode_utf16().count(),
        replace_end: source[..replace_end_byte.min(source.len())]
            .encode_utf16()
            .count(),
        value_key,
        value_needs_space,
        is_array_item: current_is_array_item,
        base_indent,
    }
}

fn resolve_branches<'a>(schema: &'a Value, root: &'a Value, depth: usize) -> Vec<&'a Value> {
    if depth > 16 {
        return Vec::new();
    }
    let mut branches = Vec::new();
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(target) = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
    {
        branches.extend(resolve_branches(target, root, depth + 1));
    }
    branches.push(schema);
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(options) = schema.get(keyword).and_then(Value::as_array) {
            for option in options {
                branches.extend(resolve_branches(option, root, depth + 1));
            }
        }
    }
    branches
}

fn schema_at_path<'a>(root: &'a Value, path: &[SchemaPathSegment]) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for segment in path {
        let mut next = Vec::new();
        for current in schemas {
            for branch in resolve_branches(current, root, 0) {
                if let SchemaPathSegment::ArrayItem(index) = segment {
                    if let Some(prefix_items) = branch.get("prefixItems").and_then(Value::as_array)
                        && let Some(item_schema) = prefix_items.get(*index)
                    {
                        next.push(item_schema);
                    } else if let Some(items) =
                        branch.get("items").filter(|items| items.is_object())
                    {
                        next.push(items);
                    }
                    continue;
                }
                let SchemaPathSegment::Key(key) = segment else {
                    continue;
                };
                let mut matched = false;
                if let Some(property) = branch
                    .get("properties")
                    .and_then(|properties| properties.get(key))
                {
                    next.push(property);
                    matched = true;
                }
                if let Some(patterns) = branch.get("patternProperties").and_then(Value::as_object) {
                    for (pattern, property) in patterns {
                        if Regex::new(pattern).is_ok_and(|regex| regex.is_match(key)) {
                            next.push(property);
                            matched = true;
                        }
                    }
                }
                if !matched
                    && let Some(additional) = branch.get("additionalProperties")
                    && additional.is_object()
                {
                    next.push(additional);
                }
            }
        }
        schemas = next;
        if schemas.is_empty() {
            break;
        }
    }
    schemas
}

fn schema_value_candidates(branch: &Value) -> Vec<Value> {
    let mut values = branch
        .get("enum")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(value) = branch.get("const") {
        values.push(value.clone());
    }
    match branch.get("examples") {
        Some(Value::Array(examples)) => values.extend(examples.iter().cloned()),
        Some(example) => values.push(example.clone()),
        None => {}
    }
    if let Some(example) = branch.get("example") {
        values.push(example.clone());
    }
    if let Some(default) = branch.get("default") {
        values.push(default.clone());
    }
    if type_includes(branch, "boolean") {
        values.extend([Value::Bool(true), Value::Bool(false)]);
    }
    if type_includes(branch, "null") {
        values.push(Value::Null);
    }
    values.retain(|value| !value.is_array() && !value.is_object());
    values
}

fn path_with_key(path: &[SchemaPathSegment], key: &str) -> Vec<SchemaPathSegment> {
    let mut path = path.to_vec();
    path.push(SchemaPathSegment::Key(key.to_string()));
    path
}

fn value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn unquoted(typed: &str) -> &str {
    let typed = typed.trim_start();
    if let Some(quote) = typed
        .chars()
        .next()
        .filter(|quote| *quote == '"' || *quote == '\'')
    {
        let inner = &typed[quote.len_utf8()..];
        return inner.strip_suffix(quote).unwrap_or(inner);
    }
    typed
}

/// Ranks schema names against the typed prefix. Exact prefixes win, followed by
/// case-insensitive prefixes, word initials (for `max_connections`), substrings,
/// and finally an ordered subsequence match.
fn match_score(label: &str, typed: &str) -> Option<i64> {
    if typed.is_empty() {
        return Some(0);
    }
    let label_length = label.chars().count() as i64;
    if label.starts_with(typed) {
        return Some(1000 - label_length);
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return Some(900 - label_length);
    }
    let initials: String = label
        .split(['_', '-', '.', ' '])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_lowercase)
        .collect();
    if !initials.is_empty() && initials.starts_with(&typed_lower) {
        return Some(800 - label_length);
    }
    if let Some(index) = label_lower.find(&typed_lower) {
        return Some(700 - index as i64 * 2 - label_length / 4);
    }
    let mut remaining = label_lower.chars();
    let mut matched = 0usize;
    for wanted in typed_lower.chars() {
        if remaining.any(|candidate| candidate == wanted) {
            matched += 1;
        } else {
            return None;
        }
    }
    (matched == typed_lower.chars().count()).then_some(500 - label_length)
}

/// Character-index ranges of `label` covered by `typed`, mirroring the
/// `match_score` tiers so the suggestion row can highlight what matched:
/// prefixes cover the leading run, initials cover each consumed word start,
/// substrings cover the found run, and subsequence matches cover the greedy
/// per-character hits. Returns an empty vector when there is no match.
pub(super) fn match_ranges(label: &str, typed: &str) -> Vec<(usize, usize)> {
    if typed.is_empty() {
        return Vec::new();
    }
    let typed_len = typed.chars().count();
    if label.starts_with(typed) {
        return vec![(0, typed_len)];
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return vec![(0, typed_len)];
    }
    let mut initials: Vec<(usize, char)> = Vec::new();
    // Word separators are single ASCII bytes, so a running byte offset
    // tracks each part's start exactly.
    let mut byte_offset = 0;
    for part in label.split(['_', '-', '.', ' ']) {
        if let Some(first) = part.chars().next() {
            let char_offset = label[..byte_offset].chars().count();
            initials.extend(first.to_lowercase().map(|lower| (char_offset, lower)));
        }
        byte_offset += part.len() + 1;
    }
    if !initials.is_empty()
        && initials
            .iter()
            .map(|(_, initial)| *initial)
            .collect::<String>()
            .starts_with(&typed_lower)
    {
        return initials
            .into_iter()
            .take(typed_len)
            .map(|(offset, _)| (offset, offset + 1))
            .collect();
    }
    if let Some(byte_index) = label_lower.find(&typed_lower) {
        let start = label_lower[..byte_index].chars().count();
        return vec![(start, start + typed_len)];
    }
    let mut ranges = Vec::new();
    let mut remaining = label_lower.chars().enumerate();
    for wanted in typed_lower.chars() {
        let mut hit = None;
        for (index, candidate) in remaining.by_ref() {
            if candidate == wanted {
                hit = Some(index);
                break;
            }
        }
        match hit {
            Some(index) => ranges.push((index, index + 1)),
            None => return Vec::new(),
        }
    }
    ranges
}

/// Required keys sort above optional ones with the same match quality, without
/// overriding a stronger match tier.
const REQUIRED_BONUS: i64 = 40;

fn sorted_completions(mut scored: Vec<(i64, Completion)>) -> Vec<Completion> {
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.label.cmp(&right.1.label))
    });
    scored.dedup_by(|left, right| left.1.label == right.1.label);
    scored
        .into_iter()
        .map(|(_, completion)| completion)
        .collect()
}

/// Schemas often document a port as a string example (`"7835"`) next to an integer
/// branch. When the schema accepts numbers, insert the bare number.
fn numeric_example_as_number(value: Value, numeric_allowed: bool) -> Value {
    if numeric_allowed
        && let Value::String(text) = &value
        && let Ok(number) = text.parse::<i64>()
    {
        return Value::from(number);
    }
    value
}

fn value_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    leading: &str,
) -> Vec<Completion> {
    let query = unquoted(typed);
    let mut candidates = Vec::new();
    let numeric_allowed = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| type_includes(branch, "integer") || type_includes(branch, "number"));
    for schema in schemas {
        for branch in resolve_branches(schema, root, 0) {
            let description = branch
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let type_label = schema_type_label(branch, root, "value");
            for value in schema_value_candidates(branch) {
                let value = numeric_example_as_number(value, numeric_allowed);
                let label = value_label(&value);
                if let Some(score) = match_score(&label, query) {
                    let insert = format!("{leading}{}", yaml_value(&value));
                    candidates.push((
                        score,
                        Completion {
                            label: label.clone(),
                            detail: description.to_string(),
                            type_label: type_label.clone(),
                            cursor_offset: insert.encode_utf16().count(),
                            insert,
                            start,
                            end,
                            is_value: true,
                            matched: match_ranges(&label, query),
                        },
                    ));
                }
            }
        }
    }
    sorted_completions(candidates)
}

#[allow(clippy::too_many_arguments)]
fn property_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    existing: Option<&HashSet<String>>,
    base_indent: &str,
    unit: &str,
) -> Vec<Completion> {
    let mut candidates = Vec::new();
    for branch in schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
    {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let required: HashSet<&str> = branch
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for (key, property) in properties {
            if existing.is_some_and(|existing| existing.contains(key)) {
                continue;
            }
            let Some(score) = match_score(key, typed) else {
                continue;
            };
            let score = score
                + if required.contains(key.as_str()) {
                    REQUIRED_BONUS
                } else {
                    0
                };
            let Some((insert, cursor_offset)) =
                property_completion_insert(key, property, root, base_indent, unit)
            else {
                continue;
            };
            candidates.push((
                score,
                Completion {
                    label: key.clone(),
                    detail: property
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    type_label: schema_type_label(property, root, "property"),
                    insert,
                    start,
                    end,
                    cursor_offset,
                    is_value: false,
                    matched: match_ranges(key, typed),
                },
            ));
        }
    }
    sorted_completions(candidates)
}

fn existing_mapping_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    // A partially typed mapping key makes the whole YAML document temporarily
    // unparsable. Remove only the active line for the existing-key lookup so
    // keys already present elsewhere in this mapping remain filtered.
    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |newline| line_start + newline);
    let sanitized = format!("{}{}", &source[..line_start], &source[line_end..]);
    let document: Value = noyalib::from_str_with_config(&sanitized, &parser_config).ok()?;
    let mapping = path
        .iter()
        .try_fold(&document, |value, segment| match segment {
            SchemaPathSegment::Key(key) => value.get(key),
            SchemaPathSegment::ArrayItem(index) => value.get(index),
        })?
        .as_object()?;
    let current_line = source[..cursor_byte]
        .rsplit('\n')
        .next()
        .unwrap_or_default();
    let current_line = current_line.trim_start().trim_start_matches("- ");
    let current_key = mapping_colon(current_line).map(|colon| {
        current_line[..colon]
            .trim()
            .trim_matches(['\'', '"'])
            .to_string()
    });
    Some(
        mapping
            .keys()
            .filter(|key| current_key.as_deref() != Some(key.as_str()))
            .cloned()
            .collect(),
    )
}

pub(super) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    let cursor_byte = utf16_to_byte_index(source, cursor);
    let context = completion_context(source, cursor);
    let schemas = schema_at_path(&bundle.schema, &context.path);

    if let Some(key) = context.value_key.as_deref() {
        let value_schemas = schema_at_path(&bundle.schema, &path_with_key(&context.path, key));
        let leading = if context.value_needs_space { " " } else { "" };
        let mut options = value_completions(
            &value_schemas,
            &context.typed,
            context.replace_start,
            context.replace_end,
            &bundle.schema,
            leading,
        );
        if !force && unquoted(&context.typed).is_empty() {
            options.retain(|option| option.label != "null");
        }
        return options;
    }

    if context.is_array_item {
        let has_object_properties = schemas
            .iter()
            .flat_map(|schema| resolve_branches(schema, &bundle.schema, 0))
            .any(|branch| {
                branch.get("properties").is_some()
                    || branch.get("patternProperties").is_some()
                    || type_includes(branch, "object")
            });
        if !has_object_properties {
            return value_completions(
                &schemas,
                &context.typed,
                context.replace_start,
                context.replace_end,
                &bundle.schema,
                "",
            );
        }
    }

    let query = unquoted(&context.typed);
    if query.is_empty() && !force {
        return Vec::new();
    }
    if query.contains([':', '#', '\n']) {
        return Vec::new();
    }

    let existing_keys = existing_mapping_keys(source, cursor_byte, &context.path);
    let unit = indent_unit(source);
    property_completions(
        &schemas,
        query,
        context.replace_start,
        context.replace_end,
        &bundle.schema,
        existing_keys.as_ref(),
        &context.base_indent,
        &unit,
    )
}

/// Accepting a key keeps the list open so its child keys can be suggested. Accepting
/// a value ends the entry, so the list closes instead of offering the same value again.
pub(super) fn accept_reopens_suggestions(completion: &Completion, auto_trigger: bool) -> bool {
    auto_trigger && !completion.is_value
}

/// Whether Enter/Tab accept the highlighted completion instead of editing.
/// Accept only after explicit engagement: a `Ctrl+Space` (or accept-chained)
/// request, or navigation via arrow keys/hover. An auto-triggered list with
/// the highlight still on index 0 must not hijack Enter — the keystroke
/// belongs to newline+indent (Enter) or indent/outdent (Tab), as in VS Code.
pub(super) fn should_accept_with_keyboard(explicit_request: bool, navigated: bool) -> bool {
    explicit_request || navigated
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct KeyHover {
    pub(super) key: String,
    pub(super) start_column: usize,
    pub(super) type_label: String,
    pub(super) description: Option<String>,
    pub(super) default: Option<String>,
    pub(super) examples: Vec<String>,
    pub(super) required: bool,
}

/// Returns the mapping key under `column` (a character index) on the 0-based
/// `line_index`, together with its schema documentation.
pub(super) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
) -> Option<KeyHover> {
    let bundle = schema?;
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    let (key, start_column) = key_at_column(line, column)?;
    let cursor = source[..line_start + line.len()].encode_utf16().count();
    let context = completion_context(source, cursor);
    if context.value_key.as_deref() != Some(key.as_str()) {
        return None;
    }
    let root = &bundle.schema;
    let path = path_with_key(&context.path, &key);
    let schemas = schema_at_path(root, &path);
    if schemas.is_empty() {
        return None;
    }
    let branches: Vec<&Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .collect();
    let description = branches.iter().find_map(|branch| {
        branch
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let default = schemas
        .iter()
        .find_map(|schema| schema_default(schema, root))
        .map(value_label);
    let mut examples: Vec<String> = Vec::new();
    for branch in &branches {
        let values = match branch.get("examples") {
            Some(Value::Array(examples)) => examples.clone(),
            Some(example) => vec![example.clone()],
            None => Vec::new(),
        };
        for value in values.iter().chain(branch.get("example")) {
            let label = value_label(value);
            if !examples.contains(&label) {
                examples.push(label);
            }
        }
    }
    examples.truncate(5);
    let required = schema_at_path(root, &context.path)
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(&key)))
        });
    Some(KeyHover {
        key,
        start_column,
        type_label: schema_type_label(schemas[0], root, "property"),
        description,
        default,
        examples,
        required,
    })
}

fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let body = match trimmed.strip_prefix('-') {
        Some(rest) if rest.is_empty() || rest.starts_with([' ', '\t']) => {
            rest.trim_start_matches([' ', '\t'])
        }
        _ => trimmed,
    };
    if body.starts_with('#') {
        return None;
    }
    let colon = mapping_colon(body)?;
    let key_raw = &body[..colon];
    let key = key_raw.trim().trim_matches(['\'', '"']);
    if key.is_empty() {
        return None;
    }
    let body_start = line.len() - body.len();
    let start_column = line[..body_start].chars().count();
    let end_column = line[..body_start + key_raw.trim_end().len()]
        .chars()
        .count();
    (column >= start_column && column < end_column).then(|| (key.to_string(), start_column))
}

/// Whether an input event should open schema suggestions. New lines, deletions,
/// pastes, drag-and-drop, and undo/redo must not pop the list open; the editor
/// keeps suggestions from accepted completions and explicit `Ctrl+Space`.
pub(super) fn should_auto_trigger(input_type: &str, previous: &str, current: &str) -> bool {
    if current == previous || current.matches('\n').count() > previous.matches('\n').count() {
        return false;
    }
    let grew = current.len() > previous.len();
    let typed_over_selection = matches!(input_type, "insertText" | "insertCompositionText");
    if !grew && !typed_over_selection {
        return false;
    }
    !matches!(
        input_type,
        "insertLineBreak"
            | "insertParagraph"
            | "insertFromPaste"
            | "insertFromDrop"
            | "deleteContentBackward"
            | "deleteContentForward"
            | "deleteByCut"
            | "deleteByDrag"
            | "historyUndo"
            | "historyRedo"
    )
}

pub(super) fn apply_completion(source: &str, completion: &Completion) -> (String, usize) {
    let (updated, end_cursor) =
        replace_selection(source, completion.start, completion.end, &completion.insert);
    let end_offset = completion.insert.encode_utf16().count();
    (
        updated,
        end_cursor
            .saturating_sub(end_offset)
            .saturating_add(completion.cursor_offset),
    )
}

pub(super) fn scroll_completion_into_view(list: &web_sys::HtmlDivElement, index: usize) {
    let Some(option) = list
        .children()
        .item(index as u32)
        .and_then(|option| option.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    let top = option.offset_top();
    let bottom = top + option.offset_height();
    let scroll_top = list.scroll_top();
    let visible_bottom = scroll_top + list.client_height();
    if top < scroll_top {
        list.set_scroll_top(top);
    } else if bottom > visible_bottom {
        list.set_scroll_top(bottom - list.client_height());
    }
}

fn visual_column(text: &str) -> f64 {
    let mut column = 0.0;
    for character in text.chars() {
        if character == '\t' {
            column += 2.0 - (column % 2.0);
        } else {
            column += 1.0;
        }
    }
    column
}

/// Pure completion-popup placement: given the caret position in viewport
/// coordinates and the viewport size, decide above/below placement and clamp
/// to the viewport. No DOM access, so this is directly unit-testable.
pub(super) fn completion_popup_geometry(
    viewport_width: f64,
    viewport_height: f64,
    caret_x: f64,
    caret_y: f64,
    line_height: f64,
    option_count: usize,
) -> String {
    let width = 420.0_f64.min((viewport_width - 16.0).max(0.0));
    let left = caret_x.clamp(8.0, (viewport_width - width - 8.0).max(8.0));
    let ideal_height = (option_count as f64 * 44.0 + 2.0).min(320.0);
    let space_below = (viewport_height - caret_y - line_height - 8.0).max(0.0);
    let space_above = (caret_y - 8.0).max(0.0);
    let (top, available_height) = if space_below >= ideal_height || space_below >= space_above {
        (caret_y + line_height, space_below)
    } else {
        ((caret_y - ideal_height).max(8.0), space_above)
    };
    let max_height = ideal_height.min(available_height).max(0.0);
    format!(
        "position:fixed;left:{left:.1}px;top:{top:.1}px;width:{width:.1}px;max-height:{max_height:.1}px"
    )
}

pub(super) fn completion_popup_style(
    editor: &web_sys::HtmlTextAreaElement,
    source: &str,
    cursor: usize,
    option_count: usize,
) -> String {
    let Some(window) = web_sys::window() else {
        return "position:fixed;visibility:hidden".to_string();
    };
    let element: &web_sys::Element = editor.unchecked_ref();
    let rect = element.get_bounding_client_rect();
    let metrics = read_editor_metrics(element);
    let prefix = &source[..utf16_to_byte_index(source, cursor)];
    let (line, column) = prefix
        .rsplit_once('\n')
        .map_or((0, visual_column(prefix)), |(before, after)| {
            (before.matches('\n').count() + 1, visual_column(after))
        });
    let caret_x = rect.left() + metrics.padding_left + column * metrics.font_size * 0.6
        - editor.scroll_left() as f64;
    let caret_y = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - editor.scroll_top() as f64;
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or(1024.0);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(768.0);
    completion_popup_geometry(
        viewport_width,
        viewport_height,
        caret_x,
        caret_y,
        metrics.line_height,
        option_count,
    )
}

#[cfg(test)]
mod tests {
    use super::completion_popup_geometry;

    /// Regression test: the popup opens below the caret when there is room.
    /// Behavior mirror of `peek_geometry_places_popup_below` in `diff.rs`.
    #[test]
    fn completion_popup_opens_below_caret_when_space_allows() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 200.0, 21.6, 3);
        // 3 options: ideal height = 3*44+2 = 134. Below: 768-200-21.6-8 = 538.4.
        assert!(
            style.contains("top:221.6px"),
            "popup should sit just below the caret line, got: {style}"
        );
        assert!(
            style.contains("left:100.0px"),
            "popup should anchor at the caret x, got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup should fit all options, got: {style}"
        );
    }

    /// Regression test: near the bottom edge the popup flips above the caret
    /// instead of clipping off-viewport.
    #[test]
    fn completion_popup_flips_above_caret_near_viewport_bottom() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 700.0, 21.6, 3);
        // Below: 768-700-21.6-8 = 38.4 < 134; above: 700-8 = 692. So flip.
        assert!(
            style.contains("top:566.0px"),
            "popup should flip above the caret (700-134), got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup keeps full height when there is room above, got: {style}"
        );
    }

    /// Regression test: caret near the right edge clamps the popup so it
    /// stays inside the viewport (no horizontal overflow).
    #[test]
    fn completion_popup_clamps_to_viewport_right_edge() {
        let style = completion_popup_geometry(1024.0, 768.0, 1000.0, 200.0, 21.6, 3);
        // width = min(420, 1024-16) = 420; left clamped to 1024-420-8 = 596.
        assert!(
            style.contains("left:596.0px"),
            "popup should clamp to the right edge, got: {style}"
        );
        assert!(
            style.contains("width:420.0px"),
            "popup keeps full width when clamped, got: {style}"
        );
    }
}
