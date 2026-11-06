//! Where suggestions apply and how accepted keys and values are written.

use super::*;

pub(super) fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let scan = analyze(source, cursor);
    let Some(snapshot) = scan.snapshot else {
        unreachable!("analyze always captures a snapshot");
    };
    let typed_start = snapshot.partial.map_or(cursor, |token| token.start);
    let mut replace_end = snapshot.partial.map_or(cursor, |token| match token.kind {
        // An unclosed string has no closing quote to replace; stop at the caret.
        Kind::Str { closed: false } => cursor,
        _ => token.end.max(cursor),
    });
    // Completing a key whose colon is already typed (`"key"|: value`): swallow
    // the existing colon and its spaces, since the key insert supplies its own
    // `": "`.
    let completing_key = matches!(
        snapshot.frame,
        Some(
            Frame::Object {
                mode: Mode::Key,
                ..
            } | Frame::Object {
                mode: Mode::Colon(_),
                ..
            }
        )
    );
    if completing_key {
        let rest = &source[replace_end..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            let spaces = after_colon.len() - after_colon.trim_start_matches([' ', '\t']).len();
            replace_end += rest.len() - trimmed.len() + 1 + spaces;
        }
    }
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let base_indent: String = source[line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let path: Vec<SchemaPathSegment> = snapshot
        .path
        .iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect();
    // Past a complete value (object awaiting `,` or `}`, array not awaiting an
    // item), a new entry needs a separating comma.
    let needs_separator = matches!(
        snapshot.frame,
        Some(
            Frame::Object {
                mode: Mode::After,
                ..
            } | Frame::Array {
                awaiting: false,
                ..
            }
        )
    );
    // After the root value closes nothing can follow, so nothing is offered.
    if snapshot.frame.is_none() && !source[..typed_start].trim().is_empty() {
        return CompletionContext {
            no_properties: true,
            ..Default::default()
        };
    }
    // A typed opening quote is the start of a key, so the key list opens now.
    let explicit_entry_start = snapshot.partial.is_some() && source[typed_start..].starts_with('"');
    let (path, value_key, is_array_item) = match snapshot.frame {
        None => (path, None, false),
        Some(Frame::Object {
            mode: Mode::Value(key),
            ..
        }) => (path, Some(key), false),
        Some(Frame::Object { .. }) => (path, None, false),
        Some(Frame::Array { index, .. }) => {
            let mut path = path;
            path.push(SchemaPathSegment::ArrayItem(index));
            (path, None, true)
        }
    };
    // `"web": {"le|}`: the brace opened on this line is still open, so a
    // multi-line insert moves onto its own line inside it.
    let block = value_key.is_none().then(|| {
        inline_block(
            source,
            typed_start,
            replace_end,
            unclosed_brackets(&source[line_start..typed_start]),
            &indent_unit(source),
            |rest| rest.starts_with(['}', ']']),
            ",",
        )
    });
    // The missing comma goes right after the previous value, not before the
    // new key: `"a": 1|` + Enter + key gives `"a": 1,\n  "b": …`.
    let previous_end = source[..typed_start].trim_end().len();
    let (replace_start, entry_prefix) = if needs_separator {
        let gap = &source[previous_end..typed_start];
        let prefix = if gap.contains('\n') {
            format!(",{gap}")
        } else {
            ", ".to_string()
        };
        (previous_end, prefix)
    } else {
        (typed_start, String::new())
    };
    // The caret right after an existing key's opening quote (`"|tcp"`) types
    // in front of it: a new entry goes before the key, which is not offered.
    // Inside a key's quotes with more of it after the caret (`"ud|tcp"`):
    // the completion layer decides between renaming the key and a new entry
    // in front of it. The replaced range is up to the caret, so a new entry
    // goes before the key, which gets its opening quote back.
    let key_after = snapshot.partial.filter(|token| {
        completing_key
            && matches!(token.kind, Kind::Str { closed: true })
            && cursor > token.start
            && token.end > cursor + 1
    });
    let (typed_start, replace_start, replace_end) = match key_after {
        Some(token) => (token.start, token.start, cursor),
        None => (typed_start, replace_start, replace_end),
    };
    let entry_after = key_after.map(|token| source[cursor..token.end - 1].to_string());
    let following = &source[replace_end..];
    // An entry that starts its line (`  hea|"tcp"`) puts what follows on the
    // next one, as the other languages do.
    let entry_line = source[..replace_start].rfind('\n').map_or(0, |at| at + 1);
    let starts_line = source[entry_line..replace_start].trim().is_empty();
    let entry_suffix = match key_after.is_some() || following.trim_start().starts_with('"') {
        // A following key on its own line keeps it; no trailing space.
        true if following[..following.len() - following.trim_start().len()].contains('\n') => {
            ",".to_string()
        }
        true if starts_line => format!(",\n{base_indent}"),
        true => ", ".to_string(),
        false => String::new(),
    };
    // The key typed in front of gets back the quote the insert replaced.
    let entry_suffix = if key_after.is_some() { format!("{entry_suffix}\"") } else { entry_suffix };
    // An empty document has no root object yet: the first key opens it.
    let empty_document = source.trim().is_empty();
    CompletionContext {
        entry_wrap: if empty_document {
            Some(("{\n  ", "\n}\n"))
        } else {
            is_array_item.then_some(("{", "}"))
        },
        block: block.flatten(),
        path,
        typed: source[typed_start..cursor].to_string(),
        replace_start: utf16(&source[..replace_start]),
        replace_end: utf16(&source[..replace_end]),
        value_key,
        value_needs_space: snapshot.partial.is_none() && source[..cursor].ends_with(':'),
        is_array_item,
        base_indent,
        entry_prefix,
        explicit_entry_start,
        entry_suffix,
        entry_after,
        ..Default::default()
    }
}

/// Keys already present in the object holding the caret. The partly typed key
/// is not counted, so it can still be completed.
pub(super) fn existing_keys(source: &str, cursor_byte: usize) -> Option<HashSet<String>> {
    let scan = analyze(source, cursor_byte);
    let Some(Frame::Object { id, .. }) = scan.snapshot?.frame else {
        return None;
    };
    Some(
        scan.keys
            .iter()
            .filter(|(owner, _, _)| *owner == id)
            .map(|(_, key, _)| key.clone())
            .collect(),
    )
}

/// The required properties of an object schema that have a schema of their
/// own, in the order the schema lists them.
pub(super) fn required_children(branch: &Value) -> Vec<(String, &Value)> {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|name| properties.get(name).map(|child| (name.to_string(), child)))
        .collect()
}

/// Insert text for a property suggestion, with the caret placed where the user
/// will type the value. Objects with required children expand into a block.
pub(super) fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    let mut insert = format!("{}: ", serde_json::to_string(key).ok()?);
    let mut caret = None;
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&json_text(value));
    } else if let Some(array) = array_branch(property, root) {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => insert.push_str(&json_text(default)),
            // A list of strings starts with a quoted item, so typing a value
            // keeps the document valid.
            None if array
                .get("items")
                .is_some_and(|items| placeholder(items, root).is_string()) =>
            {
                insert.push_str("[\"\"]");
                caret = Some(insert.len() - 2);
            }
            None => {
                insert.push_str("[]");
                caret = Some(insert.len() - 1);
            }
        }
    } else if let Some(branch) = object_branch(property, root) {
        match schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(Map::is_empty))
        {
            Some(default) => insert.push_str(&json_text(default)),
            None => {
                let children = required_children(branch);
                if children.is_empty() {
                    insert.push_str("{}");
                    caret = Some(insert.len() - 1);
                } else {
                    let child_indent = format!("{base_indent}{unit}");
                    insert.push_str("{\n");
                    for (index, (name, child)) in children.iter().enumerate() {
                        if index > 0 {
                            insert.push_str(",\n");
                        }
                        insert.push_str(&child_indent);
                        insert.push_str(&serde_json::to_string(name).ok()?);
                        insert.push_str(": ");
                        // The first child takes the caret; the others get a valid
                        // placeholder so the document stays parseable.
                        let takes_caret = caret.is_none();
                        if takes_caret {
                            caret = Some(insert.len());
                        }
                        match schema_default(child, root) {
                            Some(default) => insert.push_str(&json_text(default)),
                            None if !takes_caret => insert.push_str(&json_text(&placeholder(child, root))),
                            // The caret's own child: a string starts quoted, caret inside.
                            None if placeholder(child, root).is_string() => {
                                insert.push_str("\"\"");
                                caret = caret.map(|position| position + 1);
                            }
                            None => {}
                        }
                    }
                    insert.push('\n');
                    insert.push_str(base_indent);
                    insert.push('}');
                }
            }
        }
    } else if let Some(default) = schema_default(property, root) {
        insert.push_str(&json_text(default));
    } else if placeholder(property, root).is_string() {
        // A string value starts quoted, with the caret inside.
        insert.push_str("\"\"");
        caret = Some(insert.len() - 1);
    }
    let caret = caret.unwrap_or(insert.len());
    let caret_units = utf16(&insert[..caret]);
    Some((insert, caret_units))
}

/// Key under `column` (a character index) on a single line, if the string there
/// is followed by a colon. Returns the token, the unescaped key, and its start
/// column.
pub(super) fn key_at_column(line: &str, column: usize) -> Option<(Token, String, usize)> {
    tokenize(line).into_iter().find_map(|token| {
        if !matches!(token.kind, Kind::Str { closed: true }) {
            return None;
        }
        let start_column = line[..token.start].chars().count();
        let end_column = start_column + line[token.start..token.end].chars().count();
        let followed_by_colon = line[token.end..].trim_start().starts_with(':');
        if !(start_column..end_column).contains(&column) || !followed_by_colon {
            return None;
        }
        let key = decode_string(&line[token.start..token.end], true);
        Some((token, key, start_column))
    })
}
