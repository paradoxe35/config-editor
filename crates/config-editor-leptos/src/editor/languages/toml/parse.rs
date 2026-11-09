//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// Datetimes become strings. Parsing as a `Table`, not a bare `Value`,
/// reports a duplicate key at its second occurrence rather than at line end.
pub(super) fn toml_parse(source: &str) -> Result<Value, ParseError> {
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
    Ok(Value::Object(
        document
            .iter()
            .map(|(key, value)| (key.clone(), toml_to_json(value)))
            .collect(),
    ))
}

/// A TOML value in the shared data model: dates and times are their TOML
/// text (`1979-05-27`), not serde's private wrapper object; `nan` and
/// infinities are held as `codec::non_finite` holds them.
fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => super::super::super::codec::non_finite::string_value(text.clone()),
        toml::Value::Integer(number) => Value::from(*number),
        toml::Value::Float(number) => super::super::super::codec::non_finite::float_value(*number),
        toml::Value::Boolean(flag) => Value::Bool(*flag),
        toml::Value::Datetime(datetime) => Value::String(datetime.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .iter()
                .map(|(key, value)| (key.clone(), toml_to_json(value)))
                .collect(),
        ),
    }
}

pub(super) fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (
        line,
        column,
        column + source[start..end.min(source.len())].chars().count(),
    )
}

/// Splits a line into code and trailing comment. A `#` starts a comment only
/// at the line start or after whitespace; `#` inside strings stays code.
/// Where `delimiter` closes a string in `text` (not escaped, for `"""`).
pub(super) fn closing_delimiter(text: &str, delimiter: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = text[from..].find(delimiter).map(|at| from + at) {
        let backslashes = text[..at].chars().rev().take_while(|character| *character == '\\').count();
        if delimiter.starts_with('\'') || backslashes % 2 == 0 {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

pub(super) fn split_toml_comment(line: &str) -> (&str, Option<&str>) {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    // The end of a triple-quoted string closed on this line.
    let mut skip_until = 0;
    for (index, character) in line.char_indices() {
        if index < skip_until {
            continue;
        }
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
                let delimiter = &line[index..index + 3];
                let Some(close) = closing_delimiter(&line[index + 3..], delimiter) else {
                    return (line, None);
                };
                // Up to two more quotes end the string's own text.
                let end = index + 3 + close + 3;
                let extra = line[end..]
                    .chars()
                    .take_while(|next| *next == character)
                    .count()
                    .min(2);
                skip_until = end + extra;
                continue;
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
pub(super) fn parse_header(line: &str) -> Option<(Vec<String>, bool)> {
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
pub(super) fn split_key_path(text: &str) -> Option<Vec<String>> {
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
pub(super) fn unquote_key(segment: &str) -> String {
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
pub(super) fn key_assignment(line: &str) -> Option<(String, usize, usize)> {
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
pub(super) fn assignment_equals(line: &str) -> Option<usize> {
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
pub(super) fn value_span_in_line(line: &str) -> Option<(usize, usize)> {
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
pub(super) fn table_path_at(lines: &[&str], line_index: usize) -> Vec<SchemaPathSegment> {
    table_paths(&lines[..line_index.min(lines.len())])
        .pop()
        .unwrap_or_default()
}

/// The table path in effect after each line, in one pass: `[[a.b]]` adds the
/// entry's index, and a later `[a.b.c]` belongs to the latest `a.b` entry.
/// Entry `i` is the path for line `i + 1`; a leading empty path is for line 0.
pub(super) fn table_paths(lines: &[&str]) -> Vec<Vec<SchemaPathSegment>> {
    let mut paths = Vec::with_capacity(lines.len() + 1);
    let mut tables = Tables::default();
    paths.push(tables.path.clone());
    for line in lines {
        let (code, _) = split_toml_comment(line);
        if let Some((segments, is_array)) = parse_header(code) {
            tables.enter(&segments, is_array);
        }
        paths.push(tables.path.clone());
    }
    paths
}

/// The table the lines read so far are in, header by header.
#[derive(Default)]
pub(super) struct Tables {
    pub(super) path: Vec<SchemaPathSegment>,
    /// How many items each array of tables has had, by its path with the
    /// entries above it (`a/1/b` → 2): an array in a later entry of its
    /// parent starts again at 0.
    arrays: HashMap<Vec<SchemaPathSegment>, usize>,
}

impl Tables {
    /// After the header `[segments]` (`[[segments]]` with `is_array`).
    pub(super) fn enter(&mut self, segments: &[String], is_array: bool) {
        let (own, parents) = match segments.split_last() {
            Some((own, parents)) if is_array => (Some(own), parents),
            _ => (None, segments),
        };
        let mut path = self.resolve(parents);
        if let Some(own) = own {
            path.push(SchemaPathSegment::Key(own.clone()));
            let count = self.arrays.entry(path.clone()).or_insert(0);
            path.push(SchemaPathSegment::ArrayItem(*count));
            *count += 1;
        }
        self.path = path;
    }

    /// The path of the table `segments` name now: each array of tables
    /// among them at its latest entry.
    pub(super) fn resolve<S: AsRef<str>>(&self, segments: &[S]) -> Vec<SchemaPathSegment> {
        let mut path = Vec::with_capacity(segments.len() * 2);
        for segment in segments {
            path.push(SchemaPathSegment::Key(segment.as_ref().to_string()));
            if let Some(count) = self.arrays.get(&path) {
                path.push(SchemaPathSegment::ArrayItem(count.saturating_sub(1)));
            }
        }
        path
    }
}
/// Array indices render as numbers, matching the validator's JSON-pointer segments.
pub(super) fn path_strings(path: &[SchemaPathSegment]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            SchemaPathSegment::Key(key) => key.clone(),
            SchemaPathSegment::ArrayItem(index) => index.to_string(),
        })
        .collect()
}

/// One entry inside an inline table, with byte offsets relative to the line
/// and its full schema path (outer table + inline key chain).
pub(super) struct InlineEntry {
    pub(super) key: (usize, usize),
    pub(super) value: (usize, usize),
    pub(super) path: Vec<String>,
}

/// Collects the entries of a single-line inline table starting just after its
/// `{`. Nested inline tables recurse, so `a = { b = { c = 1 } }` yields `a`,
/// `a.b`, and `a.b.c`. `offset` is where `text` starts inside the line.
pub(super) fn inline_entries(text: &str, offset: usize, base: &[String], out: &mut Vec<InlineEntry>) {
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
pub(super) fn inline_equals(text: &str, from: usize) -> Option<usize> {
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
pub(super) fn matching_brace(text: &str, open: usize) -> Option<usize> {
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
pub(super) fn inline_item_end(text: &str, from: usize) -> usize {
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

/// Every entry of the inline table that is the value on `code` (nested
/// ones too), `base` being the outer assignment's path; none when the
/// value is no inline table.
fn inline_entries_on(code: &str, base: &[String]) -> Vec<InlineEntry> {
    let mut entries = Vec::new();
    let Some((value_start, _)) = value_span_in_line(code) else {
        return entries;
    };
    if !code[value_start..].starts_with('{') {
        return entries;
    }
    let inner_start = value_start + 1;
    let inner_end = matching_brace(code, value_start)
        .unwrap_or(code.len())
        .max(inner_start);
    inline_entries(&code[inner_start..inner_end], inner_start, base, &mut entries);
    entries
}

/// The paths a batch of lookups wants, indexed: where each is, which are
/// below a path, which tables hold one.
struct Wanted {
    count: usize,
    /// Each path's positions in the list asked for.
    exact: HashMap<Vec<String>, Vec<usize>>,
    /// Every path some wanted one is strictly below.
    above: std::collections::HashSet<Vec<String>>,
}

impl Wanted {
    fn new(paths: &[String]) -> Self {
        let mut exact: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        let mut above = std::collections::HashSet::new();
        for (at, path) in paths.iter().enumerate() {
            let segments = pointer_segments(path);
            for end in 0..segments.len() {
                above.insert(segments[..end].to_vec());
            }
            exact.entry(segments).or_default().push(at);
        }
        Self { count: paths.len(), exact, above }
    }

    /// Whether a path at or below `path` is wanted.
    fn at_or_below(&self, path: &[String]) -> bool {
        self.above.contains(path) || self.exact.contains_key(path)
    }
}

/// Spans found for a batch, each kept from the first line it is found on.
struct Found {
    spans: Vec<Option<(usize, usize, usize)>>,
    missing: usize,
}

impl Found {
    fn new(count: usize) -> Self {
        Self { spans: vec![None; count], missing: count }
    }

    fn set(&mut self, at: &[usize], span: impl Fn() -> Option<(usize, usize, usize)>) {
        for at in at {
            if self.spans[*at].is_none() {
                self.spans[*at] = span();
                if self.spans[*at].is_some() {
                    self.missing -= 1;
                }
            }
        }
    }
}

pub(super) fn toml_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    toml_key_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`toml_key_span`] for each of `paths`, reading the document once: each
/// line's key looked up among the paths wanted, and the reading stops once
/// each is found.
pub(super) fn toml_key_spans(source: &str, paths: &[String]) -> Vec<Option<(usize, usize, usize)>> {
    let wanted = Wanted::new(paths);
    let mut found = Found::new(wanted.count);
    let lines: Vec<&str> = source.lines().collect();
    let tables = table_paths(&lines);
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        if found.missing == 0 {
            break;
        }
        let line_offset = offset;
        offset += line.len() + 1;
        let (code, _) = split_toml_comment(line);
        if let Some((key, start, end)) = key_assignment(code) {
            let table = path_strings(&tables[index]);
            let mut full = table.clone();
            full.extend(split_key_path(&key).unwrap_or_else(|| vec![key.clone()]));
            if let Some(at) = wanted.exact.get(&full) {
                // A dotted key (`websocket.bogus`) names its last segment.
                let start = start + last_segment_start(&code[start..end]);
                found.set(at, || Some(span_between(source, line_offset + start, line_offset + end)));
            }
            // A dotted key below the path (`bogus.deep` for `/limits/bogus`)
            // names it with one of its segments.
            for length in table.len() + 1..full.len() {
                if let Some(at) = wanted.exact.get(&full[..length]) {
                    found.set(at, || {
                        let (from, to) = segment_range(&code[start..end], length - table.len() - 1)?;
                        Some(span_between(source, line_offset + start + from, line_offset + start + to))
                    });
                }
            }
            // `key = { nested = … }` keeps every nested key on this line.
            if wanted.above.contains(&full) {
                for entry in inline_entries_on(code, &full) {
                    if let Some(at) = wanted.exact.get(&entry.path) {
                        found.set(at, || Some(span_between(source, line_offset + entry.key.0, line_offset + entry.key.1)));
                    }
                }
            }
        }
        if let Some((segments, is_array)) = parse_header(code)
            && !segments.is_empty()
        {
            let header_start = line.find('[').unwrap_or(0);
            let header = || Some(span_between(source, line_offset + header_start, line_offset + line.len()));
            let full = &tables[index + 1];
            // A `[table]` header names the table itself.
            if let Some(at) = wanted.exact.get(&path_strings(full)) {
                found.set(at, header);
            }
            // And what it is below: the array of an `[[array]]` header, by
            // the header; a table it opens implicitly (`a` and `a.b` of
            // `[a.b.c]`), by its segment.
            let inner_start = header_start + if is_array { 2 } else { 1 };
            let closing = if is_array { "]]" } else { "]" };
            let inner = code.trim_end().strip_suffix(closing).and_then(|code| code.get(inner_start..)).unwrap_or("");
            let mut keys = 0;
            for cut in 1..full.len() {
                let SchemaPathSegment::Key(_) = full[cut - 1] else {
                    continue;
                };
                keys += 1;
                let Some(at) = wanted.exact.get(&path_strings(&full[..cut])) else {
                    continue;
                };
                if is_array && cut + 1 == full.len() {
                    found.set(at, header);
                } else {
                    found.set(at, || {
                        let (from, to) = segment_range(inner, keys - 1)?;
                        Some(span_between(source, line_offset + inner_start + from, line_offset + inner_start + to))
                    });
                }
            }
        }
    }
    found.spans
}

pub(super) fn toml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    toml_value_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`toml_value_span`] for each of `paths`, reading the document once: a
/// line is looked at only while it is in a table some path wanted lies
/// under, and the reading stops once each is found.
pub(super) fn toml_value_spans(source: &str, paths: &[String]) -> Vec<Option<(usize, usize, usize)>> {
    let wanted = Wanted::new(paths);
    let mut found = Found::new(wanted.count);
    let mut tables = Tables::default();
    let mut table: Vec<String> = Vec::new();
    let mut offset = 0;
    for line in source.lines() {
        if found.missing == 0 {
            break;
        }
        let line_offset = offset;
        offset += line.len() + 1;
        if line.trim_start().starts_with('[') {
            let (code, _) = split_toml_comment(line);
            if let Some((segments, is_array)) = parse_header(code) {
                tables.enter(&segments, is_array);
                table = path_strings(&tables.path);
                continue;
            }
        }
        if !wanted.at_or_below(&table) {
            continue;
        }
        let (code, _) = split_toml_comment(line);
        let Some((key, _, _)) = key_assignment(code) else {
            continue;
        };
        let mut full = table.clone();
        full.extend(split_key_path(&key).unwrap_or_else(|| vec![key.clone()]));
        if let Some(at) = wanted.exact.get(&full) {
            found.set(at, || {
                let (start, end) = value_span_in_line(code)?;
                Some(value_span_at(source, code, line_offset, start, end))
            });
        }
        // A value nested in an inline table there.
        if wanted.above.contains(&full) {
            for entry in inline_entries_on(code, &full) {
                let Some(at) = wanted.exact.get(&entry.path) else {
                    continue;
                };
                let (start, end) = entry.value;
                found.set(at, || {
                    if start >= end && !code[start..].trim_start().starts_with(['{', '[']) {
                        return None;
                    }
                    Some(value_span_at(source, code, line_offset, start, end))
                });
            }
        }
    }
    found.spans
}

/// The span of the value from `start` to `end` of a line's `code` (at
/// `offset`): a container is marked at its opening bracket.
fn value_span_at(source: &str, code: &str, offset: usize, start: usize, end: usize) -> (usize, usize, usize) {
    if code[start..].trim_start().starts_with(['{', '[']) {
        let (line_number, column) = line_column(source, offset + start);
        return (line_number, column, column + 1);
    }
    span_between(source, offset + start, offset + end)
}

pub(super) fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}

/// Where the last segment of a dotted key starts (after its last `.` outside
/// quotes, and any spaces), in bytes; 0 for a plain key.
/// The byte range of segment `wanted` (from 0) of a dotted key, trimmed.
pub(super) fn segment_range(key: &str, wanted: usize) -> Option<(usize, usize)> {
    let mut quote: Option<char> = None;
    let mut segment = 0;
    let mut segment_start = 0;
    let trimmed = |from: usize, to: usize| {
        let text = &key[from..to];
        let start = from + text.len() - text.trim_start().len();
        (start, start + text.trim().len())
    };
    for (index, character) in key.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == '.' => {
                if segment == wanted {
                    return Some(trimmed(segment_start, index));
                }
                segment += 1;
                segment_start = index + 1;
            }
            None => {}
        }
    }
    (segment == wanted).then(|| trimmed(segment_start, key.len()))
}

fn last_segment_start(key: &str) -> usize {
    let mut quote: Option<char> = None;
    let mut last_dot = None;
    for (index, character) in key.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == '.' => last_dot = Some(index),
            None => {}
        }
    }
    last_dot.map_or(0, |dot| {
        let rest = &key[dot + 1..];
        dot + 1 + rest.len() - rest.trim_start().len()
    })
}
