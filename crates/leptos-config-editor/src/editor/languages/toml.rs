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

use super::super::inline::{InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_keys, inline_sibling_keys, inline_value_len, statement_start, entry_follows};
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{bracket_newline, toggle_line_comments, CompletionContext, Newline, EntryPosition, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment,
};
use super::super::highlight::{Token, TokenKind, Tokens};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct TomlBehavior;


impl ConfigLanguage for TomlBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "TOML",
            icon: "T",
            extensions: &["toml"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        toml_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        toml_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        toml_value_span(source, pointer).map(Span::from)
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_line_span(source).map(Span::from)
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

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        hover_context_offset(line, column)
    }

    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        dotted_hover_path(self, source, line_index, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        inline_table_exit(source, cursor).unwrap_or_else(|| {
            bracket_newline(source, cursor, &[('[', ']')], |line| split_toml_comment(line).0)
        })
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_line_comments(source, selection_start, selection_end, &["#"])
    }

    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        // A third quote typed in a row opens a multi-line string.
        if !matches!(typed, '"' | '\'') {
            return None;
        }
        let byte = utf16_to_byte_index(source, cursor);
        let before = &source[..byte];
        let pair: String = [typed, typed].iter().collect();
        let opens = before.ends_with(&pair)
            && !before[..before.len() - 2].ends_with(typed)
            && toml_lexical_state(source, cursor.saturating_sub(2)).0.is_none();
        opens.then(|| [typed; 3].iter().collect())
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = toml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }


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
    table_paths(&lines[..line_index.min(lines.len())])
        .pop()
        .unwrap_or_default()
}

/// The table path in effect after each line, in one pass: `[[a.b]]` adds the
/// entry's index, and a later `[a.b.c]` belongs to the latest `a.b` entry.
/// Entry `i` is the path for line `i + 1`; a leading empty path is for line 0.
fn table_paths(lines: &[&str]) -> Vec<Vec<SchemaPathSegment>> {
    let mut paths = Vec::with_capacity(lines.len() + 1);
    let mut path: Vec<SchemaPathSegment> = Vec::new();
    let mut arrays: HashMap<String, usize> = HashMap::new();
    paths.push(path.clone());
    for line in lines {
        let (code, _) = split_toml_comment(line);
        if let Some((segments, is_array)) = parse_header(code) {
            path = Vec::with_capacity(segments.len() + 2);
            for (depth, segment) in segments.iter().enumerate() {
                path.push(SchemaPathSegment::Key(segment.clone()));
                let own = depth + 1 == segments.len() && is_array;
                if !own && let Some(count) = arrays.get(&segments[..=depth].join(".")) {
                    path.push(SchemaPathSegment::ArrayItem(count.saturating_sub(1)));
                }
            }
            if is_array {
                let count = arrays.entry(segments.join(".")).or_insert(0);
                path.push(SchemaPathSegment::ArrayItem(*count));
                *count += 1;
            }
        }
        paths.push(path.clone());
    }
    paths
}

/// Array indices render as numbers, matching the validator's JSON-pointer segments.
fn path_strings(path: &[SchemaPathSegment]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            SchemaPathSegment::Key(key) => key.clone(),
            SchemaPathSegment::ArrayItem(index) => index.to_string(),
        })
        .collect()
}

/// One entry inside an inline table, with byte offsets relative to the line
/// and its full schema path (outer table + inline key chain).
struct InlineEntry {
    key: (usize, usize),
    value: (usize, usize),
    path: Vec<String>,
}

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

fn toml_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let lines: Vec<&str> = source.lines().collect();
    let tables = table_paths(&lines);
    for (index, line) in lines.iter().enumerate() {
        let table = path_strings(&tables[index]);
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
            && path_strings(&tables[index + 1]) == wanted
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

fn toml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let lines: Vec<&str> = source.lines().collect();
    let tables = table_paths(&lines);
    for (index, line) in lines.iter().enumerate() {
        let table = path_strings(&tables[index]);
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

    // The caret is on a table header: suggest child tables, not keys. Past a
    // complete header, the context is that table (hover documents it).
    if code_before.trim_start().starts_with('[') {
        let path = match parse_header(code_before.trim()) {
            Some((segments, _)) => segments.into_iter().map(SchemaPathSegment::Key).collect(),
            None => path,
        };
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent: String::new(),
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }

    // Inside `key = { … }` or `key = [ … ]` opened on this line.
    // The statement may span lines (`certificates = [` … the caret).
    if let Some(statement) = statement_start(&source[..cursor])
        && let Some(caret) = inline_caret(&source[statement..cursor], b'=')
    {
        let code_before = &source[statement..cursor];
        let mut full = path.clone();
        for (index, segment) in caret.path.into_iter().enumerate() {
            // The line's own key may be dotted (`a.b = { … }`).
            match segment {
                SchemaPathSegment::Key(key) if index == 0 => full.extend(
                    split_key_path(&key)
                        .unwrap_or_else(|| vec![key])
                        .into_iter()
                        .map(SchemaPathSegment::Key),
                ),
                segment => full.push(segment),
            }
        }
        let line_end = (line_start + line.len()).min(source.len());
        let value_end = cursor + inline_value_len(&source[cursor..line_end]);
        let at = |offset: usize| statement + offset;
        return match caret.entry {
            CaretEntry::Key { start } => CompletionContext {
                typed: code_before[start..].trim_end().to_string(),
                replace_start: utf16(&source[..at(start)]),
                replace_end: utf16(&source[..cursor]),
                path: full,
                position: EntryPosition::Inline,
                ..Default::default()
            },
            CaretEntry::Value { key, start } => CompletionContext {
                typed: code_before[start..].trim_end().to_string(),
                replace_start: utf16(&source[..at(start)]),
                replace_end: utf16(&source[..value_end]),
                value_needs_space: start == code_before.len() && !code_before.ends_with([' ', '\t']),
                value_key: Some(key),
                path: full,
                position: EntryPosition::Inline,
                ..Default::default()
            },
            CaretEntry::Item { index, start } => {
                full.push(SchemaPathSegment::ArrayItem(index));
                CompletionContext {
                    typed: code_before[start..].trim_end().to_string(),
                    replace_start: utf16(&source[..at(start)]),
                    replace_end: utf16(&source[..value_end]),
                    is_array_item: true,
                    entry_wrap: Some(("{ ", " }")),
                    path: full,
                    position: EntryPosition::Inline,
                    ..Default::default()
                }
            }
        };
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
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
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
    // Typed in front of an existing entry (`lim|listen = 1`): the new entry
    // goes before it, which moves to its own line.
    let rest = split_toml_comment(&line[caret_in_line..]).0;
    let splits = entry_follows(rest, b'=');
    let word_after: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    let indent: String = line.chars().take_while(|character| matches!(character, ' ' | '\t')).collect();
    // A `[table]` header here would capture every key below it in this table,
    // so tables are written inline when more keys follow.
    let keys_follow = splits
        || lines
            .iter()
            .skip(line_index + 1)
            .map(|line| split_toml_comment(line).0)
            .take_while(|code| parse_header(code).is_none())
            .any(|code| key_assignment(code).is_some());
    CompletionContext {
        typed,
        replace_start: utf16(&source[..query_start]),
        replace_end: utf16(&source[..cursor]),
        path,
        entry_suffix: if splits { format!("\n{indent}") } else { String::new() },
        entry_after: splits.then_some(word_after),
        position: if keys_follow { EntryPosition::Inline } else { EntryPosition::Block },
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
    // A line scan, not a parse: mid-typing the document rarely parses.
    let lines: Vec<&str> = source.lines().collect();
    let (line_index, _) = line_at(source, &lines, cursor_byte);
    if let Some(statement) = statement_start(&source[..cursor_byte])
        && let Some(keys) = inline_sibling_keys(&source[statement..], b'=', cursor_byte - statement)
    {
        return Some(keys.into_iter().collect());
    }
    let child_of = |full: &[SchemaPathSegment]| match full.get(path.len()) {
        Some(SchemaPathSegment::Key(key)) if full.starts_with(path) => Some(key.clone()),
        _ => None,
    };
    let mut keys = HashSet::new();
    let mut section: Vec<SchemaPathSegment> = Vec::new();
    let mut array_tables: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        let (code, _) = split_toml_comment(line);
        if let Some((segments, is_array)) = parse_header(code) {
            // Every prefix that is an array of tables addresses its latest
            // entry: `[a.b.c]` after `[[a.b]]` is `a.b[last].c`.
            let mut header: Vec<SchemaPathSegment> = Vec::new();
            for (depth, segment) in segments.iter().enumerate() {
                header.push(SchemaPathSegment::Key(segment.clone()));
                let prefix = segments[..=depth].join(".");
                let own = depth + 1 == segments.len() && is_array;
                if !own && let Some(count) = array_tables.get(&prefix) {
                    header.push(SchemaPathSegment::ArrayItem(count.saturating_sub(1)));
                }
            }
            keys.extend(child_of(&header));
            section = header;
            if is_array {
                let seen = array_tables.entry(segments.join(".")).or_insert(0);
                section.push(SchemaPathSegment::ArrayItem(*seen));
                *seen += 1;
            }
            continue;
        }
        if index == line_index {
            continue;
        }
        if let Some((key, _, _)) = key_assignment(code) {
            let mut full = section.clone();
            full.extend(
                split_key_path(&key)
                    .unwrap_or_else(|| vec![unquote_key(&key)])
                    .into_iter()
                    .map(SchemaPathSegment::Key),
            );
            keys.extend(child_of(&full));
        }
    }
    Some(keys)
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
            // The first child takes the caret; the others get a valid
            // placeholder so the document stays parseable.
            let takes_caret = cursor.is_none();
            if takes_caret {
                cursor = Some(insert.encode_utf16().count());
            }
            match schema_default(child, root) {
                Some(default) => insert.push_str(&toml_text(default)),
                None if !takes_caret => insert.push_str(&toml_text(&placeholder(child, root))),
                // The caret's own child: a string starts quoted, caret inside.
                None if placeholder(child, root).is_string() => {
                    insert.push_str("\"\"");
                    cursor = cursor.map(|position| position + 1);
                }
                None => {}
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
    if let Some(array) = array_branch(property, root) {
        // An array of tables at table level is idiomatic as `[[path.key]]`
        // with the item's required keys.
        if context.position == EntryPosition::Block
            && let Some(item) = array
                .get("items")
                .and_then(|items| object_branch(items, root))
            && let Some(header) = table_header(context.path, &toml_key)
        {
            let mut insert = format!("[[{header}]]\n");
            let mut cursor = None;
            for (name, child) in required_properties(item) {
                if cursor.is_some() {
                    // Later children get a valid placeholder.
                    let Some(child_key) = toml_key_for_completion(name) else {
                        continue;
                    };
                    insert.push_str(&format!("{child_key} = {}\n", toml_text(&placeholder(child, root))));
                    continue;
                }
                let inline = InsertContext {
                    position: EntryPosition::Inline,
                    path: &[],
                    ..*context
                };
                let Some((line, caret)) = property_insert(name, child, root, &inline) else {
                    continue;
                };
                cursor = Some(insert.encode_utf16().count() + caret);
                insert.push_str(&line);
                insert.push('\n');
            }
            let cursor = cursor.unwrap_or_else(|| insert.encode_utf16().count());
            return Some((insert, cursor));
        }
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
        let Some(header) = table_header(context.path, &toml_key) else {
            let inline = InsertContext {
                position: EntryPosition::Inline,
                ..*context
            };
            return property_insert(key, property, root, &inline);
        };
        let mut insert = format!("[{header}]\n");
        let mut cursor = None;
        for (name, child) in required_properties(branch) {
            let Some(child_key) = toml_key_for_completion(name) else {
                continue;
            };
            insert.push_str(&child_key);
            insert.push_str(" = ");
            // The first child takes the caret; the others get a valid
            // placeholder so the document stays parseable.
            let takes_caret = cursor.is_none();
            if takes_caret {
                cursor = Some(insert.encode_utf16().count());
            }
            match schema_default(child, root) {
                Some(default) => insert.push_str(&toml_text(default)),
                None if !takes_caret => insert.push_str(&toml_text(&placeholder(child, root))),
                // The caret's own child: a string starts quoted, caret inside.
                None if placeholder(child, root).is_string() => {
                    insert.push_str("\"\"");
                    cursor = cursor.map(|position| position + 1);
                }
                None => {}
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
    if placeholder(property, root).is_string() {
        // A string value starts quoted, with the caret inside.
        let insert = format!("{toml_key} = \"\"");
        let cursor = insert.encode_utf16().count() - 1;
        return Some((insert, cursor));
    }
    let insert = format!("{toml_key} = ");
    let cursor = insert.encode_utf16().count();
    Some((insert, cursor))
}

/// The dotted header naming `key` under `path` (`web.certificates`), or
/// `None` when the path crosses an array item, which headers cannot address.
fn table_header(path: &[SchemaPathSegment], key: &str) -> Option<String> {
    let mut segments = Vec::with_capacity(path.len() + 1);
    for segment in path {
        match segment {
            SchemaPathSegment::Key(parent) => segments.push(toml_key_for_completion(parent)?),
            SchemaPathSegment::ArrayItem(_) => return None,
        }
    }
    segments.push(key.to_string());
    Some(segments.join("."))
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
        // Always quoted: a bare string could read back as a date or number.
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
    if let Some(entry) = inline_keys(code, b'=')
        .into_iter()
        .skip(1)
        .find(|entry| (entry.start..entry.end).contains(&byte_at_column(line, column)))
    {
        let key = unquote_key(&line[entry.start..entry.end]);
        return Some((key, line[..entry.start].chars().count()));
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

/// The path of one segment of a table header or dotted key: hovering `web`
/// in `[[web.certificates]]` or `websocket` in `websocket.listen = 1` shows
/// that segment's card.
fn dotted_hover_path(
    language: &TomlBehavior,
    source: &str,
    line_index: usize,
    column: usize,
) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line = source.split('\n').nth(line_index)?;
    let (code, _) = split_toml_comment(line);
    let segment_at = |text_start: usize, segments: &[String]| {
        let mut offset = text_start;
        for (index, segment) in segments.iter().enumerate() {
            let width = segment.chars().count();
            if column >= offset && column < offset + width {
                return Some((index, offset));
            }
            offset += width + 1;
        }
        None
    };
    if let Some((segments, is_array)) = parse_header(code) {
        let indent = line.len() - line.trim_start().len();
        let text_start = indent + if is_array { 2 } else { 1 };
        let (index, start) = segment_at(text_start, &segments)?;
        // Arrays of tables declared before this line address their entries.
        let before = &source[..source.split('\n').take(line_index).map(|line| line.len() + 1).sum::<usize>()];
        let arrays: HashSet<String> = before
            .lines()
            .filter_map(|line| parse_header(split_toml_comment(line).0))
            .filter(|(_, array)| *array)
            .map(|(segments, _)| segments.join("."))
            .collect();
        let mut path = Vec::new();
        for depth in 0..=index {
            path.push(SchemaPathSegment::Key(segments[depth].clone()));
            if depth < index && arrays.contains(&segments[..=depth].join(".")) {
                path.push(SchemaPathSegment::ArrayItem(0));
            }
        }
        return Some((path, segments[index].clone(), start));
    }
    let (key, start, _) = key_assignment(code)?;
    let segments = split_key_path(&key)?;
    if segments.len() < 2 {
        return None;
    }
    let (index, start_column) = segment_at(line[..start].chars().count(), &segments)?;
    let line_start: usize = source.split('\n').take(line_index).map(|line| line.len() + 1).sum();
    let mut path = language
        .completion_context(source, source[..line_start].encode_utf16().count())
        .path;
    path.extend(segments[..=index].iter().cloned().map(SchemaPathSegment::Key));
    Some((path, segments[index].clone(), start_column))
}

/// The byte offset of character `column` (0-based) in `line`.
fn byte_at_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}

/// Where hover reads the context for the key at `column`: just past its `=`,
/// so a key inside an inline table resolves under that table.
fn hover_context_offset(line: &str, column: usize) -> usize {
    let (code, _) = split_toml_comment(line);
    inline_key_at(code, b'=', byte_at_column(line, column))
        .map_or(line.len(), |key| key.after_separator)
}

// ---------------------------------------------------------------------------
// Editing and highlighting
// ---------------------------------------------------------------------------

/// Inline tables and table headers cannot span lines, so Enter inside one
/// with only closers after the caret (`{ token = "x"| }`, `[web|]`) steps
/// past them and starts the next line there.
fn inline_table_exit(source: &str, cursor: usize) -> Option<Newline> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[byte..].find('\n').map_or(source.len(), |index| byte + index);
    let rest = &source[byte..line_end];
    let closers = split_toml_comment(rest).0.trim_end();
    let before = &source[line_start..byte];
    let header = before.trim_start().starts_with('[') && !before.contains('=');
    if closers.is_empty()
        || !(closers.contains('}') || header)
        || !closers.chars().all(|character| matches!(character, '}' | ']' | ' ' | '\t' | ','))
        || toml_lexical_state(source, cursor).0.is_some()
    {
        return None;
    }
    let line = &source[line_start..];
    let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
    Some(Newline {
        text: format!("{closers}\n{indent}"),
        caret: closers.len() + 1 + indent.len(),
        replaces: closers.len(),
    })
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


fn highlight(line: &str) -> Vec<Token> {
    let mut tokens = Tokens::new();
    let code_start = line.len() - line.trim_start().len();
    let (code, comment) = split_toml_comment(&line[code_start..]);
    let comment_start = code_start + code.len();
    let push_comment = |tokens: &mut Tokens| {
        if let Some(comment) = comment {
            tokens.push(comment_start, comment_start + comment.len(), TokenKind::Comment);
        }
    };
    if code.trim().is_empty() {
        push_comment(&mut tokens);
    } else if code.starts_with('[') {
        let header_len = code.trim_end().len();
        tokens.push(code_start, code_start + header_len, TokenKind::Punctuation);
        push_comment(&mut tokens);
    } else if let Some((_, start, end)) = key_assignment(code)
        && let Some(equals) = code[end..].find('=').map(|index| end + index)
    {
        let after_equals = &code[equals + 1..];
        let gap_len = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
        let scalar_start = equals + 1 + gap_len;
        // Includes any whitespace before the comment.
        let (scalar, trailing) = split_toml_comment(&code[scalar_start..]);
        let scalar_end = scalar_start + scalar.len();
        tokens.push(code_start + start, code_start + end, TokenKind::Key);
        tokens.push(code_start + scalar_start, code_start + scalar_end, scalar_kind(scalar));
        if let Some(trailing) = trailing {
            tokens.push(
                code_start + scalar_end,
                code_start + scalar_end + trailing.len(),
                TokenKind::Comment,
            );
        }
        push_comment(&mut tokens);
    }
    tokens.finish()
}

fn scalar_kind(scalar: &str) -> TokenKind {
    let scalar = scalar.trim();
    if scalar.is_empty() {
        TokenKind::Value
    } else if matches!(scalar, "true" | "false") || scalar.parse::<f64>().is_ok() {
        TokenKind::Literal
    } else if scalar.starts_with(['"', '\'', '{', '[']) {
        TokenKind::String
    } else {
        TokenKind::Value
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------


#[cfg(test)]
mod tests {
    use super::super::super::completion::{
        Completion, accept_marked, apply_completion, completions, key_hover,
    };
    #[cfg(feature = "schema-validation")]
    use super::super::super::diagnostics::validate;
    use super::super::super::diagnostics::{SchemaBundle, build_schema};
    use super::super::super::language::ConfigLanguage;
    use super::super::super::highlight::{TokenKind, runs};
    use super::{TomlBehavior, highlight};

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
        assert_eq!(accepted, "web = { letsencrypt = { email = \"|\" } }\n");
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
        assert_eq!(accepted, "[web]\n[web.letsencrypt]\nemail = \"|\"\n");
        let document = TomlBehavior
            .parse(&accepted.replace('|', "a@b.c"))
            .expect("the accepted document parses");
        assert_eq!(document["web"]["letsencrypt"]["email"], "a@b.c");
    }

    #[test]
    fn a_dotted_key_completes_only_its_last_segment() {
        let schema = nested_schema();
        let accepted = accept_marked("[web]\nletsencrypt.em|", "email", &schema, &TomlBehavior);
        assert_eq!(accepted, "[web]\nletsencrypt.email = \"|\"");
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
        let (updated, _, _) = apply_completion(source, option);
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
        let (updated, _, _) = apply_completion(source, option);
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
        let (updated, _, _) = apply_completion(source, option);
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

    fn colored(line: &str) -> Vec<(Option<TokenKind>, String)> {
        runs(line, &highlight(line))
    }

    fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
        (kind, text.to_string())
    }

    #[test]
    fn table_headers_are_punctuation_with_a_trailing_comment() {
        assert_eq!(
            colored("  [server.http] # main"),
            vec![
                run(None, "  "),
                run(Some(TokenKind::Punctuation), "[server.http]"),
                run(None, " "),
                run(Some(TokenKind::Comment), "# main"),
            ]
        );
        assert_eq!(
            colored("[[bin]]"),
            vec![run(Some(TokenKind::Punctuation), "[[bin]]")]
        );
    }

    #[test]
    fn keys_and_equals_color_the_key_and_leave_the_separator_plain() {
        assert_eq!(
            colored("  name = plain"),
            vec![
                run(None, "  "),
                run(Some(TokenKind::Key), "name"),
                run(None, " = "),
                run(Some(TokenKind::Value), "plain"),
            ]
        );
        assert_eq!(
            colored("\"a=b\".c=x"),
            vec![
                run(Some(TokenKind::Key), "\"a=b\".c"),
                run(None, "="),
                run(Some(TokenKind::Value), "x"),
            ]
        );
    }

    #[test]
    fn quoted_values_and_inline_tables_and_arrays_are_strings() {
        for value in ["\"hi\"", "'hi'", "{ a = 1 }", "[1, 2]", "\"\"\"open"] {
            let line = format!("k = {value}");
            assert_eq!(
                colored(&line),
                vec![
                    run(Some(TokenKind::Key), "k"),
                    run(None, " = "),
                    run(Some(TokenKind::String), value),
                ],
                "{line}"
            );
        }
    }

    #[test]
    fn numbers_and_booleans_are_literals_including_whitespace_before_a_comment() {
        for value in ["true", "false", "8080", "-1.5e3", "inf"] {
            let line = format!("k = {value}");
            assert_eq!(
                colored(&line),
                vec![
                    run(Some(TokenKind::Key), "k"),
                    run(None, " = "),
                    run(Some(TokenKind::Literal), value),
                ],
                "{line}"
            );
        }
        assert_eq!(
            colored("port = 80  # http"),
            vec![
                run(Some(TokenKind::Key), "port"),
                run(None, " = "),
                run(Some(TokenKind::Literal), "80  "),
                run(Some(TokenKind::Comment), "# http"),
            ]
        );
    }

    #[test]
    fn a_hash_right_after_equals_is_a_comment_and_an_empty_value_is_uncolored() {
        assert_eq!(
            colored("k =#x"),
            vec![
                run(Some(TokenKind::Key), "k"),
                run(None, " ="),
                run(Some(TokenKind::Comment), "#x"),
            ]
        );
        assert_eq!(
            colored("k = "),
            vec![run(Some(TokenKind::Key), "k"), run(None, " = ")]
        );
    }

    #[test]
    fn comment_lines_and_unrecognized_lines() {
        assert_eq!(
            colored("   # note"),
            vec![run(None, "   "), run(Some(TokenKind::Comment), "# note")]
        );
        assert_eq!(colored("no equals here"), vec![run(None, "no equals here")]);
        assert_eq!(colored("= 1"), vec![run(None, "= 1")]);
        assert_eq!(colored(""), vec![]);
        assert_eq!(colored("   "), vec![run(None, "   ")]);
    }

    #[test]
    fn a_hash_inside_a_string_is_part_of_the_string() {
        assert_eq!(
            colored("k = \"a # b\" # c"),
            vec![
                run(Some(TokenKind::Key), "k"),
                run(None, " = "),
                run(Some(TokenKind::String), "\"a # b\" "),
                run(Some(TokenKind::Comment), "# c"),
            ]
        );
    }

    #[test]
    fn tricky_lines_are_covered_exactly_once() {
        for line in [
            "",
            "# only a comment",
            "\t# indented comment",
            "名前 = \"値 ✓\" # コメント",
            "k = \"unterminated # not a comment",
            "k = 'unterminated",
            "k = \"\"\"multi # still string",
            "k = \"a # b\"",
            "[table]   ",
            "[[arr]] # c  ",
            "k = 1   ",
            "k = { a = \"#\", b = [1, 2] }  # x",
            "\"quoted key\" = true",
            "[unclosed",
            "  = ",
            "é=ü",
        ] {
            let text: String = colored(line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(text, line);
        }
    }
}
