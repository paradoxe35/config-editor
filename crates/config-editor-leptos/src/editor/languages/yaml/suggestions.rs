//! Where suggestions apply and how accepted keys and values are written.

use super::*;

/// The default a suggestion writes for `property`: one YAML reads back as
/// it is, else none (the caret stays where the value goes).
fn written_default<'a>(property: &'a Value, root: &'a Value) -> Option<&'a Value> {
    kept_default(&YamlBehavior, property, root)
}

/// The `const` a suggestion writes for `property`, as [`written_default`].
fn written_const<'a>(property: &'a Value, root: &'a Value) -> Option<&'a Value> {
    kept_const(&YamlBehavior, property, root)
}

/// A key as a suggestion, a flow mapping and a merge's written-out keys
/// write it: plain where every reader takes it as that text, else
/// quoted (`"on"`, `"a b"`, `""`).
pub(super) fn yaml_key(key: &str) -> String {
    if is_plain_identifier(key) { key.to_string() } else { yaml_quoted(key) }
}

use super::super::super::codec::non_finite;

pub(super) fn yaml_value(value: &Value) -> String {
    if let Some(name) = non_finite::name(value) {
        return format!(".{name}").replace(".-", "-.");
    }
    match value {
        Value::String(value) if is_plain_identifier(non_finite::written(value)) => non_finite::written(value).to_string(),
        Value::String(value) => yaml_quoted(non_finite::written(value)),
        // Flow style, as people write it: `{a: 1, b: x}`, `[a, b]`.
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(yaml_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(key, child)| {
                    format!("{}: {}", yaml_key(key), yaml_value(child))
                })
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        _ => value.to_string(),
    }
}

/// `text` in double quotes, escaped so every YAML reader takes it back:
/// JSON's escapes, the characters YAML 1.1 reads as line breaks (U+0085,
/// U+2028, U+2029) as YAML writes them (`\N`, `\L`, `\P`), and the
/// characters YAML does not allow raw (DEL, C1 controls, U+FFFE, U+FFFF) as
/// `\x7F` or `\uFFFE`.
pub(super) fn yaml_quoted(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap_or_default();
    if !quoted.chars().any(needs_yaml_escape) {
        return quoted;
    }
    let mut out = String::with_capacity(quoted.len() + 8);
    for character in quoted.chars() {
        match character {
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            '\u{7f}'..='\u{9f}' => out.push_str(&format!("\\x{:02X}", u32::from(character))),
            '\u{fffe}' | '\u{ffff}' => out.push_str(&format!("\\u{:04X}", u32::from(character))),
            other => out.push(other),
        }
    }
    out
}

/// Whether YAML writes `character` only escaped, in double quotes: a line
/// break of YAML 1.1 or a character YAML does not allow raw (beyond the
/// C0 controls JSON's escapes already cover).
pub(super) fn needs_yaml_escape(character: char) -> bool {
    matches!(character, '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}' | '\u{fffe}' | '\u{ffff}')
}

/// Enum-style words (`udp`, `least_connections`) read as plain YAML scalars, the way
/// the YAML language server writes them. Anything that YAML could read as a number,
/// boolean, or null, or that contains punctuation, stays quoted.
pub(super) fn is_plain_identifier(text: &str) -> bool {
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
        && super::plain::can_be_plain(text)
}

/// `children` as the lines of a block mapping at `child_indent`, a nested
/// object a block of its own below its key; `cursor_offset` is set to the
/// UTF-16 offset of the value the caret goes to, `output` starting at
/// `base_offset` of the insert.
pub(super) fn write_children(
    children: &[RequiredChild<'_>],
    child_indent: &str,
    unit: &str,
    base_offset: usize,
    output: &mut String,
    cursor_offset: &mut Option<usize>,
) {
    for child in children {
        output.push_str(child_indent);
        output.push_str(&yaml_key(child.key));
        output.push(':');
        match &child.fill {
            Fill::Value(value) => {
                output.push(' ');
                output.push_str(&yaml_value(value));
            }
            Fill::Children(nested) => {
                output.push('\n');
                let nested_indent = format!("{child_indent}{unit}");
                write_children(nested, &nested_indent, unit, base_offset, output, cursor_offset);
                continue;
            }
            Fill::Caret { .. } => {
                output.push(' ');
                *cursor_offset = Some(base_offset + output.encode_utf16().count());
            }
            // Before the value, inside an empty list.
            Fill::CaretAt(value) => {
                output.push(' ');
                let text = yaml_value(value);
                let inside = usize::from(text == "[]");
                *cursor_offset = Some(base_offset + output.encode_utf16().count() + inside);
                output.push_str(&text);
            }
            // An empty value is null: an empty object or list is written.
            Fill::Empty(Some(value @ (Value::Object(_) | Value::Array(_)))) => {
                output.push(' ');
                output.push_str(&yaml_value(value));
            }
            Fill::Empty(_) => {}
        }
        output.push('\n');
    }
}

/// `children` as a flow mapping (`{ cpu: 2, mem: | }`); `cursor` is set
/// to the byte offset in `insert` of the value the caret goes to.
fn write_flow(children: &[RequiredChild<'_>], insert: &mut String, cursor: &mut Option<usize>) {
    let flow = |value: &Value| match value {
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
        _ => yaml_value(value),
    };
    insert.push('{');
    for (index, child) in children.iter().enumerate() {
        insert.push_str(if index == 0 { " " } else { ", " });
        insert.push_str(&yaml_key(child.key));
        insert.push_str(": ");
        match &child.fill {
            Fill::Value(value) | Fill::Empty(Some(value @ (Value::Object(_) | Value::Array(_)))) => {
                insert.push_str(&flow(value));
            }
            Fill::Children(nested) => write_flow(nested, insert, cursor),
            Fill::Caret { .. } => *cursor = Some(insert.len()),
            // Before the value, inside an empty list.
            Fill::CaretAt(value) => {
                let text = flow(value);
                *cursor = Some(insert.len() + usize::from(text == "[]"));
                insert.push_str(&text);
            }
            Fill::Empty(_) => {}
        }
    }
    insert.push(' ');
    // No children: the caret between the braces.
    if children.is_empty() {
        cursor.get_or_insert(insert.len());
        insert.push(' ');
    }
    insert.push('}');
}

/// Insert text for a property inside a flow mapping (`key: { a: 1, | }`):
/// everything stays in flow style on one line, since a block mapping is
/// invalid there. Nested objects open `{ child: | }`, arrays open `[|]`.
pub(super) fn flow_property_insert(key: &str, property: &Value, root: &Value) -> Option<(String, usize)> {
    let yaml_key = yaml_key(key);
    let flow = |value: &Value| match value {
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
        _ => yaml_value(value),
    };
    let mut insert = format!("{yaml_key}: ");
    let mut cursor = None;
    if let Some(value) = written_const(property, root) {
        insert.push_str(&flow(value));
    } else if let Some(default) = written_default(property, root).filter(|value| {
        !value.as_array().is_some_and(Vec::is_empty)
            && !value.as_object().is_some_and(serde_json::Map::is_empty)
    }) {
        insert.push_str(&flow(default));
    } else if array_branch(property, root).is_some() {
        insert.push('[');
        cursor = Some(insert.len());
        insert.push(']');
    } else if object_branch(property, root).is_some() {
        write_flow(&required_children(&YamlBehavior, property, root), &mut insert, &mut cursor);
        // Past the children: the caret before the closing brace.
        cursor.get_or_insert(insert.len() - 1);
    }
    let cursor = insert[..cursor.unwrap_or(insert.len())]
        .encode_utf16()
        .count();
    Some((insert, cursor))
}

pub(super) fn property_completion_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    let yaml_key = yaml_key(key);
    let child_indent = format!("{base_indent}{unit}");
    let mut insert = format!("{yaml_key}:");
    if let Some(value) = written_const(property, root) {
        insert.push(' ');
        insert.push_str(&yaml_value(value));
    } else if array_branch(property, root).is_some() {
        let default = written_default(property, root);
        if default.is_some_and(|value| value.as_array().is_some_and(Vec::is_empty)) {
            // An empty list default stays one, the caret inside it: an item
            // left empty (`- `) reads as `[null]`.
            insert.push_str(" []");
            let cursor = insert.encode_utf16().count() - 1;
            return Some((insert, cursor));
        }
        if let Some(default) = default {
            insert.push(' ');
            insert.push_str(&yaml_value(default));
        } else {
            insert.push('\n');
            insert.push_str(&child_indent);
            insert.push_str("- ");
        }
    } else if let Some(branch) = object_branch(property, root) {
        let default = written_default(property, root);
        if let Some(default) =
            default.filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
        {
            insert.push(' ');
            insert.push_str(&yaml_value(default));
            let cursor = insert.encode_utf16().count();
            return Some((insert, cursor));
        }
        let mut cursor_offset = None;
        let children = required_children(&YamlBehavior, property, root);
        if !children.is_empty() {
            insert.push('\n');
            let base_offset = insert.encode_utf16().count();
            let mut written = String::new();
            write_children(&children, &child_indent, unit, base_offset, &mut written, &mut cursor_offset);
            insert.push_str(&written);
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
    } else if let Some(default) = written_default(property, root) {
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
pub(super) fn mapping_colon(line: &str) -> Option<usize> {
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
pub(super) fn mapping_colon_anywhere(line: &str) -> Option<usize> {
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
pub(super) fn value_token_len(text: &str) -> usize {
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

/// The keys and list items open above a line: each with its column, and
/// how many items each list column has had.
#[derive(Clone, Default)]
pub(super) struct Ancestors {
    ancestors: Vec<(usize, SchemaPathSegment)>,
    item_counts: std::collections::BTreeMap<usize, usize>,
}

impl Ancestors {
    /// Past `line`.
    fn walk(&mut self, line: &str) {
        let Ancestors {
            ancestors,
            item_counts,
        } = self;

        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return;
        }
        let line_indent = line.len() - trimmed.len();
        // Each leading `- ` opens an item at its own column, so `- - x: 1`
        // is an item of an item (a sequence of sequences).
        let mut mapping = trimmed;
        let mut mapping_indent = line_indent;
        item_counts.retain(|level, _| *level <= line_indent);
        while let Some(after_dash) = dash_item(mapping) {
            // A sequence may sit at its key's own indentation (`key:\n- a`):
            // a dash replaces an item at its column, not the key.
            while ancestors.last().is_some_and(|(level, segment)| {
                *level > mapping_indent
                    || *level == mapping_indent && matches!(segment, SchemaPathSegment::ArrayItem(_))
            }) {
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
            return;
        };
        // A key written `""` is the empty key; nothing before the colon is none.
        let written = without_node_properties(mapping[..colon].trim());
        if written.is_empty() {
            return;
        }
        let key = written.trim_matches(['\'', '"']);
        while ancestors
            .last()
            .is_some_and(|(level, _)| *level >= mapping_indent)
        {
            ancestors.pop();
        }
        if opens_block(&mapping[colon + 1..]) {
            ancestors.push((mapping_indent, SchemaPathSegment::Key(key.to_string())));
        }
    
    }
}

/// The lines of a text as `str::lines` gives them, and the walk of
/// [`Ancestors`] over them kept every few lines, read once while a batch
/// runs for the text (see `paths::paths_at`).
struct Walked {
    ranges: Vec<(usize, usize)>,
    kept: crate::editor::paths::Checkpoints<Ancestors>,
}

/// The [`Ancestors`] above the line starting at byte `line_start`: walked
/// over the lines above from the top, or while a batch runs for the text,
/// from the last state kept before it.
fn ancestors_before(source: &str, line_start: usize) -> Ancestors {
    if !crate::editor::paths::in_batch(source) {
        let mut walk = Ancestors::default();
        for line in source[..line_start].lines() {
            walk.walk(line);
        }
        return walk;
    }
    let walked = crate::editor::paths::batch_cached(source, || {
        crate::editor::paths::walked_from_top(source);
        let base = source.as_ptr() as usize;
        let ranges: Vec<(usize, usize)> = source
            .lines()
            .map(|line| (line.as_ptr() as usize - base, line.as_ptr() as usize - base + line.len()))
            .collect();
        let kept = crate::editor::paths::Checkpoints::walk(Ancestors::default(), ranges.len(), |walk, index| {
            let (start, end) = ranges[index];
            walk.walk(&source[start..end]);
        });
        Walked { ranges, kept }
    });
    // The lines above: the ones starting before the line's start.
    let above = walked.ranges.partition_point(|(start, _)| *start < line_start);
    walked.kept.before(above, |walk, index| {
        let (start, end) = walked.ranges[index];
        walk.walk(&source[start..end]);
    })
}

pub(super) fn completion_context(source: &str, cursor: usize) -> CompletionContext {
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

    let Ancestors {
        mut ancestors,
        item_counts,
    } = ancestors_before(source, current_line_start);
    let current_is_array_item = dash_item(prefix).is_some();
    while ancestors.last().is_some_and(|(level, segment)| {
        *level > indent
            || *level == indent
                && (!current_is_array_item || matches!(segment, SchemaPathSegment::ArrayItem(_)))
    }) {
        ancestors.pop();
    }
    // The mapping holding the caret's line: its key's column (none at the
    // top level), when it is a key's mapping.
    let parent_level = match ancestors.last() {
        Some((level, SchemaPathSegment::Key(_))) => Some(Some(*level)),
        Some(_) => None,
        None => Some(None),
    };
    let mut path: Vec<_> = ancestors.into_iter().map(|(_, key)| key).collect();
    let (line_content, content_offset) = {
        // The caret line's dashes: the first continues the list at its
        // column, any further one starts a new inner list (an item the caret
        // is on only once at or past its dash: `|- - 3` is the outer item).
        let mut content = prefix;
        let mut first = true;
        let caret_column = cursor - current_line_start;
        while let Some(after_dash) = dash_item(content) {
            let column = indent + (prefix.len() - content.len());
            let index = if first {
                item_counts.get(&column).copied().unwrap_or_default()
            } else {
                0
            };
            if first || column <= caret_column {
                path.push(SchemaPathSegment::ArrayItem(index));
            }
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
        .map(|colon| without_node_properties(line_content[..colon].trim()))
        .filter(|written| !written.is_empty())
        .map(|written| written.trim_matches(['\'', '"']).to_string());
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
    let flow = match statement_start_at(source, cursor) {
        Some(statement) if statement < current_line_start => {
            // The path above the statement comes from its own first line.
            let first_line = &source[statement..];
            let mut content = statement + (first_line.len() - first_line.trim_start_matches([' ', '\t']).len());
            if source[content..].starts_with("- ") {
                content += 2;
            }
            let base = completion_context(source, crate::editor::paths::units_at(source, content)).path;
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
        let value_end = caret_value_end(source, cursor, line_end);
        let typed_from = |start: usize| source[content_byte + start..cursor].trim_end().to_string();
        let context = |path, typed, start: usize, end: usize| CompletionContext {
            path,
            typed,
            replace_start: crate::editor::paths::units_at(source, content_byte + start),
            replace_end: crate::editor::paths::units_at(source, end),
            base_indent: base_indent.clone(),
            position: EntryPosition::Inline,
            ..Default::default()
        };
        return match caret.entry {
            CaretEntry::Key { start } => {
                let base = match caret.separator_at {
                    // After a value with no comma yet, the comma goes right after it.
                    Some(separator) => CompletionContext {
                        entry_prefix: ", ".into(),
                        explicit_entry_start: true,
                        ..context(path, typed_from(start), separator, cursor)
                    },
                    None => context(path, typed_from(start), start, cursor),
                };
                in_front_of_inline_entry(base, &source[cursor..line_end], b':')
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
    // A key typed in the indentation, between its parent's column and its
    // siblings' (`websocket:\n  path: /x\n ke|`): it is written at the
    // siblings' column, and an entry after it on the line goes to its own.
    if value_key.is_none()
        && !current_is_array_item
        && content_offset == 0
        && let Some(parent) = parent_level
        && let Some(column) = sibling_column(source, current_line_start, current_line_end, parent)
        && column != indent
        && parent.is_none_or(|parent| indent > parent)
    {
        let siblings = " ".repeat(column);
        let entry_after_space = !splits && rest.starts_with([' ', '\t']) && entry_follows(rest.trim_start(), b':');
        let gap = rest.len() - rest.trim_start().len();
        return CompletionContext {
            path,
            typed,
            replace_start: crate::editor::paths::units_at(source, current_line_start),
            replace_end: source[..if splits || entry_after_space { cursor + gap } else { replace_end_byte.min(source.len()) }]
                .encode_utf16()
                .count(),
            entry_prefix: siblings.clone(),
            entry_suffix: if splits || entry_after_space { format!("\n{siblings}") } else { String::new() },
            entry_after: (splits && gap == 0).then_some(word_after),
            base_indent: siblings,
            ..Default::default()
        };
    }
    // A blank document but for the key typed: its first entry may open a
    // root list's first item.
    let root_item = (value_key.is_none()
        && path.is_empty()
        && source[..typed_start_byte].trim().is_empty()
        && source[replace_end_byte.min(source.len())..].trim().is_empty())
    .then_some(("- ", "", "  "));
    CompletionContext {
        path,
        typed,
        replace_start: crate::editor::paths::units_at(source, typed_start_byte),
        replace_end: source[..replace_end_byte.min(source.len())]
            .encode_utf16()
            .count(),
        root_item,
        value_key,
        value_needs_space,
        is_array_item: current_is_array_item,
        entry_suffix: if splits { format!("\n{base_indent}") } else { String::new() },
        entry_after: splits.then_some(word_after),
        base_indent,
        ..Default::default()
    }
}

/// The column the entries of a mapping write their keys at, found from the
/// lines around the caret's (`start..end`) that sit deeper than the
/// mapping's key at `parent` (any line for the top level).
fn sibling_column(source: &str, start: usize, end: usize, parent: Option<usize>) -> Option<usize> {
    let indent_of = |line: &str| {
        let trimmed = line.trim_start_matches([' ', '\t']);
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then(|| line.len() - trimmed.len())
    };
    let inside = |column: usize| parent.is_none_or(|parent| column > parent);
    let before = source[..start.saturating_sub(1)]
        .lines()
        .rev()
        .filter_map(indent_of)
        .take_while(|column| inside(*column));
    let after = source[(end + 1).min(source.len())..]
        .lines()
        .filter_map(indent_of)
        .take_while(|column| inside(*column));
    before.chain(after).min()
}

pub(super) fn existing_mapping_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    // A half-typed key makes the document unparsable, so parse without the
    // caret's line; the other keys in this mapping still get filtered.
    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |newline| line_start + newline);
    if let Some(statement) = statement_start_at(source, cursor_byte)
        && let Some(keys) = inline_sibling_keys(&source[statement..], b':', cursor_byte - statement)
    {
        return Some(keys.into_iter().collect());
    }
    let sanitized = format!("{}{}", &source[..line_start], &source[line_end..]);
    let Ok(parser_config) = super::limits::parser_config(&sanitized) else {
        return Some(sibling_block_keys(source, line_start, line_end));
    };
    let parser_config = parser_config.duplicate_key_policy(DuplicateKeyPolicy::Error);
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
        without_node_properties(current_line[..colon].trim())
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
pub(super) fn sibling_block_keys(source: &str, line_start: usize, line_end: usize) -> HashSet<String> {
    let indent_of = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let caret_line = &source[line_start..line_end];
    let indent = indent_of(caret_line);
    let key_of = |line: &str| -> Option<String> {
        let body = line.trim_start();
        let body = body.strip_prefix("- ").unwrap_or(body);
        let colon = mapping_colon_anywhere(body)?;
        Some(without_node_properties(body[..colon].trim()).trim_matches(['\'', '"']).to_string())
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
pub(super) fn dash_item(text: &str) -> Option<&str> {
    if text == "-" {
        return Some("");
    }
    text.strip_prefix("- ")
        .or_else(|| text.strip_prefix("-\t"))
        .map(|rest| rest.trim_start_matches([' ', '\t']))
}
