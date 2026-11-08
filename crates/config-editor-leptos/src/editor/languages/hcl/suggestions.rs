//! Where suggestions apply and how accepted keys and values are written.

use super::*;

/// A key typed in a block written on one line (`websocket { ho|path = "/x" }`,
/// `websocket { path = "/x" ho| }`): a block on one line holds a single
/// attribute, so a new one opens it over lines, its attribute on a line of
/// its own. A key typed inside the existing name renames it in place.
fn in_one_line_block(
    source: &str,
    line_start: usize,
    line: &str,
    caret: usize,
    path: &[SchemaPathSegment],
    base_indent: &str,
) -> Option<CompletionContext> {
    let before = &line[..caret];
    // The unmatched `{` before the caret, outside strings.
    let (mut depth, mut quote, mut open) = (0usize, false, None);
    let mut opens = Vec::new();
    for (index, character) in before.char_indices() {
        match character {
            '"' => quote = !quote,
            '{' if !quote => {
                opens.push(index);
                depth += 1;
            }
            '}' if !quote => {
                opens.pop();
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    if quote || depth != 1 {
        return None;
    }
    if let Some(&at) = opens.last() {
        open = Some(at);
    }
    let open = open?;
    // A block's body, not an object expression (`= {`).
    let head = line[..open].trim_end();
    if head.is_empty() || head.ends_with(['=', ':', '(', '[', ',', '{']) {
        return None;
    }
    let close = open + matching_close(&line[open..], '}')?;
    if close < caret {
        return None;
    }
    // An attribute's name being edited (`{ pa|th = "/x" }`): renamed in
    // place, as anywhere else.
    if line[caret..].trim_start().starts_with('=') {
        return None;
    }
    let typed_len = before.len() - before.trim_end_matches(|character: char| character.is_alphanumeric() || matches!(character, '_' | '-')).len();
    let typed_start = caret - typed_len;
    let first = line[open + 1..typed_start].trim();
    let rest = line[caret..close].trim();
    // Only between attributes: what comes before is one whole attribute.
    let complete = first.is_empty()
        || first.split_once('=').is_some_and(|(key, value)| !key.trim().is_empty() && !value.trim().is_empty());
    if !complete {
        return None;
    }
    let word_after: String = line[caret..]
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    let inner = format!("{base_indent}{}", indent_unit(source));
    let mut entry_prefix = format!("\n{inner}");
    if !first.is_empty() {
        entry_prefix.push_str(first);
        entry_prefix.push('\n');
        entry_prefix.push_str(&inner);
    }
    let mut entry_suffix = String::new();
    if !rest.is_empty() {
        entry_suffix.push('\n');
        entry_suffix.push_str(&inner);
        entry_suffix.push_str(rest);
    }
    entry_suffix.push('\n');
    entry_suffix.push_str(base_indent);
    entry_suffix.push('}');
    let utf16_at = |byte: usize| utf16(&source[..line_start + byte]);
    Some(CompletionContext {
        path: path.to_vec(),
        typed: line[typed_start..caret].to_string(),
        replace_start: utf16_at(typed_start),
        replace_end: utf16_at(caret),
        base_indent: inner,
        entry_prefix,
        entry_suffix,
        entry_after: (!word_after.is_empty()).then_some(word_after),
        insert_range: Some((utf16_at(open + 1), utf16_at(close + 1))),
        position: EntryPosition::Block,
        ..Default::default()
    })
}

pub(super) fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let scan = analyze(source, cursor);
    let snapshot = &scan.snapshot;
    let path: Vec<SchemaPathSegment> = body_path(&scan, snapshot.body)
        .into_iter()
        .map(|segment| match segment.parse::<usize>() {
            Ok(index) => SchemaPathSegment::ArrayItem(index),
            Err(_) => SchemaPathSegment::Key(segment),
        })
        .collect();

    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let base_indent: String = source[line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let line_end = source[cursor..]
        .find('\n')
        .map_or(source.len(), |index| cursor + index);
    let line = &source[line_start..line_end];
    let caret_in_line = cursor.saturating_sub(line_start).min(line.len());
    let before_caret = &line[..caret_in_line];

    // Right after a block's `}` on its line (`}|`, `}h|`): no entry can start
    // there (a `}` closing an interpolation inside a string is no block's).
    let in_string = super::tokens::tokenize(line).iter().any(|token| {
        matches!(token.kind, super::tokens::Kind::Str { .. })
            && token.start < caret_in_line
            && caret_in_line < token.end
    });
    if !in_string
        && before_caret
            .trim_end_matches(|character: char| character.is_alphanumeric() || matches!(character, '_' | '-'))
            .ends_with('}')
    {
        return CompletionContext {
            path,
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            base_indent,
            no_properties: true,
            ..Default::default()
        };
    }

    // The caret sits in a comment: offer nothing typed, keep the path.
    if line_comment_start(before_caret).is_some() {
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }

    // A block label (`services "|" {`) names an entry; it is not a property.
    if in_block_label(before_caret) {
        return CompletionContext {
            path,
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            base_indent,
            no_properties: true,
            ..Default::default()
        };
    }

    if let Some(context) = in_one_line_block(source, line_start, line, caret_in_line, &path, &base_indent) {
        return context;
    }

    // Inside containers opened on this line (`a = { b = [{ … }, { | }] }`):
    // the path above the line comes from the scan at the line start, the
    // rest from the line itself.
    // The statement may span lines (`certificates = [` … the caret).
    if let Some(statement) = statement_start(&source[..cursor])
        && let Some(caret) = inline_caret(&source[statement..cursor], b'=')
    {
        let before_caret = &source[statement..cursor];
        let outer = analyze(source, statement);
        let mut full: Vec<SchemaPathSegment> = body_path(&outer, outer.snapshot.body)
            .into_iter()
            .map(|segment| match segment.parse::<usize>() {
                Ok(index) => SchemaPathSegment::ArrayItem(index),
                Err(_) => SchemaPathSegment::Key(segment),
            })
            .collect();
        if outer.snapshot.in_map {
            full.extend(outer.snapshot.value_prefix.iter().cloned().map(SchemaPathSegment::Key));
        }
        full.extend(caret.path);
        let value_end = caret_value_end(source, cursor, line_end);
        let typed = |start: usize| before_caret[start..].trim_end().trim_start_matches('"').to_string();
        return match caret.entry {
            // After a value with no comma yet, the comma goes right after it.
            CaretEntry::Key { start } => in_front_of_inline_entry(
                CompletionContext {
                    typed: typed(start),
                    replace_start: utf16(&source[..statement + caret.separator_at.unwrap_or(start)]),
                    entry_prefix: if caret.separator_at.is_some() { ", ".into() } else { String::new() },
                    replace_end: utf16(&source[..swallow_existing_assignment(source, cursor)]),
                    explicit_entry_start: true,
                    block: hcl_block(source, statement + start, cursor),
                    base_indent,
                    path: full,
                    // An object expression holds attributes, never blocks.
                    position: EntryPosition::Inline,
                    ..Default::default()
                },
                &source[cursor..line_end],
                b'=',
            ),
            CaretEntry::Value { key, start } => CompletionContext {
                typed: typed(start),
                replace_start: utf16(&source[..statement + start]),
                replace_end: utf16(&source[..value_end]),
                value_key: Some(key),
                value_needs_space: start == before_caret.len() && !before_caret.ends_with([' ', '\t']),
                base_indent,
                path: full,
                ..Default::default()
            },
            CaretEntry::Item { index, start } => {
                full.push(SchemaPathSegment::ArrayItem(index));
                CompletionContext {
                    typed: typed(start),
                    replace_start: utf16(&source[..statement + start]),
                    replace_end: utf16(&source[..value_end]),
                    is_array_item: true,
                    entry_wrap: Some(("{ ", " }")),
                    base_indent,
                    path: full,
                    position: EntryPosition::Inline,
                    ..Default::default()
                }
            }
        };
    }

    // Value position.
    if snapshot.in_value
        && let Some(key) = snapshot.value_key.clone()
    {
        // Inside `key = { … }` the caret completes a nested map key, at any depth.
        if snapshot.in_map {
            let mut map_path = path.clone();
            map_path.extend(
                snapshot
                    .value_prefix
                    .iter()
                    .cloned()
                    .map(SchemaPathSegment::Key),
            );
            // `mode = |` on its own line inside a multi-line map.
            if let Some((key, start)) = line_assignment(before_caret) {
                let typed_start = line_start + start;
                let value_end = caret_value_end(source, cursor, line_end);
                return CompletionContext {
                    typed: source[typed_start..cursor].trim_end().to_string(),
                    replace_start: utf16(&source[..typed_start]),
                    replace_end: utf16(&source[..value_end]),
                    value_key: Some(key),
                    value_needs_space: typed_start == cursor && !before_caret.ends_with([' ', '\t']),
                    base_indent,
                    path: map_path,
                    ..Default::default()
                };
            }
            let mut map_path = path.clone();
            map_path.extend(
                snapshot
                    .value_prefix
                    .iter()
                    .cloned()
                    .map(SchemaPathSegment::Key),
            );
            let (typed_start, typed) = match snapshot.partial {
                Some(token) if token.start < cursor => {
                    let text = source[token.start..cursor].to_string();
                    let text = match token.kind {
                        Kind::Str { .. } => text.strip_prefix('"').unwrap_or(&text).to_string(),
                        _ => text,
                    };
                    (token.start, text.trim_end().to_string())
                }
                _ => (cursor, String::new()),
            };
            let replace_end = swallow_existing_assignment(
                source,
                snapshot
                    .partial
                    .map_or(cursor, |token| token.end.max(cursor))
                    .min(source.len()),
            );
            return CompletionContext {
                block: hcl_block(source, typed_start, replace_end),
                typed,
                replace_start: utf16(&source[..typed_start]),
                replace_end: utf16(&source[..replace_end]),
                value_key: None,
                value_needs_space: false,
                is_array_item: false,
                base_indent,
                path: map_path,
                entry_prefix: String::new(),
                explicit_entry_start: true,
                entry_suffix: String::new(),
                // Inside `key = { … }`: an object expression.
                position: EntryPosition::Inline,
                ..Default::default()
            };
        }
        let typed_start = snapshot
            .partial
            .map_or(cursor, |token| token.start.max(snapshot.value_start))
            .min(cursor);
        let replace_end = snapshot
            .partial
            .map_or(cursor, |token| token.end.max(cursor))
            .min(line_end);
        return CompletionContext {
            typed: source[typed_start..cursor].trim_end().to_string(),
            replace_start: utf16(&source[..typed_start]),
            replace_end: utf16(&source[..replace_end]),
            value_key: Some(key),
            // Only directly after `=`; a space already typed is kept as is.
            value_needs_space: snapshot.partial.is_none() && source[..cursor].ends_with('='),
            is_array_item: false,
            base_indent,
            path,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }
    if let Some((key, value_begin)) = line_assignment(before_caret) {
        let typed_start = line_start + value_begin;
        let value_len = value_token_len(&source[cursor..line_end]);
        return CompletionContext {
            typed: source[typed_start..cursor].trim_end().to_string(),
            replace_start: utf16(&source[..typed_start]),
            replace_end: utf16(&source[..cursor + value_len]),
            value_key: Some(key),
            value_needs_space: source[typed_start..cursor].is_empty()
                && before_caret.trim_end().ends_with('='),
            is_array_item: false,
            base_indent,
            path,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }

    // Key position.
    let (typed_start, typed) = match snapshot.partial {
        Some(token) if token.start < cursor => {
            let text = source[token.start..cursor].to_string();
            let text = match token.kind {
                Kind::Str { .. } => text.strip_prefix('"').unwrap_or(&text).to_string(),
                _ => text,
            };
            (token.start, text.trim_end().to_string())
        }
        _ => (cursor, String::new()),
    };
    // Typed in front of an existing entry (`lim|listen = 1`): the new entry
    // goes before it, which moves to its own line (see `entry_after`).
    let rest = &source[cursor..line_end];
    let word_after: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    // An attribute (`listen = 1`) or a block header (`tcp {`, `services "web" {`).
    let header_follows = {
        let after = rest[word_after.len()..].trim_start();
        !word_after.is_empty()
            && (after.starts_with(['{', '"'])
                || (after.starts_with(|character: char| character.is_alphabetic()) && after.contains('{') && !after.contains('=')))
    };
    let splits = entry_follows(rest, b'=') || header_follows;
    let replace_end = if splits {
        cursor
    } else {
        swallow_existing_assignment(
            source,
            snapshot
                .partial
                .map_or(cursor, |token| token.end.max(cursor))
                .min(source.len()),
        )
    };
    CompletionContext {
        block: hcl_block(source, typed_start, replace_end.min(source.len())),
        path,
        typed,
        replace_start: utf16(&source[..typed_start]),
        replace_end: utf16(&source[..replace_end.min(source.len())]),
        entry_suffix: if splits { format!("\n{base_indent}") } else { String::new() },
        entry_after: splits.then_some(word_after),
        base_indent,
        ..Default::default()
    }
}

/// `web = { le| }`: braces opened earlier on the caret's line are still open,
/// so a multi-line insert moves onto its own line inside them.
pub(super) fn hcl_block(source: &str, typed_start: usize, replace_end: usize) -> Option<InlineBlock> {
    let line_start = source[..typed_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    inline_block(
        source,
        typed_start,
        replace_end,
        unclosed_brackets(&source[line_start..typed_start]),
        &indent_unit(source),
        |rest| rest.starts_with(['}', ']']),
        ",",
    )
}

/// The assignment on a line fragment: the key before the first `=` outside
/// strings and comments, plus the byte offset (relative to the line start)
/// where the value begins. Comment-only fragments yield `None`.
pub(super) fn line_assignment(line: &str) -> Option<(String, usize)> {
    let code = code_before_comment(line)?;
    let equals = assignment_equals(code)?;
    let raw = code[..equals].trim();
    if raw.is_empty() || !is_ident(raw) {
        return None;
    }
    let after = &code[equals + 1..];
    let value_begin = equals + 1 + (after.len() - after.trim_start_matches([' ', '\t']).len());
    Some((raw.to_string(), value_begin))
}

/// Extends a key replacement end past an `=` already typed after the key
/// (`key| = value`): the key insert supplies its own `" = "` and value, so the
/// old separator and value would otherwise be duplicated.
pub(super) fn swallow_existing_assignment(source: &str, replace_end: usize) -> usize {
    let rest = &source[replace_end..];
    let trimmed = rest.trim_start_matches([' ', '\t']);
    let Some(after_equals) = trimmed.strip_prefix('=') else {
        return replace_end;
    };
    let spaces = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
    let value_start = replace_end + (rest.len() - trimmed.len()) + 1 + spaces;
    let value_line_end = source[value_start..]
        .find('\n')
        .map_or(source.len(), |index| value_start + index);
    value_start + value_token_len(&source[value_start..value_line_end])
}

/// Byte index of the `=` separating key from value, ignoring `=` inside
/// quoted strings and `==`, `=>`, `!=`, `<=`, `>=` operators.
pub(super) fn assignment_equals(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let bytes = line.as_bytes();
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '=' {
            let prev = index
                .checked_sub(1)
                .and_then(|prev| bytes.get(prev))
                .copied();
            let next = bytes.get(index + 1).copied();
            if matches!(prev, Some(b'=' | b'!' | b'<' | b'>'))
                || next == Some(b'=')
                || next == Some(b'>')
            {
                continue;
            }
            return Some(index);
        }
    }
    None
}

pub(super) fn is_ident(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

/// Length of the scalar value starting at the caret: runs to the comment or
/// line end for bare scalars, to the closing quote for strings.
pub(super) fn value_token_len(text: &str) -> usize {
    if text.trim_start().starts_with('"') {
        // Through the template sequences in it (`"${ "}" }"`).
        return string_end(text, text.len() - text.trim_start().len()).0;
    }
    match code_before_comment(text) {
        Some(code) => code.trim_end().len(),
        None => 0,
    }
}

/// Keys already present in the body holding the caret: attribute names plus,
/// at a label level, the labels already taken there. The partly typed key on
/// the active line is ignored so it can still be completed.
pub(super) fn existing_body_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    // Inside an inline container (an object in a list, a value map) its own
    // entries are the siblings, however many lines it spans.
    let cursor_byte = cursor_byte.min(source.len());
    if let Some(start) = statement_start(&source[..cursor_byte])
        && let Some((_, end)) = statements(source)
            .into_iter()
            .find(|(from, to)| *from <= start && cursor_byte <= *to)
        && let Some(keys) = inline_sibling_keys(&source[start..end], b'=', cursor_byte - start)
    {
        return Some(keys.into_iter().collect());
    }
    let scan = analyze(source, cursor_byte);
    let wanted: Vec<String> = path
        .iter()
        .map(|segment| match segment {
            SchemaPathSegment::Key(key) => key.clone(),
            SchemaPathSegment::ArrayItem(index) => index.to_string(),
        })
        .collect();
    let mut keys = HashSet::new();
    for attribute in &scan.attributes {
        // The attribute's container: its body's path plus the value-map chain
        // it is nested in (`clients = { acme = { … } }`).
        let mut container = body_path(&scan, attribute.body);
        container.extend(attribute.prefix.iter().cloned());
        if container == wanted {
            keys.insert(attribute.name.clone());
        }
    }
    for header in &scan.headers {
        let parent = body_path(&scan, header.parent);
        if parent == wanted {
            // A block type present in this body shadows an attribute of that name.
            keys.insert(header.block_type.clone());
        }
        let mut base = parent;
        base.push(header.block_type.clone());
        // Labels already taken at the level the path points into.
        if let Some(rest) = wanted.strip_prefix(base.as_slice()) {
            let level = rest.len();
            if level < header.labels.len() && rest == &header.labels[..level] {
                keys.insert(header.labels[level].clone());
            }
        }
    }
    if keys.is_empty() {
        return None;
    }
    // The key being typed on the active line stays completable.
    let line_start = source[..cursor_byte.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    // Only a line that already defines it (`htt|p2 = true`, `pol| {`): a bare
    // word still being typed is a new key and must not repeat an existing one.
    let line = &source[line_start..line_end];
    let defines = line.contains('=') || line.trim_end().ends_with('{');
    if defines && let Some(current) = current_line_key(line) {
        keys.remove(&current);
    }
    Some(keys)
}

/// The key or label being typed on a single line: the assignment key, the
/// block type word, or a quoted label under the caret region.
pub(super) fn current_line_key(line: &str) -> Option<String> {
    let code = code_before_comment(line)?;
    if let Some(equals) = assignment_equals(code) {
        let raw = code[..equals].trim();
        return is_ident(raw).then(|| raw.to_string());
    }
    let mut tokens = tokenize(code)
        .into_iter()
        .filter(|token| matches!(token.kind, Kind::Ident | Kind::Str { .. }));
    let first = token_name(code, &tokens.next()?)?;
    // A header line contributes its type word only when no label follows it on
    // this line; labels are completed at their own level.
    if tokens.next().is_none() {
        return Some(first);
    }
    None
}

/// Scalars render `key = value`; objects expand to a block over their
/// required children.
pub(super) fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
    in_body: bool,
) -> Option<(String, usize)> {
    if key.is_empty() || !is_ident(key) {
        return None;
    }
    let mut insert = format!("{key} = ");
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&hcl_text(value));
        let caret = utf16(&insert);
        return Some((insert, caret));
    }
    if array_branch(property, root).is_some() {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => insert.push_str(&hcl_text(default)),
            None => {
                insert.push_str("[]");
                let caret = utf16(&insert) - 1;
                return Some((insert, caret));
            }
        }
        let caret = utf16(&insert);
        return Some((insert, caret));
    }
    if object_branch(property, root).is_some() {
        if let Some(default) = schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
        {
            insert.push_str(&hcl_text(default));
            let caret = utf16(&insert);
            return Some((insert, caret));
        }
        let child_indent = format!("{base_indent}{unit}");
        // In a body an object is a block (`heartbeat {`), as documents and
        // the converter write it; in an object expression, an attribute.
        // Maps too (`clients`, `services`): the converter writes every
        // object with identifier keys as a block.
        if in_body {
            insert = format!("{key} ");
        }
        insert.push_str("{\n");
        let mut caret = None;
        if let Some(branch) = object_branch(property, root)
            && let Some(required) = required_children(branch)
        {
            for (name, child) in required {
                insert.push_str(&child_indent);
                insert.push_str(&name);
                insert.push_str(" = ");
                // The first child takes the caret; the others get a valid
                // placeholder so the document stays parseable.
                let takes_caret = caret.is_none();
                if takes_caret {
                    caret = Some(utf16(&insert));
                }
                match schema_default(child, root) {
                    Some(default) => insert.push_str(&hcl_text(default)),
                    None if !takes_caret => insert.push_str(&hcl_text(&fill_value(child, root))),
                    // The caret's own child: a string starts quoted, caret inside.
                    None if placeholder(child, root).is_string() => {
                        insert.push_str("\"\"");
                        caret = caret.map(|position| position + 1);
                    }
                    None => {}
                }
                insert.push('\n');
            }
        }
        if caret.is_none() {
            // No required children: the caret waits on an indented line inside.
            insert.push_str(&child_indent);
            caret = Some(utf16(&insert));
            insert.push('\n');
        }
        insert.push_str(base_indent);
        insert.push('}');
        return Some((insert, caret.unwrap_or_default()));
    }
    if let Some(default) = schema_default(property, root) {
        insert.push_str(&hcl_text(default));
    } else if placeholder(property, root).is_string() {
        // A string value starts quoted, with the caret inside.
        insert.push_str("\"\"");
        let caret = utf16(&insert) - 1;
        return Some((insert, caret));
    }
    let caret = utf16(&insert);
    Some((insert, caret))
}

/// The required properties of an object schema that have their own schema, in
/// the order the schema lists them.
pub(super) fn required_children(branch: &Value) -> Option<Vec<(String, &Value)>> {
    let properties = branch.get("properties").and_then(Value::as_object)?;
    let required = branch.get("required").and_then(Value::as_array)?;
    let children: Vec<(String, &Value)> = required
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|name| properties.get(name).map(|child| (name.to_string(), child)))
        .collect();
    (!children.is_empty()).then_some(children)
}

/// Renders a schema value as HCL text. Strings always quote: HCL has no bare
/// string scalars, and a bare word would parse back as a reference.
pub(super) fn hcl_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(number) => number.to_string(),
        // Template sequences are escaped, so `${HOST}` stays text, as the
        // serializer writes it.
        Value::String(text) => super::heredoc::quoted(super::super::super::codec::non_finite::written(text)),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(hcl_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .filter(|(key, _)| is_ident(key))
                .map(|(key, child)| format!("{key} = {}", hcl_text(child)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
    }
}

/// Whether the caret ends `before_caret` at a label of a block header: a
/// quoted one (`services "|`) or a bare word after the block type
/// (`services we|`, `services |`). A word, then labels, no `=`.
fn in_block_label(before_caret: &str) -> bool {
    let code = before_caret.trim_start();
    if assignment_equals(code).is_some() || !code.starts_with(|character: char| character.is_alphabetic() || character == '_') {
        return false;
    }
    let block_type_end = code
        .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-')))
        .unwrap_or(code.len());
    let after_type = &code[block_type_end..];
    let bare_label = after_type.starts_with([' ', '\t'])
        && after_type
            .split_whitespace()
            .all(|word| word.chars().all(|character| character.is_alphanumeric() || matches!(character, '_' | '-')));
    if bare_label {
        return true;
    }
    let mut open = false;
    let mut escaped = false;
    for character in code.chars() {
        match character {
            _ if escaped => escaped = false,
            '\\' if open => escaped = true,
            '"' => open = !open,
            '{' | '[' | '(' if !open => return false,
            _ => {}
        }
    }
    open
}
