//! YAML text-layer behavior.
//!
//! Decisions: duplicate keys are a parse error; comments are `#` line comments,
//! toggled per line. Parsing goes through `noyalib`, but completion context and
//! key/value spans come from line-based scans so they work while the document
//! is mid-edit and unparsable.

use super::super::commands::indent_unit;
use super::super::highlight::{Token, TokenKind, Tokens};
use super::super::inline::{
    InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_sibling_keys, inline_value_len,
    statement_start, entry_follows,
};
use super::super::completion::{array_branch, object_branch, schema_const, schema_default};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::{bracket_newline, CompletionContext, Newline, EntryPosition, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment,
};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct YamlBehavior;


impl ConfigLanguage for YamlBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "YAML",
            icon: "Y",
            extensions: &["yaml", "yml"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        if let Some(error) = tab_indentation(source) {
            return Err(error);
        }
        yaml_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        yaml_span_for_path(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        yaml_value_span_for_path(source, pointer).map(Span::from)
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_yaml_span(source).map(Span::from)
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        completion_context(source, cursor)
    }

    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        existing_mapping_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        if context.position == EntryPosition::Inline {
            return flow_property_insert(key, property, root);
        }
        property_completion_insert(key, property, root, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        yaml_value(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        hover_context_offset(line, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        // Between flow brackets (`[|]`) the collection opens across lines.
        let byte = utf16_to_byte_index(source, cursor);
        let before = source[..byte].trim_end_matches([' ', '\t']);
        let after = source[byte..].trim_start_matches([' ', '\t']);
        if before.ends_with(['[', '{']) && after.starts_with([']', '}']) {
            return bracket_newline(source, cursor, &[('{', '}'), ('[', ']')], code_before_comment);
        }
        Newline::new(newline_with_indent(source, cursor))
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_yaml_line_comments(source, selection_start, selection_end)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = yaml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        noyalib::to_string(value).map_err(|error| error.to_string())
    }
}

fn yaml_parse(source: &str) -> Result<Value, ParseError> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    noyalib::from_str_with_config(source, &parser_config).map_err(|error| {
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
fn yaml_path_lines(source: &str) -> Vec<(Vec<String>, usize, usize, usize)> {
    let mut entries = Vec::new();
    let mut stack: Vec<(usize, Vec<String>)> = Vec::new();
    let mut counts: std::collections::HashMap<(Vec<String>, usize), usize> =
        std::collections::HashMap::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let mut rest = trimmed;
        let mut content_indent = indent;

        let mut array_item_path = None;
        if rest == "-" || rest.starts_with("- ") {
            while stack.last().is_some_and(|(level, _)| *level >= indent) {
                stack.pop();
            }
            let parent = stack
                .last()
                .map(|(_, path)| path.clone())
                .unwrap_or_default();
            let count = counts.entry((parent.clone(), indent)).or_default();
            let mut item_path = parent;
            item_path.push(count.to_string());
            *count += 1;
            let after_dash = rest.strip_prefix('-').unwrap_or_default();
            let content = after_dash.trim_start();
            content_indent = indent + 1 + after_dash.len() - content.len();
            array_item_path = Some(item_path.clone());
            stack.push((indent, item_path));
            rest = content;
        }

        if rest.is_empty() {
            if let Some(path) = array_item_path {
                let column = line[..content_indent].chars().count() + 1;
                entries.push((path, index + 1, column, column + 1));
            }
            continue;
        }
        let Some(colon) = rest.find(':') else {
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
        let value = rest[colon + 1..].trim();
        if value.is_empty() || value.starts_with('#') {
            stack.push((content_indent, path));
        }
    }
    entries
}

/// Span of a key written inside an inline `{ ... }` mapping. The key is found
/// on its parent mapping's line as a whole word after the parent's colon.
fn flow_key_span(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
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

fn yaml_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = super::super::language::pointer_segments(path);
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

fn yaml_value_span_for_path(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = super::super::language::pointer_segments(path);
    // Only block-structured entries have a value slot on their line. Inline
    // mapping keys resolve through `yaml_span_for_path`, which points at the key.
    let line_number = yaml_path_lines(source)
        .into_iter()
        .find(|(candidate, _, _, _)| *candidate == wanted)
        .map(|(_, line, _, _)| line)?;
    let line = source.lines().nth(line_number.saturating_sub(1))?;
    // The value ends before a trailing `# comment`.
    let line = line_comment_start(line).map_or(line, |start| &line[..start]);
    let (key, raw_value) = line.split_once(':')?;
    let value_start = raw_value.len() - raw_value.trim_start().len();
    let value = raw_value.trim();
    let column = key.chars().count() + raw_value[..value_start].chars().count() + 2;
    if value.is_empty() || value.starts_with('#') {
        return Some((line_number, column, column + 1));
    }

    Some((line_number, column, column + value.chars().count()))
}

fn first_yaml_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
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

fn yaml_value(value: &Value) -> String {
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

#[allow(clippy::too_many_arguments)]
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

/// Insert text for a property inside a flow mapping (`key: { a: 1, | }`):
/// everything stays in flow style on one line, since a block mapping is
/// invalid there. Nested objects open `{ child: | }`, arrays open `[|]`.
fn flow_property_insert(key: &str, property: &Value, root: &Value) -> Option<(String, usize)> {
    let yaml_key = yaml_key_for_completion(key)?;
    let flow = |value: &Value| match value {
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
        _ => yaml_value(value),
    };
    let mut insert = format!("{yaml_key}: ");
    let mut cursor = None;
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&flow(value));
    } else if let Some(default) = schema_default(property, root).filter(|value| {
        !value.as_array().is_some_and(Vec::is_empty)
            && !value.as_object().is_some_and(serde_json::Map::is_empty)
    }) {
        insert.push_str(&flow(default));
    } else if array_branch(property, root).is_some() {
        insert.push('[');
        cursor = Some(insert.len());
        insert.push(']');
    } else if let Some(branch) = object_branch(property, root) {
        insert.push('{');
        let properties = branch.get("properties").and_then(Value::as_object);
        let required = branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|name| Some((name, properties?.get(name)?)));
        let mut first = true;
        for (name, child) in required {
            let Some(child_key) = yaml_key_for_completion(name) else {
                continue;
            };
            insert.push_str(if first { " " } else { ", " });
            first = false;
            insert.push_str(&child_key);
            insert.push_str(": ");
            if cursor.is_none() {
                cursor = Some(insert.len());
            }
            if let Some(default) = schema_default(child, root) {
                insert.push_str(&flow(default));
            }
        }
        insert.push(' ');
        if cursor.is_none() {
            cursor = Some(insert.len());
        }
        insert.push_str(if first { " }" } else { "}" });
    }
    let cursor = insert[..cursor.unwrap_or(insert.len())]
        .encode_utf16()
        .count();
    Some((insert, cursor))
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

/// Finds the mapping colon on a single line, ignoring colons inside quoted
/// scalars and comments.
fn mapping_colon(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_none() && character == '#' {
            let starts_comment = index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace);
            if starts_comment {
                return None;
            }
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

/// Comment-blind variant of [`mapping_colon`], used when scanning value
/// context: a `#` never ends the scan, so colons after it still count.
fn mapping_colon_anywhere(line: &str) -> Option<usize> {
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
    let current_line_end = source[cursor..]
        .find('\n')
        .map_or(source.len(), |index| cursor + index);
    let current_line = &source[current_line_start..current_line_end];
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
        // Each leading `- ` opens an item at its own column, so `- - x: 1`
        // is an item of an item (a sequence of sequences).
        let mut mapping = trimmed;
        let mut mapping_indent = line_indent;
        item_counts.retain(|level, _| *level <= line_indent);
        while let Some(after_dash) = dash_item(mapping) {
            while ancestors
                .last()
                .is_some_and(|(level, _)| *level >= mapping_indent)
            {
                ancestors.pop();
            }
            item_counts.retain(|level, _| *level <= mapping_indent);
            let index = item_counts.entry(mapping_indent).or_default();
            ancestors.push((mapping_indent, SchemaPathSegment::ArrayItem(*index)));
            *index += 1;
            mapping_indent += mapping.len() - after_dash.len();
            mapping = after_dash;
        }
        let Some(colon) = mapping_colon_anywhere(mapping) else {
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
    let current_is_array_item = dash_item(prefix).is_some();
    let (line_content, content_offset) = {
        // The caret line's dashes: the first continues the list at its
        // column, any further one starts a new inner list.
        let mut content = prefix;
        let mut first = true;
        while let Some(after_dash) = dash_item(content) {
            let column = indent + (prefix.len() - content.len());
            let index = if first {
                item_counts.get(&column).copied().unwrap_or_default()
            } else {
                0
            };
            path.push(SchemaPathSegment::ArrayItem(index));
            content = after_dash;
            first = false;
        }
        (content, prefix.len() - content.len())
    };
    let candidate_colon = mapping_colon_anywhere(line_content);
    let content_start = indent + content_offset;
    let cursor_in_content = cursor
        .saturating_sub(current_line_start)
        .saturating_sub(content_start)
        .min(line_content.len());
    // A colon right of the caret belongs to the key being edited.
    let colon = candidate_colon.filter(|colon| *colon < cursor_in_content);
    // Only text before the caret decides the value start, so a trailing newline
    // cannot push the insertion point past the value.
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
    // Inside a flow mapping or sequence, opened on this line or an earlier one
    // (`key: { a: 1, se|`, `key: [\n  { mode: |`).
    let flow = match statement_start(&source[..cursor]) {
        Some(statement) if statement < current_line_start => {
            // The path above the statement comes from its own first line.
            let first_line = &source[statement..];
            let mut content = statement + (first_line.len() - first_line.trim_start_matches([' ', '\t']).len());
            if source[content..].starts_with("- ") {
                content += 2;
            }
            let base = completion_context(source, source[..content].encode_utf16().count()).path;
            Some((base, content))
        }
        Some(_) => Some((path.clone(), current_line_start + content_start)),
        None => None,
    };
    if let Some((base, content_byte)) = flow
        && let Some(caret) = inline_caret(&source[content_byte..cursor], b':')
    {
        let mut path = base;
        path.extend(caret.path);
        let line_end = source[cursor..].find('\n').map_or(source.len(), |end| cursor + end);
        let value_end = cursor + inline_value_len(&source[cursor..line_end]);
        let typed_from = |start: usize| source[content_byte + start..cursor].trim_end().to_string();
        let context = |path, typed, start: usize, end: usize| CompletionContext {
            path,
            typed,
            replace_start: source[..content_byte + start].encode_utf16().count(),
            replace_end: source[..end].encode_utf16().count(),
            base_indent: base_indent.clone(),
            position: EntryPosition::Inline,
            ..Default::default()
        };
        return match caret.entry {
            CaretEntry::Key { start } => {
                let tail: usize = source[cursor..]
                    .chars()
                    .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
                    .map(char::len_utf8)
                    .sum();
                context(path, typed_from(start), start, cursor + tail)
            }
            CaretEntry::Value { key, start } => CompletionContext {
                value_key: Some(key),
                value_needs_space: content_byte + start == cursor
                    && !source[..cursor].ends_with([' ', '\t']),
                ..context(path, typed_from(start), start, value_end)
            },
            CaretEntry::Item { index, start } => {
                path.push(SchemaPathSegment::ArrayItem(index));
                CompletionContext {
                    is_array_item: true,
                    entry_wrap: Some(("{ ", " }")),
                    ..context(path, typed_from(start), start, value_end)
                }
            }
        };
    }
    let value_needs_space = colon.is_some_and(|colon| cursor_in_content == colon + 1);
    // Typed in front of an existing entry (`lim|listen: 1`): the new entry goes
    // before it, which moves to its own line (see `entry_after`).
    let rest = &line_content[cursor_in_content..];
    let splits = value_key.is_none() && entry_follows(rest, b':');
    let word_after: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    let replace_end_byte = if splits { cursor } else { replace_end_byte };
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
        entry_suffix: if splits { format!("\n{base_indent}") } else { String::new() },
        entry_after: splits.then_some(word_after),
        base_indent,
        ..Default::default()
    }
}

fn existing_mapping_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    let parser_config =
        ParserConfig::serde_yaml_compat().duplicate_key_policy(DuplicateKeyPolicy::Error);
    // A half-typed key makes the document unparsable, so parse without the
    // caret's line; the other keys in this mapping still get filtered.
    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |newline| line_start + newline);
    if let Some(statement) = statement_start(&source[..cursor_byte])
        && let Some(keys) = inline_sibling_keys(&source[statement..], b':', cursor_byte - statement)
    {
        return Some(keys.into_iter().collect());
    }
    let sanitized = format!("{}{}", &source[..line_start], &source[line_end..]);
    let Ok(document) = noyalib::from_str_with_config::<Value>(&sanitized, &parser_config) else {
        return Some(sibling_block_keys(source, line_start, line_end));
    };
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
    let current_key = mapping_colon_anywhere(current_line).map(|colon| {
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

/// Keys of the block mapping holding the caret's line, by indentation: the
/// run of lines at the caret's indent, bounded by shallower lines. Used when
/// the rest of the document does not parse.
fn sibling_block_keys(source: &str, line_start: usize, line_end: usize) -> HashSet<String> {
    let indent_of = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let caret_line = &source[line_start..line_end];
    let indent = indent_of(caret_line);
    let key_of = |line: &str| -> Option<String> {
        let body = line.trim_start();
        let body = body.strip_prefix("- ").unwrap_or(body);
        let colon = mapping_colon_anywhere(body)?;
        Some(body[..colon].trim().trim_matches(['\'', '"']).to_string())
    };
    let is_sibling = |line: &&str| line.trim().is_empty() || line.trim_start().starts_with('#') || indent_of(line) >= indent;
    let before = source[..line_start.saturating_sub(1)].lines().rev().take_while(is_sibling);
    let after = source[line_end..].lines().skip(1).take_while(is_sibling);
    before
        .chain(after)
        .filter(|line| indent_of(line) == indent && !line.trim().is_empty())
        .filter_map(key_of)
        .collect()
}

/// The text after a leading `- ` (a block sequence item), or `None`.
fn dash_item(text: &str) -> Option<&str> {
    if text == "-" {
        return Some("");
    }
    text.strip_prefix("- ")
        .or_else(|| text.strip_prefix("-\t"))
        .map(|rest| rest.trim_start_matches([' ', '\t']))
}

/// Where hover reads the context for the key at `column`: just past its `:`,
/// so a key inside a flow mapping resolves under that mapping.
fn hover_context_offset(line: &str, column: usize) -> usize {
    let code = line_comment_start(line).map_or(line, |start| &line[..start]);
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    inline_key_at(code, b':', byte).map_or(line.len(), |key| key.after_separator)
}

fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let code = line_comment_start(line).map_or(line, |start| &line[..start]);
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    if let Some(key) = inline_key_at(code, b':', byte) {
        let text = line[key.start..key.end].trim_matches(['\'', '"']);
        return Some((text.to_string(), line[..key.start].chars().count()));
    }
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
    let colon = mapping_colon_anywhere(body)?;
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

/// Finds the start of a YAML comment on a single line, ignoring `#` characters
/// that appear inside quoted scalars.
fn line_comment_start(line: &str) -> Option<usize> {
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

fn code_before_comment(line: &str) -> &str {
    line_comment_start(line).map_or(line, |comment| &line[..comment])
}

/// YAML forbids tabs in indentation, though the parser accepts some. Block
/// scalar content (after `key: |`) is text and may hold tabs.
fn tab_indentation(source: &str) -> Option<ParseError> {
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

fn is_block_scalar(value: &str) -> bool {
    let mut characters = value.trim().chars();
    if !matches!(characters.next(), Some('|' | '>')) {
        return false;
    }
    characters.all(|character| character.is_ascii_digit() || matches!(character, '+' | '-'))
}

fn newline_with_indent(source: &str, cursor: usize) -> String {
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
        if let Some(colon) = mapping_colon(content) {
            let value = content[colon + 1..].trim();
            if value.is_empty() || is_block_scalar(value) {
                return format!("\n{indent}{item_indent}{unit}");
            }
            // Mapping inside a sequence item: the next key stays in the same item.
            return format!("\n{indent}{item_indent}");
        }
        return format!("\n{indent}- ");
    }
    if let Some(colon) = mapping_colon(code) {
        let value = code[colon + 1..].trim();
        if value.is_empty() || is_block_scalar(value) {
            return format!("\n{indent}{unit}");
        }
    }
    format!("\n{indent}")
}

fn toggle_yaml_line_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let newline_count = source[..end].bytes().filter(|byte| *byte == b'\n').count();
    let last_line =
        newline_count.saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));

    let lines: Vec<_> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        return None;
    }

    let selected_lines = &lines[first_line..=last_line.min(lines.len() - 1)];
    let uncomment = selected_lines
        .iter()
        .filter_map(|line| {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            (!content.is_empty()).then_some(content.starts_with('#'))
        })
        .all(|commented| commented);
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut offset = 0;
    let mut updated = String::with_capacity(source.len() + lines.len() * 2);

    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            if !content.is_empty() {
                if uncomment && content.starts_with('#') {
                    let removed = 1 + usize::from(content.starts_with("# "));
                    edits.push((offset + indent, removed, 0));
                    updated.push_str(&line[..indent]);
                    updated.push_str(&content[removed..]);
                    updated.push_str(&line[body.len()..]);
                } else if !uncomment {
                    edits.push((offset + indent, 0, 2));
                    updated.push_str(&line[..indent]);
                    updated.push_str("# ");
                    updated.push_str(&line[indent..]);
                } else {
                    updated.push_str(line);
                }
            } else {
                updated.push_str(line);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }

    let map_offset = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (edit_position, removed, inserted) in &edits {
            if position < *edit_position {
                break;
            }
            if position == *edit_position && *removed == 0 && !right_biased {
                return (*edit_position as isize + delta) as usize;
            }
            if position <= *edit_position + *removed {
                return (*edit_position as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let updated_start = map_offset(start, start == end).min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}

fn highlight(line: &str) -> Vec<Token> {
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
fn highlight_value(tokens: &mut Tokens, offset: usize, value: &str) {
    let scalar_start = offset + value.len() - value.trim_start().len();
    let (scalar, comment) = split_yaml_comment(value.trim_start());
    let scalar_end = scalar_start + scalar.len();
    tokens.push(scalar_start, scalar_end, scalar_kind(scalar));
    if let Some(comment) = comment {
        tokens.push(scalar_end, scalar_end + comment.len(), TokenKind::Comment);
    }
}

fn scalar_kind(scalar: &str) -> TokenKind {
    let scalar = scalar.trim_end();
    if matches!(scalar, "true" | "false" | "null" | "~") || scalar.parse::<f64>().is_ok() {
        TokenKind::Literal
    } else if scalar.starts_with('\'') || scalar.starts_with('"') {
        TokenKind::String
    } else {
        TokenKind::Value
    }
}

fn split_yaml_comment(value: &str) -> (&str, Option<&str>) {
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

fn yaml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
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

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------


#[cfg(test)]
#[cfg_attr(
    not(feature = "schema-validation"),
    allow(unused_imports, dead_code, reason = "the completion tests need schema validation")
)]
mod tests {
    use super::super::super::completion::{
        accept_marked, accept_reopens_suggestions, apply_completion,
        completions as shared_completions, key_hover as shared_key_hover,
    };
    use super::super::super::diagnostics::{
        Diagnostic, SchemaBundle, build_schema, validate as shared_validate,
    };
    use super::super::super::keyboard::paired_edit as shared_paired_edit;
    use super::super::super::language::ConfigLanguage;
    use super::YamlBehavior;
    use super::super::super::highlight::{TokenKind, runs};
    use super::{highlight, scalar_kind, split_yaml_comment};

    fn validate_yaml(
        source: &str,
        schema: Option<&SchemaBundle>,
        schema_error: Option<&str>,
        enabled: bool,
    ) -> Vec<Diagnostic> {
        shared_validate(source, schema, schema_error, enabled, &YamlBehavior)
    }

    fn flow_schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{"web":{"type":"object","properties":{
                "http":{"type":"integer"},
                "hosts":{"type":"array","items":{"type":"string"}},
                "letsencrypt":{"type":"object","required":["email"],
                    "properties":{"email":{"type":"string"}}},
                "headers":{"type":"object"}}}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn an_object_accepted_inside_a_flow_mapping_stays_in_flow_style() {
        let schema = flow_schema();
        let accepted = accept_marked("web: { le| }\n", "letsencrypt", &schema, &YamlBehavior);
        assert_eq!(accepted, "web: { letsencrypt: { email: | } }\n");
        let filled = accepted.replace('|', "a@b.c");
        let document = YamlBehavior.parse(&filled).expect("flow mapping parses");
        assert_eq!(document["web"]["letsencrypt"]["email"], "a@b.c");
    }

    #[test]
    fn an_array_and_an_empty_object_inside_a_flow_mapping_open_inline() {
        let schema = flow_schema();
        assert_eq!(
            accept_marked("web: { ho| }\n", "hosts", &schema, &YamlBehavior),
            "web: { hosts: [|] }\n"
        );
        assert_eq!(
            accept_marked("web: { he| }\n", "headers", &schema, &YamlBehavior),
            "web: { headers: { | } }\n"
        );
    }

    #[test]
    fn an_object_accepted_in_a_block_mapping_still_expands_to_a_block() {
        let schema = flow_schema();
        assert_eq!(
            accept_marked("web:\n  le|", "letsencrypt", &schema, &YamlBehavior),
            "web:\n  letsencrypt:\n    email: |\n"
        );
    }

    fn completions(
        source: &str,
        cursor: usize,
        force: bool,
        schema: Option<&SchemaBundle>,
        enabled: bool,
    ) -> Vec<super::super::super::completion::Completion> {
        shared_completions(source, cursor, force, schema, enabled, &YamlBehavior)
    }

    fn key_hover(
        source: &str,
        line_index: usize,
        column: usize,
        schema: Option<&SchemaBundle>,
    ) -> Option<super::super::super::completion::KeyHover> {
        shared_key_hover(source, line_index, column, schema, &YamlBehavior)
    }

    fn paired_edit(
        source: &str,
        start: usize,
        end: usize,
        opening: char,
    ) -> Option<(String, usize, usize)> {
        shared_paired_edit(source, start, end, opening, &YamlBehavior)
    }

    fn newline_with_indent(source: &str, cursor: usize) -> String {
        YamlBehavior.newline(source, cursor).text
    }

    fn toggle_yaml_line_comments(
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        YamlBehavior.toggle_comments(source, selection_start, selection_end)
    }

    fn test_schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{"public_address":{"type":"string","description":"Address"},"websocket":{"type":"object","properties":{"challenge":{"enum":["tls","http","dns"]}}}},"additionalProperties":false}"#,
        )
        .unwrap()
    }

    /// Nullable objects, `$ref` branches, dynamic `additionalProperties` keys,
    /// enums, and numeric-as-string examples.
    #[cfg(feature = "schema-validation")]
    fn fixture_schema() -> SchemaBundle {
        build_schema(include_str!("../../../tests/fixtures/schema.json")).unwrap()
    }

    #[test]
    fn paired_quotes_and_brackets_wrap_selected_text_and_keep_it_selected() {
        for (opening, expected) in [
            ('"', "\"value\""),
            ('\'', "'value'"),
            ('(', "(value)"),
            ('[', "[value]"),
            ('{', "{value}"),
        ] {
            let (updated, selection_start, selection_end) =
                paired_edit("value", 0, 5, opening).unwrap();
            assert_eq!(updated, expected);
            assert_eq!((selection_start, selection_end), (1, 6));
        }
    }

    #[test]
    fn paired_brackets_leave_the_cursor_between_the_pair() {
        let (updated, selection_start, selection_end) = paired_edit("a b", 1, 1, '[').unwrap();
        assert_eq!(updated, "a[] b");
        assert_eq!((selection_start, selection_end), (2, 2));
        let (updated, _, _) = paired_edit("key: ", 5, 5, '{').unwrap();
        assert_eq!(updated, "key: {}");
    }

    #[test]
    fn a_pair_does_not_auto_close_before_a_word_character() {
        assert_eq!(paired_edit("ab", 1, 1, '['), None);
        assert_eq!(paired_edit("foo", 0, 0, '"'), None);
        let (updated, _, _) = paired_edit("[]", 1, 1, '"').unwrap();
        assert_eq!(updated, "[\"\"]");
    }

    #[test]
    fn a_quote_after_a_word_character_does_not_pair() {
        assert_eq!(paired_edit("msg: don", 8, 8, '\''), None);
        assert_eq!(paired_edit("name_", 5, 5, '"'), None);
        let (updated, _, _) = paired_edit("msg: ", 5, 5, '\'').unwrap();
        assert_eq!(updated, "msg: ''");
    }

    #[test]
    fn an_opening_bracket_pairs_even_before_an_outer_closer() {
        let (updated, selection_start, selection_end) = paired_edit(")", 0, 0, '(').unwrap();
        assert_eq!(updated, "())");
        assert_eq!((selection_start, selection_end), (1, 1));
    }

    #[test]
    fn paired_quote_edit_respects_escaped_context_and_utf16_selections() {
        assert_eq!(paired_edit("path\\", 5, 5, '"'), None);
        let (updated, selection_start, selection_end) = paired_edit("🙂x", 0, 3, '"').unwrap();
        assert_eq!(updated, "\"🙂x\"");
        assert_eq!((selection_start, selection_end), (1, 4));
    }

    #[test]
    fn pairing_does_not_insert_inside_quoted_scalars_or_comments() {
        assert_eq!(paired_edit("name: \"value\"", 8, 8, '['), None);
        assert_eq!(paired_edit("name: plain # comment", 17, 17, '['), None);
    }

    #[test]
    fn valid_yaml_is_accepted_by_the_provided_schema() {
        assert!(
            validate_yaml(
                "public_address: example.test\n",
                Some(&test_schema()),
                None,
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn syntax_errors_include_the_yaml_line() {
        let errors = validate_yaml("value: [", None, None, true);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, Some(1));
        assert!(errors[0].column.is_some());
        assert!(!errors[0].message.is_empty());
    }

    #[test]
    fn an_empty_document_reports_no_diagnostics() {
        for source in ["", "\n", "  \n  \n"] {
            assert!(
                validate_yaml(source, Some(&test_schema()), None, true).is_empty(),
                "{source:?} should not report diagnostics"
            );
        }
    }

    #[test]
    fn duplicate_mapping_keys_are_reported_on_the_repeated_line() {
        let errors = validate_yaml(
            "insecure_plaintext: false\ninsecure_plaintext: true\n",
            None,
            None,
            true,
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, Some(2));
        assert!(errors[0].message.contains("duplicate"));
    }

    #[test]
    fn duplicate_keys_are_detected_in_flow_mappings_and_quoted_forms() {
        for (source, expected_line) in [
            ("one: 1\n\"one\": 2\n", 2),
            ("settings: {mode: safe, mode: fast}\n", 1),
        ] {
            let errors = validate_yaml(source, None, None, true);
            assert_eq!(errors.len(), 1, "{source}");
            assert_eq!(errors[0].line, Some(expected_line), "{source}");
            assert!(errors[0].message.contains("duplicate"), "{source}");
        }
    }

    #[test]
    fn malformed_schema_is_reported_as_a_diagnostic() {
        let schema_error = build_schema("not-json").err().unwrap();
        let errors = validate_yaml("name: app\n", None, Some(&schema_error), true);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("Invalid JSON Schema"),
            "{}",
            errors[0].message
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn schema_errors_are_reported_for_unknown_properties() {
        let errors = validate_yaml("unknown_option: true\n", Some(&test_schema()), None, true);
        assert!(!errors.is_empty());
        assert_eq!(errors[0].line, Some(1));
        assert_eq!(errors[0].column, Some(1));
        assert!(errors[0].end_column.unwrap() > errors[0].column.unwrap());
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn nested_schema_errors_map_to_the_yaml_key_range() {
        let source = "websocket:\n  challenge: invalid\n";
        let errors = validate_yaml(source, Some(&test_schema()), None, true);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, Some(2));
        assert_eq!(errors[0].column, Some(14));
        assert_eq!(errors[0].end_column, Some(21));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn top_level_property_suggestions_come_from_the_schema() {
        let input = "pub";
        let schema = test_schema();
        let options = completions(input, 3, false, Some(&schema), true);
        let option = options
            .iter()
            .find(|option| option.label == "public_address")
            .unwrap();
        assert!(!option.is_value);
        assert_eq!(option.type_label, "string");
        assert_eq!(option.detail, "Address");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn explicit_completion_request_offers_schema_properties_at_an_empty_position() {
        let options = completions("", 0, true, Some(&test_schema()), true);
        assert!(!options.is_empty());
        assert!(
            options
                .iter()
                .any(|option| option.label == "public_address")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn property_completions_omit_keys_already_present_in_the_mapping() {
        let input = "public_address: existing\n";
        let options = completions(
            input,
            input.encode_utf16().count(),
            true,
            Some(&test_schema()),
            true,
        );
        assert!(
            !options
                .iter()
                .any(|option| option.label == "public_address")
        );
        assert!(options.iter().any(|option| option.label == "websocket"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn partially_typed_keys_do_not_reoffer_existing_keys_when_yaml_is_temporarily_invalid() {
        let schema = fixture_schema();
        let input = "listen: 7835\ninsecure_plaintext: false\nins";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(
            !options
                .iter()
                .any(|option| option.label == "insecure_plaintext")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn cursor_before_a_mapping_colon_is_treated_as_key_completion_without_panicking() {
        let schema = fixture_schema();
        let input = "insecure_plaintext: false";
        let cursor = "ins".encode_utf16().count();
        let option = completions(input, cursor, false, Some(&schema), true)
            .into_iter()
            .find(|option| option.label == "insecure_plaintext")
            .expect("partial key should be completed as a property, not a boolean value");
        assert_eq!(apply_completion(input, &option).0, input);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn enum_suggestions_are_available_for_nested_values() {
        let input = "websocket:\n  challenge: t";
        let schema = test_schema();
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let option = options.iter().find(|option| option.label == "tls").unwrap();
        assert!(option.is_value);
        assert_eq!(option.type_label, "enum");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn property_completion_displays_combined_schema_types() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"name":{"anyOf":[{"type":"string"},{"type":"null"}]}}}"#,
        )
        .unwrap();
        let options = completions("", 0, true, Some(&schema), true);
        let option = options
            .iter()
            .find(|option| option.label == "name")
            .unwrap();
        assert_eq!(option.type_label, "null | string");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn property_completion_quotes_keys_that_need_yaml_quoting() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"display:name":{"type":"string"},"true":{"type":"boolean"}}}"#,
        )
        .unwrap();
        let options = completions("", 0, true, Some(&schema), true);
        assert_eq!(
            options
                .iter()
                .find(|option| option.label == "display:name")
                .unwrap()
                .insert,
            "\"display:name\": "
        );
        assert_eq!(
            options
                .iter()
                .find(|option| option.label == "true")
                .unwrap()
                .insert,
            "\"true\": "
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn object_completion_expands_required_fields_and_places_cursor_at_first_value() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"auth":{"type":"object","required":["enabled","token","mode"],"properties":{"enabled":{"type":"boolean","default":false},"token":{"type":"string"},"mode":{"type":"string","default":"safe"}}}}}"#,
        )
        .unwrap();
        let options = completions("", 0, true, Some(&schema), true);
        let option = options
            .iter()
            .find(|option| option.label == "auth")
            .unwrap();
        assert_eq!(
            option.insert,
            "auth:\n  enabled: false\n  token: \n  mode: safe\n"
        );
        let (updated, _, cursor) = apply_completion("", option);
        assert_eq!(updated, option.insert);
        assert_eq!(
            cursor,
            "auth:\n  enabled: false\n  token: ".encode_utf16().count()
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn completions_follow_additional_properties_into_dynamic_object_values() {
        let schema = build_schema(
            r##"{
                "type":"object",
                "properties":{
                    "clients":{
                        "type":"object",
                        "default":{},
                        "additionalProperties":{
                            "$ref":"#/$defs/Client"
                        }
                    }
                },
                "$defs":{
                    "Client":{
                        "type":"object",
                        "properties":{
                            "secret":{"type":"string"},
                            "max_connections":{"type":"integer"}
                        }
                    }
                }
            }"##,
        )
        .unwrap();
        let root_options = completions("", 0, true, Some(&schema), true);
        assert_eq!(
            root_options
                .iter()
                .find(|option| option.label == "clients")
                .unwrap()
                .insert,
            "clients:\n  "
        );
        let input = "clients:\n  alex:\n    ";
        let options = completions(
            input,
            input.encode_utf16().count(),
            true,
            Some(&schema),
            true,
        );
        assert!(options.iter().any(|option| option.label == "secret"));
        assert!(
            options
                .iter()
                .any(|option| option.label == "max_connections")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn fixture_schema_covers_dynamic_client_keys_through_additional_properties() {
        let schema = fixture_schema();
        let input = "clients:\n  alex:\n    ";
        let options = completions(
            input,
            input.encode_utf16().count(),
            true,
            Some(&schema),
            true,
        );
        assert!(options.iter().any(|option| option.label == "secret"));
        assert!(
            options
                .iter()
                .any(|option| option.label == "max_connections")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn completions_follow_pattern_properties_and_quoted_mapping_keys() {
        let schema = build_schema(
            r##"{
                "type":"object",
                "patternProperties":{
                    "^client:":{
                        "type":"object",
                        "properties":{"enabled":{"type":"boolean"}}
                    }
                }
            }"##,
        )
        .unwrap();
        let input = "\"client:one\":\n  en";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(options.iter().any(|option| option.label == "enabled"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn array_item_completions_follow_items_and_prefix_items_schemas() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "clients":{
                        "type":"array",
                        "items":{
                            "type":"object",
                            "properties":{"name":{"type":"string"}}
                        }
                    },
                    "modes":{
                        "type":"array",
                        "default":[],
                        "prefixItems":[{"enum":["fast","safe"]}]
                    }
                }
            }"#,
        )
        .unwrap();

        let root_options = completions("", 0, true, Some(&schema), true);
        assert_eq!(
            root_options
                .iter()
                .find(|option| option.label == "modes")
                .unwrap()
                .insert,
            "modes:\n  - "
        );

        let object_input = "clients:\n  - na";
        let object_options = completions(
            object_input,
            object_input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(object_options.iter().any(|option| option.label == "name"));

        let enum_input = "modes:\n  - fa";
        let enum_options = completions(
            enum_input,
            enum_input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(enum_options.iter().any(|option| option.label == "fast"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn const_and_boolean_schema_values_are_suggested_and_inserted() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "enabled":{"const":true},
                    "secure":{"type":"boolean"}
                }
            }"#,
        )
        .unwrap();
        let key_options = completions("", 0, true, Some(&schema), true);
        assert_eq!(
            key_options
                .iter()
                .find(|option| option.label == "enabled")
                .unwrap()
                .insert,
            "enabled: true"
        );

        let value_input = "secure: t";
        let value_options = completions(
            value_input,
            value_input.encode_utf16().count(),
            true,
            Some(&schema),
            true,
        );
        assert!(value_options.iter().any(|option| option.label == "true"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn examples_and_example_schema_values_are_suggested_in_value_positions() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "region":{"type":"string","examples":["eu-west-1","us-east-1"]},
                    "legacy":{"type":"string","example":"old.example"}
                }
            }"#,
        )
        .unwrap();

        let region_input = "region: eu";
        let region_options = completions(
            region_input,
            region_input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let region = region_options
            .iter()
            .find(|option| option.label == "eu-west-1")
            .unwrap();
        assert!(region.is_value);
        assert_eq!(region.insert, "eu-west-1");

        let legacy_input = "legacy: ol";
        let legacy_options = completions(
            legacy_input,
            legacy_input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(
            legacy_options
                .iter()
                .any(|option| option.label == "old.example")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn nullable_type_arrays_expand_objects_and_offer_null_values() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "cache":{
                        "type":["object","null"],
                        "properties":{
                            "size":{"type":"integer"},
                            "ttl":{"type":"integer"}
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let key_options = completions("", 0, true, Some(&schema), true);
        assert_eq!(
            key_options
                .iter()
                .find(|option| option.label == "cache")
                .unwrap()
                .insert,
            "cache:\n  "
        );

        let value_input = "cache: n";
        let value_options = completions(
            value_input,
            value_input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert!(value_options.iter().any(|option| option.label == "null"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn fuzzy_completion_matches_word_initials_and_subsequences() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "max_connections":{"type":"integer"},
                    "min_connections":{"type":"integer"}
                }
            }"#,
        )
        .unwrap();

        let input = "maxc";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert_eq!(
            options.first().map(|option| option.label.as_str()),
            Some("max_connections")
        );
        assert!(
            !options
                .iter()
                .any(|option| option.label == "min_connections")
        );

        let initials = "mc";
        let initials_options = completions(
            initials,
            initials.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        assert_eq!(initials_options.len(), 2);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn quoted_partial_keys_are_replaced_with_the_quoted_schema_key() {
        let schema =
            build_schema(r#"{"type":"object","properties":{"display:name":{"type":"string"}}}"#)
                .unwrap();
        let input = "\"disp";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let option = options
            .iter()
            .find(|option| option.label == "display:name")
            .unwrap();
        assert_eq!(option.insert, "\"display:name\": ");
        let (updated, _, cursor) = apply_completion(input, option);
        assert_eq!(updated, "\"display:name\": ");
        assert_eq!(cursor, updated.encode_utf16().count());
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn expanded_completions_follow_the_document_indentation_unit() {
        let schema = build_schema(
            r#"{
                "type":"object",
                "properties":{
                    "server":{
                        "type":"object",
                        "properties":{
                            "auth":{
                                "type":"object",
                                "required":["enabled","token"],
                                "properties":{
                                    "enabled":{"type":"boolean","default":false},
                                    "token":{"type":"string"}
                                }
                            }
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let input = "server:\n    ";
        let options = completions(
            input,
            input.encode_utf16().count(),
            true,
            Some(&schema),
            true,
        );
        let option = options
            .iter()
            .find(|option| option.label == "auth")
            .unwrap();
        assert_eq!(
            option.insert,
            "auth:\n        enabled: false\n        token: \n"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn typing_on_a_blank_line_suggests_missing_top_level_keys() {
        let schema = fixture_schema();
        let input = "listen: 7835\n\nc";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        assert!(
            labels.contains(&"clients"),
            "typing 'c' on a blank line should suggest missing keys, got: {labels:?}"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn typing_after_a_fresh_indent_suggests_nested_keys() {
        let schema = fixture_schema();
        let input = "web:\n  h";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        assert!(
            labels.contains(&"http") || labels.contains(&"https"),
            "typing after a fresh indent should suggest nested keys, got: {labels:?}"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn empty_query_on_a_fresh_indent_stays_silent_without_force() {
        let schema = fixture_schema();
        let input = "clients:\n  alex:\n    ";
        let cursor = input.encode_utf16().count();
        assert!(
            completions(input, cursor, false, Some(&schema), true).is_empty(),
            "empty query without Ctrl+Space should stay silent (VS Code parity)"
        );
        assert!(
            !completions(input, cursor, true, Some(&schema), true).is_empty(),
            "Ctrl+Space on a fresh indent should list nested keys"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn fixture_web_http_value_completion_stays_on_the_same_line() {
        let schema = fixture_schema();
        let input = "web:\n  http: ";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let option = options
            .iter()
            .find(|option| option.label == "0.0.0.0:7835")
            .expect("Bind examples should be offered for web.http");
        let (updated, _, cursor) = apply_completion(input, option);
        assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
        assert_eq!(cursor, updated.encode_utf16().count());
    }

    #[test]
    fn enter_continues_yaml_indentation_and_nests_after_a_mapping_key() {
        assert_eq!(newline_with_indent("services:", 9), "\n  ");
        assert_eq!(newline_with_indent("services:\n  web: ", 17), "\n    ");
    }

    #[test]
    fn enter_aligns_sequence_item_mappings_and_preserves_spacing_after_the_dash() {
        let source = "services:\n  - web:";
        assert_eq!(newline_with_indent(source, source.len()), "\n      ");

        let compact = "services:\n  - billing: 8080";
        assert_eq!(newline_with_indent(compact, compact.len()), "\n    ");

        let padded = "services:\n  -   web:";
        assert_eq!(newline_with_indent(padded, padded.len()), "\n        ");

        let empty_item = "services:\n  - ";
        assert_eq!(newline_with_indent(empty_item, empty_item.len()), "\n  - ");

        let plain_scalar_key = "services:\n  -web: 8080";
        assert_eq!(
            newline_with_indent(plain_scalar_key, plain_scalar_key.len()),
            "\n  "
        );
    }

    #[test]
    fn comment_shortcut_comments_selected_lines_and_preserves_selection() {
        let source = "one: 1\ntwo: 2\n";
        let (updated, start, end) =
            toggle_yaml_line_comments(source, 0, source.encode_utf16().count()).unwrap();
        assert_eq!(updated, "# one: 1\n# two: 2\n");
        assert_eq!(start, 0);
        assert_eq!(end, updated.encode_utf16().count());
    }

    #[test]
    fn comment_shortcut_uncomments_indented_lines_and_keeps_blank_lines() {
        let source = "  # first\n\n  # second\n";
        let (updated, _, _) =
            toggle_yaml_line_comments(source, 0, source.encode_utf16().count()).unwrap();
        assert_eq!(updated, "  first\n\n  second\n");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn accepting_a_value_closes_suggestions_but_accepting_a_key_reopens_them() {
        let schema = fixture_schema();
        let input = "web:\n  http: ";
        let value = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        )
        .into_iter()
        .find(|option| option.label == "0.0.0.0:7835")
        .expect("web.http example should be suggested");
        assert!(!accept_reopens_suggestions(&value, true));

        let key = completions(
            "web:\n  ",
            "web:\n  ".encode_utf16().count(),
            true,
            Some(&schema),
            true,
        )
        .into_iter()
        .find(|option| option.label == "http")
        .expect("web.http key should be suggested");
        assert!(accept_reopens_suggestions(&key, true));
        assert!(!accept_reopens_suggestions(&key, false));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn value_directly_after_a_bare_colon_gets_a_separating_space() {
        let schema = fixture_schema();
        let input = "web:\n  http:";
        let option = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        )
        .into_iter()
        .find(|option| option.label == "0.0.0.0:7835")
        .expect("web.http example should be suggested after a bare colon");
        let (updated, _, cursor) = apply_completion(input, &option);
        assert_eq!(updated, "web:\n  http: \"0.0.0.0:7835\"");
        assert_eq!(cursor, updated.encode_utf16().count());
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn hovering_a_key_returns_its_schema_documentation() {
        let schema = fixture_schema();
        let source = "web:\n  http: 80\n";
        let web = key_hover(source, 0, 1, Some(&schema)).expect("web should have docs");
        assert_eq!(web.key, "web");
        assert_eq!(
            web.description.as_deref(),
            Some("The web proxy settings; off unless set")
        );

        let http = key_hover(source, 1, 4, Some(&schema)).expect("nested http should have docs");
        assert_eq!(http.key, "http");
        assert_eq!(http.start_column, 2);
        assert_eq!(
            http.examples,
            vec!["0.0.0.0:7835".to_string(), "7835".to_string()]
        );
        assert!(http.type_label.contains("integer") || http.type_label.contains("string"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn hovering_outside_a_key_or_on_an_unknown_key_shows_nothing() {
        let schema = fixture_schema();
        let source = "web:\n  http: 80\n  unknown_key: 1\n";
        assert!(
            key_hover(source, 1, 8, Some(&schema)).is_none(),
            "value text is not a key"
        );
        assert!(
            key_hover(source, 2, 3, Some(&schema)).is_none(),
            "unknown keys have no schema"
        );
        assert!(
            key_hover(source, 0, 1, None).is_none(),
            "no schema, no docs"
        );
    }

    #[test]
    fn hover_marks_required_properties_from_the_parent_schema() {
        let schema = build_schema(
            r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","description":"App name"},"port":{"type":"integer","default":8080}}}"#,
        )
        .unwrap();
        let name = key_hover("name: app\n", 0, 0, Some(&schema)).unwrap();
        assert!(name.required);
        assert_eq!(name.description.as_deref(), Some("App name"));
        let port = key_hover("port: 1\n", 0, 0, Some(&schema)).unwrap();
        assert!(!port.required);
        assert_eq!(port.default.as_deref(), Some("8080"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn null_is_offered_only_when_explicitly_requested_or_typed() {
        let schema = fixture_schema();
        let auto = completions("web:", 4, false, Some(&schema), true);
        assert!(
            auto.iter().all(|option| option.label != "null"),
            "auto-triggered list after a bare key must not offer null"
        );
        let explicit = completions("web:", 4, true, Some(&schema), true);
        assert!(explicit.iter().any(|option| option.label == "null"));
        let typed = completions("listen: nu", 10, false, Some(&schema), true);
        assert!(typed.iter().any(|option| option.label == "null"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn numeric_examples_are_inserted_as_numbers_when_the_schema_allows_integers() {
        let schema = fixture_schema();
        let input = "listen: ";
        let port = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        )
        .into_iter()
        .find(|option| option.label == "7835")
        .expect("port example should be suggested");
        let (updated, _, _) = apply_completion(input, &port);
        assert_eq!(updated, "listen: 7835");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn enum_words_are_inserted_as_plain_scalars_and_ambiguous_strings_stay_quoted() {
        use crate::editor::languages::yaml::yaml_value;
        use serde_json::Value;
        let schema = fixture_schema();
        let input = "services:\n  billing:\n    protocol: ";
        let udp = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        )
        .into_iter()
        .find(|option| option.label == "udp")
        .expect("protocol values should be suggested");
        assert_eq!(
            apply_completion(input, &udp).0,
            "services:\n  billing:\n    protocol: udp"
        );
        assert_eq!(yaml_value(&Value::String("yes".into())), "\"yes\"");
        assert_eq!(yaml_value(&Value::String("7835".into())), "\"7835\"");
        assert_eq!(
            yaml_value(&Value::String("0.0.0.0:7835".into())),
            "\"0.0.0.0:7835\""
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn required_properties_sort_before_optional_ones() {
        let schema = fixture_schema();
        let input = "services:\n  dashboard:\n    ";
        let options = completions(
            input,
            input.encode_utf16().count(),
            true,
            Some(&schema),
            true,
        );
        assert_eq!(
            options.first().map(|option| option.label.as_str()),
            Some("target")
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn value_for_an_expanded_required_key_stays_on_its_line() {
        let schema = fixture_schema();
        // What accepting `metrics` inserts.
        let input = "metrics:\n  listen: \n";
        let cursor = "metrics:\n  listen: ".encode_utf16().count();
        let option = completions(input, cursor, false, Some(&schema), true)
            .into_iter()
            .find(|option| option.label == "0.0.0.0:7835")
            .expect("metrics.listen should offer Bind examples");
        let (updated, _, cursor_after) = apply_completion(input, &option);
        assert_eq!(updated, "metrics:\n  listen: \"0.0.0.0:7835\"\n");
        assert_eq!(
            cursor_after,
            "metrics:\n  listen: \"0.0.0.0:7835\""
                .encode_utf16()
                .count()
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn typing_inside_a_value_uses_the_text_before_the_caret_and_replaces_the_token() {
        let schema = fixture_schema();
        let input = "update:\n  remote: ftrue";
        let cursor = "update:\n  remote: f".encode_utf16().count();
        let option = completions(input, cursor, false, Some(&schema), true)
            .into_iter()
            .find(|option| option.label == "false")
            .expect("typing before existing text should suggest the matching value");
        let (updated, _, cursor_after) = apply_completion(input, &option);
        assert_eq!(updated, "update:\n  remote: false");
        assert_eq!(cursor_after, updated.encode_utf16().count());
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn accepting_a_value_keeps_an_inline_comment() {
        let schema = fixture_schema();
        let input = "update:\n  remote: tru # keep me";
        let cursor = "update:\n  remote: tru".encode_utf16().count();
        let option = completions(input, cursor, false, Some(&schema), true)
            .into_iter()
            .find(|option| option.label == "true")
            .expect("true should match the partial value");
        let (updated, _, _) = apply_completion(input, &option);
        assert_eq!(updated, "update:\n  remote: true # keep me");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn completion_replace_range_survives_multibyte_characters_before_the_caret() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"café":{"type":"string"},"listen":{"type":"integer"}}}"#,
        )
        .unwrap();
        // `é` is one UTF-16 unit but two UTF-8 bytes.
        let input = "café: 1\nlis: 2";
        let cursor = "café: 1\nlis".encode_utf16().count();
        let option = completions(input, cursor, false, Some(&schema), true)
            .into_iter()
            .find(|option| option.label == "listen")
            .expect("partial key should complete to the schema property");
        let (updated, _, _) = apply_completion(input, &option);
        assert_eq!(updated, "café: 1\nlisten: 2");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_nested_key_maps_to_its_own_line_not_the_first_key_with_the_same_name() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{"listen":{"type":"integer","minimum":1}}}}}"#,
        )
        .unwrap();
        let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(11));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_unexpected_nested_key_maps_to_its_own_line_not_a_root_key_with_the_same_name() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"listen":{"type":"integer"},"websocket":{"type":"object","properties":{},"additionalProperties":false}}}"#,
        )
        .unwrap();
        let source = "listen: 7835\nwebsocket:\n  listen: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_key_inside_a_sequence_item_maps_to_its_own_line() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"weight":{"type":"integer"},"services":{"type":"object","additionalProperties":{"type":"object","properties":{"clients":{"type":"array","items":{"type":"object","properties":{"weight":{"type":"integer","minimum":1}}}}}}}}}"#,
        )
        .unwrap();
        let source = "weight: 5\nservices:\n  billing:\n    clients:\n      - name: laptop\n        weight: 0\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(6));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_empty_sequence_item_error_points_to_the_dash_line() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"self":{"type":"object","properties":{"allow":{"type":"array","items":{"type":"string"}}}}}}"#,
        )
        .unwrap();
        let source = "self:\n  allow:\n    - \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(7));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn empty_schema_value_points_after_the_colon_instead_of_at_the_key() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"websocket":{"type":"object","properties":{"listen":{"type":"string"}}}}}"#,
        )
        .unwrap();
        let source = "websocket:\n  listen: \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(11));
        assert_eq!(diagnostics[0].end_column, Some(12));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn empty_schema_value_with_comment_points_to_the_value_slot() {
        let schema =
            build_schema(r#"{"type":"object","properties":{"listen":{"type":"string"}}}"#).unwrap();
        let source = "listen: # set a port\n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
        assert_eq!(diagnostics[0].column, Some(9));
        assert_eq!(diagnostics[0].end_column, Some(10));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn nested_any_of_errors_point_to_the_specific_empty_value() {
        let schema = build_schema(
            r##"{
                "type":"object",
                "properties":{
                    "websocket":{
                        "anyOf":[
                            {"$ref":"#/$defs/WebSocket"},
                            {"type":"null"}
                        ]
                    }
                },
                "$defs":{
                    "WebSocket":{
                        "type":"object",
                        "properties":{
                            "listen":{"anyOf":[{"type":"integer"},{"type":"string"}]}
                        },
                        "required":["listen"]
                    }
                }
            }"##,
        )
        .unwrap();
        let source = "websocket:\n  listen: \n";

        let diagnostics = validate_yaml(source, Some(&schema), None, true);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(11));
        assert!(diagnostics[0].message.contains("/websocket/listen"));
    }

    #[test]
    fn plain_scalars_are_colored_as_strings_and_yaml_literals_stay_distinct() {
        assert_eq!(scalar_kind("unquoted value"), TokenKind::Value);
        assert_eq!(scalar_kind("\"quoted value\""), TokenKind::String);
        assert_eq!(scalar_kind("false"), TokenKind::Literal);
        assert_eq!(scalar_kind("42"), TokenKind::Literal);
    }

    fn highlighted(line: &str) -> Vec<(Option<TokenKind>, String)> {
        runs(line, &highlight(line))
    }

    fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
        (kind, text.to_string())
    }

    #[test]
    fn a_comment_line_is_one_comment_run_after_its_indent() {
        assert_eq!(
            highlighted("  # note: x"),
            [run(None, "  "), run(Some(TokenKind::Comment), "# note: x")]
        );
    }

    #[test]
    fn a_mapping_key_gets_key_and_colon_runs_then_its_value() {
        assert_eq!(
            highlighted("  name: app"),
            [
                run(None, "  "),
                run(Some(TokenKind::Key), "name"),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::Value), "app"),
            ]
        );
        assert_eq!(
            highlighted("parent:"),
            [
                run(Some(TokenKind::Key), "parent"),
                run(Some(TokenKind::Punctuation), ":"),
            ]
        );
    }

    #[test]
    fn values_are_classed_as_literal_string_or_value() {
        let value_kind = |line: &str| highlighted(line).last().unwrap().clone();
        assert_eq!(value_kind("a: 42"), run(Some(TokenKind::Literal), "42"));
        assert_eq!(value_kind("a: ~"), run(Some(TokenKind::Literal), "~"));
        assert_eq!(value_kind("a: true"), run(Some(TokenKind::Literal), "true"));
        assert_eq!(
            value_kind("a: 'x y'"),
            run(Some(TokenKind::String), "'x y'")
        );
        assert_eq!(
            value_kind("a: \"x\""),
            run(Some(TokenKind::String), "\"x\"")
        );
        assert_eq!(value_kind("a: [1, 2]"), run(Some(TokenKind::Value), "[1, 2]"));
    }

    #[test]
    fn an_inline_comment_after_a_value_is_its_own_run() {
        assert_eq!(
            highlighted("port: 80 # http"),
            [
                run(Some(TokenKind::Key), "port"),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::Literal), "80 "),
                run(Some(TokenKind::Comment), "# http"),
            ]
        );
        assert_eq!(
            highlighted("a: \"x # y\""),
            [
                run(Some(TokenKind::Key), "a"),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::String), "\"x # y\""),
            ]
        );
    }

    #[test]
    fn a_dash_before_a_key_stays_plain_but_a_bare_item_dash_is_punctuation() {
        assert_eq!(
            highlighted("  - name: x"),
            [
                run(None, "  - "),
                run(Some(TokenKind::Key), "name"),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::Value), "x"),
            ]
        );
        assert_eq!(
            highlighted("- 1 # one"),
            [
                run(Some(TokenKind::Punctuation), "- "),
                run(Some(TokenKind::Literal), "1 "),
                run(Some(TokenKind::Comment), "# one"),
            ]
        );
    }

    #[test]
    fn lines_matching_no_rule_stay_plain() {
        for line in ["---", "key with spaces: x", "url:http", "plain text", "-x"] {
            assert_eq!(highlighted(line), [run(None, line)], "{line:?}");
        }
        assert!(highlighted("").is_empty());
    }

    #[test]
    fn highlighting_covers_every_character_of_tricky_lines() {
        for line in [
            "",
            "   ",
            "# only a comment",
            "name: héllo 🙂 # ünïcode",
            "- \"unterminated # still quoted",
            "key: 'it''s # quoted' # real",
            "key: value   ",
            "- ",
            "\u{3000}key: x",
            "a:\tb",
        ] {
            let text: String = highlighted(line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(text, line);
        }
    }

    #[test]
    fn comments_are_split_only_outside_yaml_quotes() {
        assert_eq!(
            split_yaml_comment("plain value # note"),
            ("plain value ", Some("# note"))
        );
        assert_eq!(
            split_yaml_comment("\"value # not a comment\""),
            ("\"value # not a comment\"", None)
        );
        assert_eq!(
            split_yaml_comment("'it''s # still quoted'"),
            ("'it''s # still quoted'", None)
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn keys_inside_an_inline_mapping_are_suggested_from_the_schema() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"clients":{"type":"object","additionalProperties":{"type":"object","properties":{"secret":{"type":"string"},"max_connections":{"type":"integer"}}}}}}"#,
        )
        .unwrap();
        let input = "clients:\n  acme: { sec";
        let options = completions(
            input,
            input.encode_utf16().count(),
            false,
            Some(&schema),
            true,
        );
        let option = options
            .iter()
            .find(|option| option.label == "secret")
            .expect("a key typed inside { } should be suggested");
        let (updated, _, _) = apply_completion(input, option);
        assert_eq!(updated, "clients:\n  acme: { secret: ");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_an_inline_mapping_points_at_its_key() {
        let schema = build_schema(
            r#"{"type":"object","properties":{"clients":{"type":"object","additionalProperties":{"type":"object","properties":{"secret":{"type":"string"}}}}}}"#,
        )
        .unwrap();
        let source = "clients:\n  acme: { secret }\n";
        let diagnostics = validate_yaml(source, Some(&schema), None, true);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(11));
    }
}
