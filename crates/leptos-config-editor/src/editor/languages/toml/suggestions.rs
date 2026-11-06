//! Where suggestions apply and how accepted keys and values are written.

use super::*;

pub(super) fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let lines: Vec<&str> = source.lines().collect();
    let (line_index, line_start) = line_at(source, &lines, cursor);
    let line = lines.get(line_index).copied().unwrap_or("");
    let caret_in_line = cursor.saturating_sub(line_start).min(line.len());
    let before_caret = &line[..caret_in_line];
    let (code_before, _) = split_toml_comment(before_caret);
    let mut path: Vec<SchemaPathSegment> = table_path_at(&lines, line_index);
    let (code_line, _) = split_toml_comment(line);

    // Inside a table header's brackets (`[web.lets|]`): complete that segment
    // with the tables under the segments before it.
    let header_text = code_before.trim_start();
    if header_text.starts_with('[') && !header_text.trim_start_matches('[').contains(']') {
        let inner = header_text.trim_start_matches('[');
        let (parents, typed) = inner.rsplit_once('.').unwrap_or(("", inner));
        let parents: Vec<&str> = parents.split('.').map(str::trim).filter(|part| !part.is_empty()).collect();
        let arrays: HashSet<String> = lines[..line_index]
            .iter()
            .filter_map(|line| parse_header(split_toml_comment(line).0))
            .filter(|(_, array)| *array)
            .map(|(segments, _)| segments.join("."))
            .collect();
        let mut header_path = Vec::new();
        for (depth, part) in parents.iter().enumerate() {
            header_path.push(SchemaPathSegment::Key(unquote_key(part)));
            if arrays.contains(&parents[..=depth].join(".")) {
                header_path.push(SchemaPathSegment::ArrayItem(0));
            }
        }
        let typed = typed.trim_start();
        let word_end = source[cursor..]
            .find(|character: char| matches!(character, '.' | ']' | '\n') || character.is_whitespace())
            .map_or(source.len(), |offset| cursor + offset);
        return CompletionContext {
            path: header_path,
            typed: typed.to_string(),
            replace_start: utf16(&source[..cursor - typed.len()]),
            replace_end: utf16(&source[..word_end]),
            position: EntryPosition::Name,
            // `[[…]]` names an array of tables; `[…]` a table.
            is_array_item: header_text.starts_with("[["),
            explicit_entry_start: true,
            ..Default::default()
        };
    }
    // Past a complete table header, the context is that table (hover
    // documents it); nothing is suggested there.
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
            // After a value with no comma yet, the comma goes right after it.
            CaretEntry::Key { start } => CompletionContext {
                typed: code_before[start..].trim_end().to_string(),
                replace_start: utf16(&source[..at(caret.separator_at.unwrap_or(start))]),
                replace_end: utf16(&source[..cursor]),
                entry_prefix: if caret.separator_at.is_some() { ", ".into() } else { String::new() },
                explicit_entry_start: caret.separator_at.is_some(),
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
pub(super) fn line_at(source: &str, lines: &[&str], byte: usize) -> (usize, usize) {
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
pub(super) fn value_token_len(text: &str) -> usize {
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
pub(super) fn existing_table_keys(
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
pub(super) fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    context: &InsertContext<'_>,
) -> Option<(String, usize)> {
    let toml_key = toml_key_for_completion(key)?;
    if context.position == EntryPosition::Name {
        // `[[…]]` names an array of tables; `[…]` a table, or a parent on the
        // way to a deeper header.
        let array_of_tables = array_branch(property, root)
            .and_then(|array| array.get("items"))
            .is_some_and(|items| object_branch(items, root).is_some());
        let table = if context.array_item {
            array_of_tables
        } else {
            object_branch(property, root).is_some() && array_branch(property, root).is_none()
        };
        return table.then(|| {
            let length = toml_key.encode_utf16().count();
            (toml_key, length)
        });
    }
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
pub(super) fn table_header(path: &[SchemaPathSegment], key: &str) -> Option<String> {
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
pub(super) fn required_properties(branch: &Value) -> Vec<(&str, &Value)> {
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
pub(super) fn toml_text(value: &Value) -> String {
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

pub(super) fn toml_inline_table(value: &Value) -> String {
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
pub(super) fn toml_key_for_completion(key: &str) -> Option<String> {
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
pub(super) fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
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
pub(super) fn dotted_hover_path(
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
pub(super) fn byte_at_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}

/// Where hover reads the context for the key at `column`: just past its `=`,
/// so a key inside an inline table resolves under that table.
pub(super) fn hover_context_offset(line: &str, column: usize) -> usize {
    let (code, _) = split_toml_comment(line);
    inline_key_at(code, b'=', byte_at_column(line, column))
        .map_or(line.len(), |key| key.after_separator)
}
