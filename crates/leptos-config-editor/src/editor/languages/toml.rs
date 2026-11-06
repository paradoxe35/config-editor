//! TOML language support.
//!
//! - Syntax errors come from the `toml` crate; duplicate keys are errors.
//! - Structure comes from a tolerant line scan: `[table]` headers set the
//!   path from the root, `[[array]]` headers add the occurrence index, and
//!   dotted keys nest under the current table.
//! - Inline tables (`key = { … }`) are scanned within their line; completion
//!   inside one stays inline, since a `[table]` header is invalid there.
//! - Objects insert as `[table]` headers, except beneath array items, which
//!   headers cannot address, where they are written inline.
//! - `Ctrl+/` toggles `#` line comments.

use super::super::completion::{array_branch, object_branch, schema_const, schema_default};
use super::super::keyboard::utf16_to_byte_index;
#[cfg(feature = "schema-validation")]
use super::super::language::pointer_segments;
use super::super::language::{
    CompletionContext, EntryPosition, InsertContext, LanguageBehavior, ParseError,
    SchemaPathSegment,
};
use leptos::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct TomlBehavior;

pub(super) const SPEC: super::LanguageSpec = super::LanguageSpec {
    label: "TOML",
    icon: "T",
    extensions: &["toml"],
    behavior: &TomlBehavior,
    codec: &TomlBehavior,
};

impl LanguageBehavior for TomlBehavior {
    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        toml_parse(source)
    }

    #[cfg(feature = "schema-validation")]
    fn span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)> {
        toml_key_span(source, path)
    }

    #[cfg(feature = "schema-validation")]
    fn value_span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)> {
        toml_value_span(source, path)
    }

    #[cfg(feature = "schema-validation")]
    fn first_span(&self, source: &str) -> Option<(usize, usize, usize)> {
        first_line_span(source)
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
        existing_table_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, context)
    }

    fn format_value(&self, value: &Value) -> String {
        toml_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn highlight_line(&self, line: &str) -> AnyView {
        highlight_line(line)
    }

    fn newline_with_indent(&self, source: &str, cursor: usize) -> String {
        newline_with_indent(source, cursor)
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_toml_line_comments(source, selection_start, selection_end)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> (Option<char>, bool) {
        toml_lexical_state(source, cursor)
    }

    fn confidence(&self, source: &str) -> u8 {
        toml_confidence(source)
    }
}

fn utf16(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One-based line and character column of a byte offset.
fn line_column(source: &str, byte: usize) -> (usize, usize) {
    let before = &source[..byte.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

// ---------------------------------------------------------------------------
// Parsing and spans
// ---------------------------------------------------------------------------

/// Datetimes become strings. Parsing as a `Table`, not a bare `Value`,
/// reports a duplicate key at its second occurrence rather than at line end.
fn toml_parse(source: &str) -> Result<Value, ParseError> {
    let document: toml::Table = source.parse().map_err(|error: toml::de::Error| {
        let line_count = source.split('\n').count().max(1);
        let (line, column) = error
            .span()
            .map(|span| line_column(source, span.start))
            .unwrap_or((1, 1));
        ParseError {
            message: error.message().to_string(),
            line: Some(line.clamp(1, line_count)),
            column: Some(column.max(1)),
        }
    })?;
    serde_json::to_value(&document).map_err(|error| ParseError {
        message: error.to_string(),
        line: Some(1),
        column: Some(1),
    })
}

#[cfg(feature = "schema-validation")]
fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (
        line,
        column,
        column + source[start..end.min(source.len())].chars().count(),
    )
}

/// Splits a line into code and trailing comment. A `#` starts a comment only
/// at the line start or after whitespace; `#` inside strings stays code.
fn split_toml_comment(line: &str) -> (&str, Option<&str>) {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                if active == '\'' && line[index..].starts_with("''") {
                    continue;
                }
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            // A run of 3+ quotes opens a multi-line string even if unterminated here.
            let run = line[index..]
                .chars()
                .take_while(|next| *next == character)
                .count();
            if run >= 3 {
                return (line, None);
            }
            quote = Some(character);
        } else if character == '#'
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (&line[..index], Some(&line[index..]));
        }
    }
    (line, None)
}

/// A table-addressing header: `[table]`, `[[array]]`, or dotted `a.b.c`.
/// Returns the path segments plus whether it is an array-of-tables header.
fn parse_header(line: &str) -> Option<(Vec<String>, bool)> {
    let trimmed = line.trim();
    let (inner, is_array) = trimmed
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]").map(|rest| (rest, true)))
        .or_else(|| {
            trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .map(|inner| (inner, false))
        })?;
    if inner.trim().is_empty() {
        return None;
    }
    let segments: Vec<String> = split_key_path(inner)?;
    (!segments.is_empty()).then_some((segments, is_array))
}

/// Splits a dotted key path honoring quoted segments (`a."b.c".d`).
fn split_key_path(text: &str) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for character in text.chars() {
        if let Some(active) = quote {
            current.push(character);
            if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
            current.push(character);
        } else if character == '.' {
            segments.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(character);
        }
    }
    if quote.is_some() {
        return None;
    }
    segments.push(current.trim().to_string());
    if segments.iter().any(String::is_empty) {
        return None;
    }
    Some(
        segments
            .into_iter()
            .map(|segment| unquote_key(&segment))
            .collect(),
    )
}

/// Strips one layer of basic/literal quotes from a key segment.
fn unquote_key(segment: &str) -> String {
    if segment.len() >= 2
        && ((segment.starts_with('"') && segment.ends_with('"'))
            || (segment.starts_with('\'') && segment.ends_with('\'')))
    {
        segment[1..segment.len() - 1].to_string()
    } else {
        segment.to_string()
    }
}

/// The unquoted key before `=` on a value line, with its byte span relative
/// to the line start. Comment lines and table headers yield `None`.
fn key_assignment(line: &str) -> Option<(String, usize, usize)> {
    let (code, _) = split_toml_comment(line);
    if code.trim().is_empty() || code.trim_start().starts_with('[') {
        return None;
    }
    let equals = assignment_equals(code)?;
    let raw = code[..equals].trim();
    if raw.is_empty() {
        return None;
    }
    let key = unquote_key(raw);
    let start = code[..equals].find(raw).unwrap_or(0);
    Some((key, start, start + raw.len()))
}

/// Byte index of the `=` separating key from value, ignoring `=` inside
/// quoted keys/strings.
fn assignment_equals(line: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '=' {
            return Some(index);
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
/// Byte span of the value after `=` on a value line: the scalar, or the whole
/// rest of the line for inline tables/arrays that may span lines.
fn value_span_in_line(line: &str) -> Option<(usize, usize)> {
    let (code, _) = split_toml_comment(line);
    let equals = assignment_equals(code)?;
    let after = &code[equals + 1..];
    let trimmed = after.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() {
        return None;
    }
    let start = equals + 1 + (after.len() - trimmed.len());
    let end = start + trimmed.trim_end_matches([' ', '\t']).len();
    (start < end).then_some((start, end))
}

/// Schema path of the table holding `line_index`: walks headers above it. A
/// `[table]` header sets the absolute path from the root; `[[array]]`
/// appends the occurrence index of that header so repeated tables address
/// distinct array items. A blank path is the document root.
fn table_path_at(lines: &[&str], line_index: usize) -> Vec<SchemaPathSegment> {
    let end = line_index.min(lines.len());
    let mut path: Vec<SchemaPathSegment> = Vec::new();
    for (position, line) in lines[..end].iter().enumerate() {
        let (code, _) = split_toml_comment(line);
        let Some((segments, is_array)) = parse_header(code) else {
            continue;
        };
        if is_array {
            let parent = &segments[..segments.len().saturating_sub(1)];
            path = parent
                .iter()
                .map(|segment| SchemaPathSegment::Key(segment.clone()))
                .collect();
            let last = segments.last().cloned().unwrap_or_default();
            let header = code.trim().to_string();
            let seen = lines[..position]
                .iter()
                .filter(|candidate| split_toml_comment(candidate).0.trim() == header)
                .count();
            path.push(SchemaPathSegment::Key(last));
            path.push(SchemaPathSegment::ArrayItem(seen));
        } else {
            path = segments.into_iter().map(SchemaPathSegment::Key).collect();
        }
    }
    path
}

#[cfg(feature = "schema-validation")]
/// Array indices render as numbers, matching the validator's JSON-pointer segments.
fn path_strings(path: &[SchemaPathSegment]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            SchemaPathSegment::Key(key) => key.clone(),
            SchemaPathSegment::ArrayItem(index) => index.to_string(),
        })
        .collect()
}

#[cfg(feature = "schema-validation")]
/// One entry inside an inline table, with byte offsets relative to the line
/// and its full schema path (outer table + inline key chain).
struct InlineEntry {
    key: (usize, usize),
    value: (usize, usize),
    path: Vec<String>,
}

#[cfg(feature = "schema-validation")]
/// Collects the entries of a single-line inline table starting just after its
/// `{`. Nested inline tables recurse, so `a = { b = { c = 1 } }` yields `a`,
/// `a.b`, and `a.b.c`. `offset` is where `text` starts inside the line.
fn inline_entries(text: &str, offset: usize, base: &[String], out: &mut Vec<InlineEntry>) {
    let mut index = 0;
    while index < text.len() {
        while index < text.len()
            && matches!(text.as_bytes()[index], b' ' | b'\t' | b'\n' | b'\r' | b',')
        {
            index += 1;
        }
        if index >= text.len() || text.as_bytes()[index] == b'}' {
            return;
        }
        let key_start = index;
        let Some(equals) = inline_equals(text, key_start) else {
            index = inline_item_end(text, key_start).max(index + 1);
            continue;
        };
        let raw = text[key_start..equals].trim_end();
        let key_end = key_start + raw.len();
        let mut full = base.to_vec();
        full.extend(split_key_path(&unquote_key(raw)).unwrap_or_else(|| vec![unquote_key(raw)]));
        let mut value_start = equals + 1;
        while value_start < text.len() && matches!(text.as_bytes()[value_start], b' ' | b'\t') {
            value_start += 1;
        }
        let nested = text[value_start..].starts_with('{');
        let value_end = if nested {
            matching_brace(text, value_start)
                .map_or(text.len(), |close| (close + 1).min(text.len()))
        } else {
            inline_item_end(text, value_start)
        };
        out.push(InlineEntry {
            key: (offset + key_start, offset + key_end),
            value: (offset + value_start, offset + value_end),
            path: full.clone(),
        });
        if nested
            && let Some(close) = matching_brace(text, value_start)
            && close > value_start + 1
        {
            inline_entries(
                &text[value_start + 1..close],
                offset + value_start + 1,
                &full,
                out,
            );
        }
        index = value_end.max(index + 1);
    }
}

#[cfg(feature = "schema-validation")]
/// Byte index of the `=` after `from`, outside strings, stopping at the entry
/// boundary (`,` or `}`).
fn inline_equals(text: &str, from: usize) -> Option<usize> {
    let mut quote = None;
    for (index, character) in text[from..].char_indices() {
        match quote {
            Some(active) => {
                if character == active {
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                '=' => return Some(from + index),
                ',' | '}' => return None,
                _ => {}
            },
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
/// Byte index of the closing brace matching the `{` at `open`, honoring
/// strings, arrays, and nested braces.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in text[open..].char_indices() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && active == '"' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(open + index);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
/// Byte index just past the value starting at `from`: the next `,` or `}` at
/// the top level, with trailing whitespace trimmed.
fn inline_item_end(text: &str, from: usize) -> usize {
    let mut index = from;
    let mut quote = None;
    let mut depth = 0usize;
    let mut escaped = false;
    while index < text.len() {
        let character = text[index..].chars().next().unwrap_or_default();
        let width = character.len_utf8();
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && active == '"' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else {
            match character {
                '"' | '\'' => quote = Some(character),
                '{' | '[' | '(' => depth += 1,
                '}' | ']' | ')' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                ',' if depth == 0 => break,
                _ => {}
            }
        }
        index += width;
    }
    let end = &text[from..index];
    from + end.trim_end_matches([' ', '\t']).len()
}

/// Unclosed `{` count at the end of `text`, ignoring quoted strings.
fn inline_depth(text: &str) -> usize {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for character in text.chars() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && active == '"' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Path of the inline table holding the caret. `base` is the path of the
/// outer table (the assignment's key included) and `open` the byte index of
/// its `{`. Nested `key = {` entries extend the path, so
/// `clients = { acme = { |` completes under `clients.acme`.
fn inline_table_path(
    base: &[SchemaPathSegment],
    code: &str,
    open: usize,
) -> Vec<SchemaPathSegment> {
    let mut stack = vec![base.to_vec()];
    let mut segment_start = open + 1;
    let mut index = open + 1;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    while index < code.len() {
        let character = code[index..].chars().next().unwrap_or('\0');
        let width = character.len_utf8();
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && active == '"' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            index += width;
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '{' => {
                let raw = code[segment_start..index].trim();
                let mut path = stack.last().cloned().unwrap_or_else(|| base.to_vec());
                if let Some(equals) = raw.rfind('=') {
                    let key = raw[..equals].trim();
                    match split_key_path(&unquote_key(key)) {
                        Some(parts) => path.extend(parts.into_iter().map(SchemaPathSegment::Key)),
                        None => path.push(SchemaPathSegment::Key(unquote_key(key))),
                    }
                }
                stack.push(path);
                segment_start = index + 1;
            }
            '}' => {
                if stack.len() > 1 {
                    stack.pop();
                }
                segment_start = index + 1;
            }
            ',' => segment_start = index + 1,
            _ => {}
        }
        index += width;
    }
    stack.pop().unwrap_or_else(|| base.to_vec())
}

#[cfg(feature = "schema-validation")]
/// Span of a key nested in an inline-table value on `code`: `base` is the
/// outer assignment's path. Returns the entry's key and value spans.
fn inline_entry_at(
    code: &str,
    base: &[String],
    wanted: &[String],
) -> Option<(usize, usize, usize, usize)> {
    if wanted.len() <= base.len() || !wanted.starts_with(base) {
        return None;
    }
    let (value_start, _) = value_span_in_line(code)?;
    if !code[value_start..].starts_with('{') {
        return None;
    }
    let inner_start = value_start + 1;
    let inner_end = matching_brace(code, value_start)
        .unwrap_or(code.len())
        .max(inner_start);
    let mut entries = Vec::new();
    inline_entries(
        &code[inner_start..inner_end],
        inner_start,
        base,
        &mut entries,
    );
    entries
        .into_iter()
        .find(|entry| entry.path == wanted)
        .map(|entry| (entry.key.0, entry.key.1, entry.value.0, entry.value.1))
}

#[cfg(feature = "schema-validation")]
fn toml_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let table = path_strings(&table_path_at(&lines, index));
        let (code, _) = split_toml_comment(line);
        if let Some((key, start, end)) = key_assignment(code) {
            let mut full = table.clone();
            for segment in split_key_path(&key).unwrap_or_else(|| vec![key.clone()]) {
                full.push(segment);
            }
            if full == wanted {
                let offset: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
                return Some(span_between(source, offset + start, offset + end));
            }
            // `key = { nested = … }` keeps every nested key on this line.
            if let Some((key_start, key_end, _, _)) = inline_entry_at(code, &full, &wanted) {
                let offset: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
                return Some(span_between(source, offset + key_start, offset + key_end));
            }
        }
        // A `[table]` header's last segment names the table itself.
        if let Some((segments, _)) = parse_header(code)
            && path_strings(&table_path_at(&lines, index + 1)) == wanted
            && !segments.is_empty()
        {
            let offset: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
            let header_start = line.find('[').unwrap_or(0);
            return Some(span_between(
                source,
                offset + header_start,
                offset + line.len(),
            ));
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
fn toml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let table = path_strings(&table_path_at(&lines, index));
        let (code, _) = split_toml_comment(line);
        let Some((key, _, _)) = key_assignment(code) else {
            continue;
        };
        let mut full = table.clone();
        for segment in split_key_path(&key).unwrap_or_else(|| vec![key.clone()]) {
            full.push(segment);
        }
        if full == wanted
            && let Some((start, end)) = value_span_in_line(code)
        {
            let offset: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
            if code[start..].trim_start().starts_with(['{', '[']) {
                let (line_number, column) = line_column(source, offset + start);
                return Some((line_number, column, column + 1));
            }
            return Some(span_between(source, offset + start, offset + end));
        }
        // A value nested in an inline table points at that entry's value.
        if let Some((_, _, value_start, value_end)) = inline_entry_at(code, &full, &wanted) {
            let offset: usize = lines[..index].iter().map(|line| line.len() + 1).sum();
            if code[value_start..].trim_start().starts_with(['{', '[']) {
                let (line_number, column) = line_column(source, offset + value_start);
                return Some((line_number, column, column + 1));
            }
            return (value_start < value_end)
                .then(|| span_between(source, offset + value_start, offset + value_end));
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let lines: Vec<&str> = source.lines().collect();
    let (line_index, line_start) = line_at(source, &lines, cursor);
    let line = lines.get(line_index).copied().unwrap_or("");
    let caret_in_line = cursor.saturating_sub(line_start).min(line.len());
    let before_caret = &line[..caret_in_line];
    let (code_before, _) = split_toml_comment(before_caret);
    let mut path: Vec<SchemaPathSegment> = table_path_at(&lines, line_index);
    let (code_line, _) = split_toml_comment(line);

    // The caret is on a table header: suggest child tables, not keys.
    if code_before.trim_start().starts_with('[') {
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent: String::new(),
            entry_prefix: "",
            explicit_entry_start: false,
            entry_suffix: "",
            ..Default::default()
        };
    }

    // Inside `key = { … }`: the path descends through each enclosing inline
    // table and the typed token follows the last `{` or `,`.
    if let Some(equals) = assignment_equals(code_before) {
        let after_equals = &code_before[equals + 1..];
        let spaces = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
        let value_text = &after_equals[spaces..];
        if value_text.starts_with('{') && inline_depth(value_text) > 0 {
            let raw_key = code_line[..assignment_equals(code_line).unwrap_or(equals)]
                .trim()
                .to_string();
            let mut full = path.clone();
            full.extend(
                split_key_path(&unquote_key(&raw_key))
                    .unwrap_or_else(|| vec![unquote_key(&raw_key)])
                    .into_iter()
                    .map(SchemaPathSegment::Key),
            );
            let brace = equals + 1 + spaces;
            let full = inline_table_path(&full, code_before, brace);
            let segment_start = code_before.rfind([',', '{']).map_or(0, |index| index + 1);
            let raw = &code_before[segment_start..];
            let typed = raw.trim_start();
            let typed_start = line_start + segment_start + (raw.len() - typed.len());
            return CompletionContext {
                typed: typed.trim_end().to_string(),
                replace_start: utf16(&source[..typed_start]),
                replace_end: utf16(&source[..cursor]),
                value_key: None,
                value_needs_space: false,
                is_array_item: false,
                base_indent: String::new(),
                path: full,
                entry_prefix: "",
                explicit_entry_start: false,
                entry_suffix: "",
                position: EntryPosition::Inline,
                ..Default::default()
            };
        }
    }

    // Value position.
    if let Some(equals) = assignment_equals(code_before) {
        let raw_key = code_line[..assignment_equals(code_line).unwrap_or(equals)]
            .trim()
            .to_string();
        let key = unquote_key(&raw_key);
        let mut full = path.clone();
        // In `a.b = |` only `b` is the value's key; `a` extends the path.
        let mut dotted: Vec<SchemaPathSegment> = split_key_path(&key)
            .unwrap_or_else(|| vec![key.clone()])
            .into_iter()
            .map(SchemaPathSegment::Key)
            .collect();
        let value_key = dotted.pop().and_then(|segment| match segment {
            SchemaPathSegment::Key(key) => Some(key),
            SchemaPathSegment::ArrayItem(_) => None,
        });
        full.extend(dotted);
        let after_equals = &code_before[equals + 1..];
        let value_start = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
        let typed_start = line_start + equals + 1 + value_start;
        let line_end = line_start + line.len();
        // Replace the scalar under the caret, but not a trailing comment.
        let value_len = value_token_len(&source[cursor..line_end.min(source.len())]);
        return CompletionContext {
            typed: source[typed_start..cursor].trim_end().to_string(),
            replace_start: utf16(&source[..typed_start]),
            replace_end: utf16(&source[..cursor + value_len]),
            value_key,
            value_needs_space: source[typed_start..cursor].is_empty()
                && !after_equals.trim_start().is_empty(),
            is_array_item: false,
            base_indent: String::new(),
            path: full,
            entry_prefix: "",
            explicit_entry_start: false,
            entry_suffix: "",
            ..Default::default()
        };
    }

    // Key position.
    let token_start_in_line = code_before.rfind([' ', '\t']).map_or(0, |index| index + 1);
    let typed_start = line_start + token_start_in_line;
    let raw_typed = code_before[token_start_in_line..].trim_start();
    let typed_start = typed_start + (code_before[token_start_in_line..].len() - raw_typed.len());
    // A dotted key in progress (`database.po|`) completes the last segment
    // against the nested table; the earlier segments extend the path.
    let (query_start, typed) = match raw_typed.rfind('.') {
        Some(dot) if !raw_typed[..dot].trim().is_empty() => {
            let parent = raw_typed[..dot].trim();
            let typed = raw_typed[dot + 1..].trim_start();
            if let Some(parents) = split_key_path(parent) {
                path.extend(parents.into_iter().map(SchemaPathSegment::Key));
            }
            (
                typed_start + (raw_typed.len() - typed.len()),
                typed.to_string(),
            )
        }
        _ => (typed_start, raw_typed.to_string()),
    };
    CompletionContext {
        typed,
        replace_start: utf16(&source[..query_start]),
        replace_end: utf16(&source[..cursor]),
        value_key: None,
        value_needs_space: false,
        is_array_item: false,
        base_indent: String::new(),
        path,
        entry_prefix: "",
        explicit_entry_start: false,
        entry_suffix: "",
        ..Default::default()
    }
}

/// Index of the line containing `byte` and that line's start offset.
fn line_at(source: &str, lines: &[&str], byte: usize) -> (usize, usize) {
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        if byte <= offset + line.len() {
            return (index, offset);
        }
        offset += line.len() + 1;
    }
    (lines.len(), source.len())
}

/// Length of the scalar value starting at the caret: runs to the comment or
/// line end for bare scalars, to the closing quote for strings.
fn value_token_len(text: &str) -> usize {
    let trimmed_end = text.trim_end_matches([' ', '\t']);
    if trimmed_end.starts_with('"') {
        let mut escaped = false;
        for (index, character) in trimmed_end.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                return index + 1;
            }
        }
        return trimmed_end.len();
    }
    if let Some(rest) = trimmed_end.strip_prefix('\'') {
        return rest.find('\'').map_or(trimmed_end.len(), |index| index + 2);
    }
    let (code, _) = split_toml_comment(trimmed_end);
    code.trim_end().len()
}

/// Keys already present in the caret's table, ignoring the active line so
/// its partly typed key can still complete.
fn existing_table_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    let document: Value = toml_parse(source).ok()?;
    let mapping = path
        .iter()
        .try_fold(&document, |value, segment| match segment {
            SchemaPathSegment::Key(key) => value.get(key),
            SchemaPathSegment::ArrayItem(index) => value.get(index),
        })?;
    let table = mapping.as_object()?;
    let lines: Vec<&str> = source.lines().collect();
    let (line_index, _) = line_at(source, &lines, cursor_byte);
    let current_key = lines
        .get(line_index)
        .and_then(|line| key_assignment(split_toml_comment(line).0))
        .map(|(key, _, _)| {
            split_key_path(&key)
                .and_then(|segments| segments.last().cloned())
                .unwrap_or(key)
        });
    Some(
        table
            .keys()
            .filter(|key| current_key.as_deref() != Some(key.as_str()))
            .cloned()
            .collect(),
    )
}

/// Insert text for a property completion. Scalar values render as TOML
/// (`key = value`); objects expand to a `[table]` header plus their required
/// children, with the caret on the first value to fill.
fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    context: &InsertContext<'_>,
) -> Option<(String, usize)> {
    let toml_key = toml_key_for_completion(key)?;
    // Inside an inline table a nested object stays inline; a header is invalid there.
    if context.position == EntryPosition::Inline
        && let Some(branch) = object_branch(property, root)
        && schema_const(property, root).is_none()
        && array_branch(property, root).is_none()
    {
        if let Some(default) = schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(|map| map.is_empty()))
        {
            let insert = format!("{toml_key} = {}", toml_inline_table(default));
            let cursor = insert.encode_utf16().count();
            return Some((insert, cursor));
        }
        let mut insert = format!("{toml_key} = {{");
        let mut cursor = None;
        let mut first = true;
        for (name, child) in required_properties(branch) {
            let Some(child_key) = toml_key_for_completion(name) else {
                continue;
            };
            insert.push_str(if first { " " } else { ", " });
            first = false;
            insert.push_str(&child_key);
            insert.push_str(" = ");
            if cursor.is_none() {
                cursor = Some(insert.encode_utf16().count());
            }
            if let Some(default) = schema_default(child, root) {
                insert.push_str(&toml_text(default));
            }
        }
        // No required children: the caret lands between padded braces.
        if first {
            insert.push(' ');
        }
        let cursor = cursor.unwrap_or_else(|| insert.encode_utf16().count());
        insert.push_str(" }");
        return Some((insert, cursor));
    }
    if let Some(value) = schema_const(property, root) {
        let insert = format!("{toml_key} = {}", toml_text(value));
        let cursor = insert.encode_utf16().count();
        return Some((insert, cursor));
    }
    if array_branch(property, root).is_some() {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => {
                let insert = format!("{toml_key} = {}", toml_text(default));
                let cursor = insert.encode_utf16().count();
                return Some((insert, cursor));
            }
            None => {
                let insert = format!("{toml_key} = []");
                let cursor = insert.len() - 1;
                return Some((insert, cursor));
            }
        }
    }
    if let Some(branch) = object_branch(property, root) {
        if let Some(default) = schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(|map| map.is_empty()))
        {
            let insert = format!("{toml_key} = {}", toml_inline_table(default));
            let cursor = insert.encode_utf16().count();
            return Some((insert, cursor));
        }
        // Headers name the full path (`[web.letsencrypt]` inside `[web]`).
        // They cannot address array items, so beneath one the object is inline.
        let mut header = Vec::new();
        for segment in context.path {
            match segment {
                SchemaPathSegment::Key(parent) => header.push(toml_key_for_completion(parent)?),
                SchemaPathSegment::ArrayItem(_) => {
                    return property_insert(
                        key,
                        property,
                        root,
                        &InsertContext {
                            position: EntryPosition::Inline,
                            ..*context
                        },
                    );
                }
            }
        }
        header.push(toml_key);
        let mut insert = format!("[{}]\n", header.join("."));
        let mut cursor = None;
        for (name, child) in required_properties(branch) {
            let Some(child_key) = toml_key_for_completion(name) else {
                continue;
            };
            insert.push_str(&child_key);
            insert.push_str(" = ");
            if cursor.is_none() {
                cursor = Some(insert.encode_utf16().count());
            }
            if let Some(default) = schema_default(child, root) {
                insert.push_str(&toml_text(default));
            }
            insert.push('\n');
        }
        let cursor = cursor.unwrap_or_else(|| insert.encode_utf16().count());
        return Some((insert, cursor));
    }
    if let Some(default) = schema_default(property, root) {
        let insert = format!("{toml_key} = {}", toml_text(default));
        let cursor = insert.encode_utf16().count();
        return Some((insert, cursor));
    }
    let insert = format!("{toml_key} = ");
    let cursor = insert.encode_utf16().count();
    Some((insert, cursor))
}

/// The required properties of an object schema that have a schema of their
/// own, in the order the schema lists them.
fn required_properties(branch: &Value) -> Vec<(&str, &Value)> {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|name| properties.get(name).map(|child| (name, child)))
        .collect()
}

/// Non-string scalars render as JSON, which TOML also accepts.
fn toml_text(value: &Value) -> String {
    match value {
        Value::String(text) if is_bare_scalar(text) => text.clone(),
        Value::String(text) => serde_json::to_string(text).unwrap_or_else(|_| text.clone()),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(toml_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => toml_inline_table(&Value::Object(map.clone())),
        // TOML has no null; `nan` reads back through `serde_json` as `null`.
        Value::Null => "nan".to_string(),
        _ => value.to_string(),
    }
}

fn toml_inline_table(value: &Value) -> String {
    let Value::Object(map) = value else {
        return toml_text(value);
    };
    let pairs: Vec<String> = map
        .iter()
        .filter_map(|(key, child)| {
            toml_key_for_completion(key)
                .map(|toml_key| format!("{toml_key} = {}", toml_text(child)))
        })
        .collect();
    format!("{{ {} }}", pairs.join(", "))
}

fn is_bare_scalar(_text: &str) -> bool {
    // Always quote: a bare string could read back as a date or number.
    false
}

/// Bare when the key matches TOML's bare-key grammar, quoted otherwise.
/// Dotted keys quote per segment.
fn toml_key_for_completion(key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let rendered: Vec<String> = key
        .split('.')
        .map(|segment| {
            if !segment.is_empty()
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
            {
                segment.to_string()
            } else if segment.is_empty() {
                format!("\"{segment}\"")
            } else {
                serde_json::to_string(segment).unwrap_or_else(|_| format!("\"{segment}\""))
            }
        })
        .collect();
    Some(rendered.join("."))
}

/// Key under `column` (a character index) on a single line: the key before
/// `=`, or the last header segment on a `[table]` line.
fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let (code, _) = split_toml_comment(line);
    if let Some((segments, _)) = parse_header(code) {
        let last = segments.last()?.clone();
        let start_column = line.len() - line.trim_start().len();
        let end_column = line.trim_end().len();
        return (column >= start_column && column <= end_column).then_some((last, start_column));
    }
    let (key, start, _) = key_assignment(code)?;
    let start_column = line[..start].chars().count();
    let end_column = start_column + line[start..start + key.len()].chars().count();
    let dotted = split_key_path(&key).unwrap_or_else(|| vec![key.clone()]);
    if dotted.len() > 1 {
        let mut offset = start_column;
        for segment in &dotted {
            let segment_len = segment.chars().count();
            if column >= offset && column < offset + segment_len {
                return Some((segment.clone(), offset));
            }
            offset += segment_len + 1;
        }
        return None;
    }
    (column >= start_column && column < end_column).then_some((key, start_column))
}

// ---------------------------------------------------------------------------
// Editing and highlighting
// ---------------------------------------------------------------------------

/// Keeps the current indentation; TOML never indents after a bracket.
fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    format!("\n{indent}")
}

/// Comment toggling over whole lines with `#`, mirroring the YAML toggler's
/// selection mapping: blank lines are skipped, commented selections uncomment.
fn toggle_toml_line_comments(
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

/// The unclosed string quote at the caret, or comment state: the pairing guard.
fn toml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..byte];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '#'
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (None, true);
        }
    }
    (quote, false)
}

/// Table headers and `key = value` lines score; JSON braces and YAML colons do not.
fn toml_confidence(source: &str) -> u8 {
    let mut score = 0u8;
    for line in source.lines().take(50) {
        let (code, _) = split_toml_comment(line);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }
        if parse_header(code).is_some() {
            score = score.saturating_add(30);
        } else if assignment_equals(code).is_some() {
            score = score.saturating_add(5);
        }
        if score >= 200 {
            break;
        }
    }
    score.min(200)
}

fn highlight_line(line: &str) -> AnyView {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, content) = line.split_at(indent_len);
    let (code, comment) = split_toml_comment(content);
    if code.trim().is_empty() {
        return match comment {
            Some(comment) => view! {
                <span>{indent}<span class="config-editor__comment">{comment.to_string()}</span></span>
            }
            .into_any(),
            None => view! { <span>{line.to_string()}</span> }.into_any(),
        };
    }
    if code.trim_start().starts_with('[') {
        return view! {
            <span>
                {indent}
                <span class="config-editor__punctuation">{code.trim().to_string()}</span>
                {comment.map(|comment| view! { <span class="config-editor__comment">{comment.to_string()}</span> })}
            </span>
        }
        .into_any();
    }
    if let Some((_, start, end)) = key_assignment(code) {
        let equals_at = code[end..].find('=').map(|index| end + index);
        let Some(equals) = equals_at else {
            return view! { <span>{line.to_string()}</span> }.into_any();
        };
        // Re-emit the whitespace after `=` so the overlay stays byte-identical
        // to the textarea.
        let after_equals = &code[equals + 1..];
        let gap_len = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
        let scalar_start = equals + 1 + gap_len;
        let (scalar, trailing) = split_toml_comment(&code[scalar_start..]);
        let class = scalar_class(scalar);
        return view! {
            <span>
                {indent}
                {code[..start].to_string()}
                <span class="config-editor__key">{code[start..end].to_string()}</span>
                {code[end..scalar_start].to_string()}
                <span class=class>{scalar.to_string()}</span>
                {trailing.map(|trailing| view! { <span class="config-editor__comment">{trailing.to_string()}</span> })}
                {comment.map(|comment| view! { <span class="config-editor__comment">{comment.to_string()}</span> })}
            </span>
        }
        .into_any();
    }
    view! { <span>{line.to_string()}</span> }.into_any()
}

fn scalar_class(scalar: &str) -> &'static str {
    let scalar = scalar.trim();
    if scalar.is_empty() {
        "config-editor__value"
    } else if matches!(scalar, "true" | "false") || scalar.parse::<f64>().is_ok() {
        "config-editor__literal"
    } else if scalar.starts_with(['"', '\'']) || scalar.starts_with(['{', '[']) {
        "config-editor__string"
    } else {
        "config-editor__value"
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

impl super::super::codec::Codec for TomlBehavior {
    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        use super::super::codec::first_pointer;
        if !value.is_object() {
            return Err("a TOML document is a table: the top level must be an object".to_string());
        }
        if let Some(pointer) = first_pointer(value, &Value::is_null) {
            return Err(format!("TOML has no null value (at {pointer})"));
        }
        toml::to_string(value).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::completion::{
        Completion, accept_marked, apply_completion, completions, key_hover,
    };
    #[cfg(feature = "schema-validation")]
    use super::super::super::diagnostics::validate;
    use super::super::super::diagnostics::{SchemaBundle, build_schema};
    use super::super::super::language::LanguageBehavior;
    use super::TomlBehavior;

    fn schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{
                "listen":{"type":"integer","description":"Port"},
                "insecure_plaintext":{"type":"boolean"},
                "server":{"type":"object","required":["host"],"properties":{
                    "host":{"type":"string","default":"localhost"},
                    "port":{"type":"integer","default":7835}}},
                "clients":{"type":"object","additionalProperties":{"type":"object",
                    "properties":{"secret":{"type":"string"},"max_connections":{"type":"integer"}},"required":["secret"]}}
            },"additionalProperties":false}"#,
        )
        .unwrap()
    }

    fn complete(source: &str, schema: &SchemaBundle) -> Vec<Completion> {
        completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(schema),
            true,
            &TomlBehavior,
        )
    }

    fn nested_schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{"web":{"type":"object","properties":{
                "http":{"type":"integer"},
                "letsencrypt":{"type":"object","required":["email"],
                    "properties":{"email":{"type":"string"}}},
                "headers":{"type":"object"}}}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn an_object_accepted_inside_an_inline_table_stays_an_inline_table() {
        let schema = nested_schema();
        let accepted = accept_marked("web = { le| }\n", "letsencrypt", &schema, &TomlBehavior);
        assert_eq!(accepted, "web = { letsencrypt = { email = | } }\n");
    }

    #[test]
    fn an_object_without_required_children_inside_an_inline_table_opens_empty_braces() {
        let schema = nested_schema();
        let accepted = accept_marked("web = { he| }\n", "headers", &schema, &TomlBehavior);
        assert_eq!(accepted, "web = { headers = { | } }\n");
    }

    #[test]
    fn an_object_accepted_under_a_table_header_opens_the_nested_table_path() {
        let schema = nested_schema();
        let accepted = accept_marked("[web]\nle|", "letsencrypt", &schema, &TomlBehavior);
        assert_eq!(accepted, "[web]\n[web.letsencrypt]\nemail = |\n");
        let document = TomlBehavior
            .parse(&accepted.replace('|', "\"a@b.c\""))
            .expect("the accepted document parses");
        assert_eq!(document["web"]["letsencrypt"]["email"], "a@b.c");
    }

    #[test]
    fn a_dotted_key_completes_only_its_last_segment() {
        let schema = nested_schema();
        let accepted = accept_marked("[web]\nletsencrypt.em|", "email", &schema, &TomlBehavior);
        assert_eq!(accepted, "[web]\nletsencrypt.email = |");
    }

    #[test]
    fn duplicate_keys_are_a_parse_error_on_the_repeated_line() {
        let error = TomlBehavior.parse("listen = 1\nlisten = 2\n").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(!error.message.is_empty(), "{}", error.message);
    }

    #[test]
    fn syntax_errors_report_a_position() {
        let error = TomlBehavior.parse("listen = \n").unwrap_err();
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some());
    }

    #[test]
    fn tables_parse_into_nested_values() {
        let document = match TomlBehavior.parse("[server]\nhost = \"example.com\"\n") {
            Ok(document) => document,
            Err(error) => panic!("parse failed: {}", error.message),
        };
        assert_eq!(
            document.get("server").and_then(|server| server.get("host")),
            Some(&serde_json::Value::String("example.com".to_string()))
        );
    }

    #[test]
    fn keys_are_suggested_and_a_partial_key_is_replaced() {
        let source = "lis";
        let options = complete(source, &schema());
        let option = options
            .iter()
            .find(|option| option.label == "listen")
            .expect("listen should be suggested");
        let (updated, _) = apply_completion(source, option);
        assert_eq!(updated, "listen = ");
    }

    #[test]
    fn table_members_are_suggested_under_their_header() {
        let source = "[server]\nho";
        let schema_bundle = schema();
        let options = completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        assert!(
            options.iter().any(|option| option.label == "host"),
            "host should be suggested, have {:?}",
            options
                .iter()
                .map(|option| &option.label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_boolean_value_is_suggested_after_its_equals() {
        let source = "insecure_plaintext = ";
        let options = complete(source, &schema());
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        assert!(labels.contains(&"true"), "{labels:?}");
        assert!(labels.contains(&"false"), "{labels:?}");
    }

    #[test]
    fn existing_table_keys_are_filtered_from_suggestions() {
        let schema_bundle = schema();
        let source = "[server]\nhost = \"x\"\npo";
        let options = completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        assert!(!labels.contains(&"host"), "{labels:?}");
        assert!(labels.contains(&"port"), "{labels:?}");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_an_inline_table_is_reported_on_its_own_line() {
        let source = "listen = 7835\n[clients]\nacme = { secret = 1 }\n";
        let diagnostics = validate(source, Some(&schema()), None, true, &TomlBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        // Column of the `1` inside the inline table.
        assert_eq!(diagnostics[0].column, Some(19));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_a_nested_inline_table_is_reported_on_its_own_line() {
        let schema_bundle = build_schema(
            r#"{"type":"object","properties":{"outer":{"type":"object","properties":{
                "inner":{"type":"object","properties":{"count":{"type":"integer"}}}}}}}"#,
        )
        .unwrap();
        let source = "outer = { inner = { count = \"x\" } }\n";
        let diagnostics = validate(source, Some(&schema_bundle), None, true, &TomlBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
        assert_eq!(diagnostics[0].column, Some(29));
    }

    #[test]
    fn keys_inside_an_inline_table_are_suggested_from_the_assigned_key_schema() {
        let schema_bundle = schema();
        let source = "[clients]\nacme = { se";
        let options = completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        assert!(
            options.iter().any(|option| option.label == "secret"),
            "secret should be suggested inside the inline table, have {:?}",
            options
                .iter()
                .map(|option| &option.label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn keys_inside_a_nested_inline_table_are_suggested() {
        let schema_bundle = schema();
        let source = "clients = { acme = { se";
        let options = completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        assert!(
            options.iter().any(|option| option.label == "secret"),
            "secret should be suggested inside the nested inline table, have {:?}",
            options
                .iter()
                .map(|option| &option.label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn entries_after_comma_inside_a_nested_inline_table_complete_from_it() {
        let schema_bundle = schema();
        let source = "clients = { acme = { secret = \"x\", ma";
        let options = completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        assert!(
            options
                .iter()
                .any(|option| option.label == "max_connections"),
            "max_connections should be suggested after the first entry, have {:?}",
            options
                .iter()
                .map(|option| &option.label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_null_schema_value_inserts_nan_and_reads_back_as_null() {
        let schema_bundle = build_schema(
            r#"{"type":"object","properties":{
                "max_connections":{"type":["integer","null"],"minimum":1}}}"#,
        )
        .unwrap();
        // An explicit request: an empty value hides the null option by default.
        let source = "max_connections = ";
        let options = completions(
            source,
            source.encode_utf16().count(),
            true,
            Some(&schema_bundle),
            true,
            &TomlBehavior,
        );
        let option = options
            .iter()
            .find(|option| option.label == "null")
            .expect("the null option should be suggested");
        let (updated, _) = apply_completion(source, option);
        assert_eq!(updated, "max_connections = nan");
        let parsed = match TomlBehavior.parse(&updated) {
            Ok(parsed) => parsed,
            Err(error) => panic!("nan should be valid TOML: {}", error.message),
        };
        assert_eq!(parsed["max_connections"], serde_json::Value::Null);
    }

    #[test]
    fn object_properties_expand_to_a_table_header_with_required_children() {
        let source = "";
        let schema_bundle = schema();
        let options = completions(source, 0, true, Some(&schema_bundle), true, &TomlBehavior);
        let option = options
            .iter()
            .find(|option| option.label == "server")
            .expect("server should be suggested");
        let (updated, _) = apply_completion(source, option);
        assert_eq!(updated, "[server]\nhost = \"localhost\"\n");
    }

    #[test]
    fn toggle_comments_adds_and_removes_hash_prefixes() {
        let source = "listen = 7835\n";
        let (commented, _, _) = TomlBehavior
            .toggle_comments(source, 0, source.encode_utf16().count())
            .unwrap();
        assert_eq!(commented, "# listen = 7835\n");
        let (restored, _, _) = TomlBehavior
            .toggle_comments(&commented, 0, commented.encode_utf16().count())
            .unwrap();
        assert_eq!(restored, source);
    }

    #[test]
    fn hovering_a_key_reports_its_schema_type() {
        let hover = key_hover(
            "[server]\nhost = \"x\"\n",
            1,
            1,
            Some(&schema()),
            &TomlBehavior,
        )
        .expect("hover should resolve");
        assert_eq!(hover.key, "host");
        assert_eq!(hover.type_label, "string");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn schema_violations_point_at_the_offending_value() {
        let diagnostics = validate(
            "listen = \"not-a-port\"\n",
            Some(&schema()),
            None,
            true,
            &TomlBehavior,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
        let schema_bundle = schema();
        for source in [
            "listen = ",
            "[server\nhost = ",
            "[[clients]]\nsecret = ",
            "a.b = ",
            "# only a comment\n",
            "",
        ] {
            for cursor in 0..=source.encode_utf16().count() {
                let _ = TomlBehavior.parse(source);
                let _ = TomlBehavior.completion_context(source, cursor);
                let _ = TomlBehavior.lexical_state(source, cursor);
                for force in [false, true] {
                    for option in completions(
                        source,
                        cursor,
                        force,
                        Some(&schema_bundle),
                        true,
                        &TomlBehavior,
                    ) {
                        let _ = apply_completion(source, &option);
                    }
                }
            }
        }
    }
}
