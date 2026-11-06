//! Where suggestions apply and how accepted keys and values are written.

use super::*;

pub(super) fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let mut scan = analyze(source, cursor);
    let Some(snapshot) = scan.snapshot.take() else {
        unreachable!("analyze always captures a snapshot");
    };
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

    if snapshot.in_tag {
        return tag_completion_context(source, cursor, &snapshot, path, base_indent);
    }

    // In text: the typed fragment runs from the last `>` (or line start) to
    // the caret, extended through a partial `<name` when the caret sits right
    // after it (`<roo|` completes the element `root`).
    let (typed, replace_start, kind) = text_typed(
        source,
        cursor,
        snapshot.partial.map(|token| (token.start, token.end)),
    );
    let explicit_entry_start = kind == Typed::TagName;
    // Text right after a tag (`<listen>7|`) completes the element's value from
    // its parent path; a word on its own line names a new child element.
    // Between an element's own tags (`<mode>|</mode>`) the caret may type
    // its value or, for an object, its first child.
    let before_own_end = matches!(snapshot.path.last(), Some(Seg::Key(key)) if source[cursor..].starts_with(&format!("</{key}>")));
    let between_own_tags = typed.is_empty() && before_own_end;
    // A word typed there (`<web>certif|</web>`) may still name a child.
    let may_be_child = before_own_end && !typed.trim().contains(char::is_whitespace);
    if (!typed.trim().is_empty() || between_own_tags)
        && kind == Typed::ElementText
        && let Some(Seg::Key(key)) = snapshot.path.last()
    {
        let mut parent = snapshot.path.clone();
        parent.pop();
        let path: Vec<SchemaPathSegment> = parent
            .iter()
            .map(|segment| match segment {
                Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
                Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
            })
            .collect();
        return CompletionContext {
            path,
            typed,
            replace_start,
            // The whole value is replaced, not just up to the caret.
            replace_end: {
                let rest = &source[cursor..];
                let value_end = rest.find(['<', '\n']).unwrap_or(rest.len());
                if rest[value_end..].starts_with('<') {
                    cursor_bytes(source, cursor + rest[..value_end].trim_end().len())
                } else {
                    cursor_bytes(source, cursor)
                }
            },
            value_key: Some(key.clone()),
            value_or_children: may_be_child,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }
    let typed_byte = utf16_to_byte_index(source, replace_start);
    CompletionContext {
        block: xml_block(source, typed_byte, cursor),
        path,
        typed,
        replace_start,
        replace_end: cursor_bytes(source, cursor),
        value_key: None,
        value_needs_space: false,
        is_array_item: false,
        base_indent,
        entry_prefix: String::new(),
        explicit_entry_start,
        entry_suffix: String::new(),
..Default::default()
    }
}

/// Completion inside `<...>`: an element name, attribute name, or attribute
/// value. `pending_attr` (`name = |`) and a caret inside quotes complete the
/// value; a bare word completes the attribute; `<` or `</` plus a name
/// completes the element.
pub(super) fn tag_completion_context(
    source: &str,
    cursor: usize,
    snapshot: &Snapshot,
    path: Vec<SchemaPathSegment>,
    base_indent: String,
) -> CompletionContext {
    let replace_end = cursor_bytes(source, cursor);
    // The config root's start tag holds only editor metadata: no schema
    // property or value belongs there.
    if snapshot.in_root_tag {
        return CompletionContext {
            replace_start: replace_end,
            replace_end,
            base_indent,
            no_properties: true,
..Default::default()
        };
    }
    let holder = snapshot.open_tag_path.clone();
    if let Some((name, start, end)) = quoted_value_at(source, &snapshot.inner, cursor) {
        let value_path = attribute_value_path(&holder, &name);
        let typed = source[start.min(cursor)..cursor].to_string();
        return CompletionContext {
            path: value_path.0,
            typed,
            replace_start: cursor_bytes(source, start.min(cursor)),
            replace_end: cursor_bytes(source, end.max(cursor).min(source.len())),
            value_key: value_path.1,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
..Default::default()
        };
    }
    if let Some(name) = snapshot.pending_attr.clone() {
        let value_path = attribute_value_path(&holder, &name);
        return CompletionContext {
            path: value_path.0,
            typed: String::new(),
            replace_start: replace_end,
            replace_end,
            value_key: value_path.1,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
..Default::default()
        };
    }
    // Just after the name of a complete tag (`<host|>`) the context is the
    // element's own value, addressed from its parent: hover reads schema
    // documentation there.
    if let Some(tag) = snapshot.open_tag
        && matches!(tag.kind, Kind::OpenTag { name_end, .. } if name_end == cursor)
        && source[cursor..]
            .trim_start_matches([' ', '\t'])
            .starts_with(['>', '/'])
    {
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: replace_end,
            replace_end,
            value_key: Some(token_name(source, &tag)),
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
..Default::default()
        };
    }
    // The insert carries its own `<`, so the replaced range starts at it.
    if let Some(tag) = snapshot
        .open_tag
        .filter(|tag| is_element_name_position(tag, cursor))
    {
        let name_start = (tag.start + 1).min(cursor);
        return CompletionContext {
            block: xml_block(source, tag.start.min(cursor), cursor),
            path,
            typed: source[name_start..cursor].to_string(),
            replace_start: cursor_bytes(source, tag.start.min(cursor)),
            replace_end,
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: true,
            entry_suffix: String::new(),
..Default::default()
        };
    }
    let word = word_at(&snapshot.inner, cursor);
    let (typed, replace_start) = match word {
        Some((start, end)) => (
            source[start..cursor.min(end)].to_string(),
            cursor_bytes(source, start),
        ),
        None => (String::new(), replace_end),
    };
    // Typed in front of an attribute (`<tcp k|nodelay="true"/>`): a suggestion
    // completing its name renames it, any other goes before it.
    let name_after = word
        .filter(|(_, end)| *end > cursor)
        .map(|(_, end)| source[cursor..end].to_string());
    let entry_suffix = if name_after.is_some() { " ".to_string() } else { String::new() };
    CompletionContext {
        entry_after: name_after,
        path: attribute_holder_path(&holder),
        typed,
        replace_start,
        replace_end,
        value_key: None,
        value_needs_space: false,
        is_array_item: false,
        base_indent,
        entry_prefix: String::new(),
        explicit_entry_start: false,
        entry_suffix,
        position: EntryPosition::Attribute,
..Default::default()
    }
}

/// `<web><le|</web>`: elements opened earlier on the caret's line are still
/// open, so a multi-line element insert moves onto its own line inside them
/// and the closing tag that followed moves to its own line too.
pub(super) fn xml_block(source: &str, start: usize, end: usize) -> Option<InlineBlock> {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..start];
    let mut depth = 0usize;
    for token in tokenize(line) {
        match token.kind {
            Kind::OpenTag { .. }
                if !token_name(line, &token).is_empty()
                    && line[..token.end].ends_with('>')
                    && !line[..token.end].ends_with("/>") =>
            {
                depth += 1;
            }
            Kind::EndTag => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    inline_block(
        source,
        start,
        end,
        depth,
        &indent_unit(source),
        |rest| rest.starts_with("</"),
        "",
    )
}

pub(super) fn attribute_value_path(path: &[Seg], name: &str) -> (Vec<SchemaPathSegment>, Option<String>) {
    (attribute_holder_path(path), Some(name.to_string()))
}

pub(super) fn attribute_holder_path(path: &[Seg]) -> Vec<SchemaPathSegment> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect()
}

/// `(name, start, end)` of the quoted value around the caret, excluding quotes.
pub(super) fn quoted_value_at(source: &str, inner: &[Token], cursor: usize) -> Option<(String, usize, usize)> {
    for (position, token) in inner.iter().enumerate() {
        if !matches!(token.kind, Kind::Text { quoted: true }) {
            continue;
        }
        if !(token.start < cursor && cursor <= token.end) {
            continue;
        }
        // The attribute name is `name = "value"`: word, `=`, then this quote.
        let name = (position >= 2
            && matches!(inner[position - 1].kind, Kind::Equals)
            && matches!(inner[position - 2].kind, Kind::Text { quoted: false }))
        .then(|| source[inner[position - 2].start..inner[position - 2].end].to_string())?;
        let quote_len = source[token.start..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        let start = token.start + quote_len;
        let ends_with_quote = token.end > start
            && source[..token.end.min(source.len())]
                .chars()
                .next_back()
                .is_some_and(|character| character == '"' || character == '\'');
        let end = token.end - usize::from(ends_with_quote);
        return Some((name, start.min(cursor), end.max(start)));
    }
    None
}

pub(super) fn word_at(inner: &[Token], cursor: usize) -> Option<(usize, usize)> {
    inner.iter().find_map(|token| {
        if !matches!(token.kind, Kind::Text { quoted: false }) {
            return None;
        }
        (token.start < cursor && cursor <= token.end).then_some((token.start, token.end))
    })
}

/// Whether the caret is within the element name of `tag` (`<na|me`), as opposed
/// to after it, where it completes an attribute.
pub(super) fn is_element_name_position(tag: &Token, cursor: usize) -> bool {
    match tag.kind {
        Kind::OpenTag { name_end, .. } => tag.start < cursor && cursor <= name_end,
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Typed {
    /// A partial `<name`: an element name (an explicit entry start).
    TagName,
    /// Text right after a `>` on the same line (`<listen>78|`): the element's
    /// own value.
    ElementText,
    /// A bare word on a line of its own (`  vis|`): the name of a new element
    /// typed without its `<`.
    BareName,
}

/// Text around the caret outside tags: the typed fragment, its UTF-16 start,
/// and what kind of entry it is.
pub(super) fn text_typed(
    source: &str,
    cursor: usize,
    partial: Option<(usize, usize)>,
) -> (String, usize, Typed) {
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    // Typed text never reaches back across a line break: a `>` on an earlier
    // line (`</clients>\n  vis|`) must not pull the newline and indentation
    // into the replaced range, or accepting would join the lines.
    let after_tag = source[line_start..cursor].rfind('>');
    let segment_start = after_tag.map_or(line_start, |index| line_start + index + 1);
    if let Some((start, end)) = partial
        && source[start..].starts_with('<')
        && start >= segment_start
    {
        // The insert carries its own `<`, so the replaced range starts at it.
        return (
            source[start + 1..cursor.min(end)].to_string(),
            cursor_bytes(source, start),
            Typed::TagName,
        );
    }
    // Leading whitespace is indentation, not part of the query: the replaced
    // range starts at the first typed character so indentation survives.
    let segment = &source[segment_start..cursor];
    let typed_start = segment_start + (segment.len() - segment.trim_start().len());
    let kind = if after_tag.is_some() {
        Typed::ElementText
    } else {
        Typed::BareName
    };
    (
        source[typed_start..cursor].to_string(),
        cursor_bytes(source, typed_start),
        kind,
    )
}

pub(super) fn cursor_bytes(source: &str, cursor: usize) -> usize {
    source[..cursor.min(source.len())].encode_utf16().count()
}

/// Names to filter from suggestions. Only attribute positions filter;
/// repeated sibling elements are arrays, so element names never do.
pub(super) fn existing_elements(source: &str, cursor_byte: usize) -> Option<HashSet<String>> {
    let mut scan = analyze(source, cursor_byte);
    let snapshot = scan.snapshot.take()?;
    let naming_element = snapshot
        .open_tag
        .is_some_and(|tag| is_element_name_position(&tag, cursor_byte));
    if snapshot.in_tag && !naming_element {
        return snapshot
            .pending_attr
            .is_none()
            .then(|| existing_attributes(source, &snapshot));
    }
    // Children and attributes already on the element holding the caret; the
    // tag being typed does not count. Arrays may still repeat (see
    // `repetition`).
    let holder = strings(&snapshot.path);
    Some(
        scan.key_spans
            .iter()
            .filter(|(_, start, end)| !(*start <= cursor_byte && cursor_byte <= *end))
            .filter(|(segments, ..)| segments.len() > holder.len() && segments.starts_with(&holder))
            .map(|(segments, ..)| segments[holder.len()].clone())
            .collect(),
    )
}

/// Attribute names already on the tag under the caret.
pub(super) fn existing_attributes(source: &str, snapshot: &Snapshot) -> HashSet<String> {
    let mut names = HashSet::new();
    for (position, token) in snapshot.inner.iter().enumerate() {
        if !matches!(token.kind, Kind::Text { quoted: false }) {
            continue;
        }
        let is_name = snapshot
            .inner
            .get(position + 1)
            .is_some_and(|next| matches!(next.kind, Kind::Equals));
        if is_name {
            names.insert(source[token.start..token.end].to_string());
        }
    }
    names
}

/// Elements expand to `<key>|</key>` (or a block with required children);
/// attributes insert `name="|"`.
pub(super) fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    context: &InsertContext<'_>,
) -> Option<(String, usize)> {
    if context.position == EntryPosition::Attribute {
        return attribute_insert(key, property, root);
    }
    let (base_indent, unit) = (context.base_indent, context.unit);
    let mut insert = format!("<{key}>");
    let mut caret = None;
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&escape_text(&xml_text(value)));
    } else if let Some(branch) = object_branch(property, root).or_else(|| item_object(property, root)) {
        // An array default lists items, which are separate elements.
        match schema_default(property, root)
            .filter(|value| !value.is_array() && !value.as_object().is_some_and(Map::is_empty))
        {
            Some(default) => insert.push_str(&escape_text(&xml_text(default))),
            None => {
                let children = required_children(branch);
                if children.is_empty() {
                    // No required children: leave the caret between the tags.
                    caret = Some(insert.len());
                } else {
                    let child_indent = format!("{base_indent}{unit}");
                    insert.push('\n');
                    for (name, child) in &children {
                        if caret.is_none() {
                            caret = Some(insert.len() + child_indent.len() + name.len() + 2);
                        }
                        insert.push_str(&child_indent);
                        insert.push_str(&format!("<{name}>"));
                        if let Some(default) = schema_default(child, root) {
                            insert.push_str(&escape_text(&xml_text(default)));
                        }
                        insert.push_str(&format!("</{name}>\n"));
                    }
                    insert.push_str(base_indent);
                }
            }
        }
    } else if let Some(default) = schema_default(property, root).filter(|value| !value.is_array()) {
        insert.push_str(&escape_text(&xml_text(default)));
    }
    let caret = caret.unwrap_or(insert.len());
    insert.push_str(&format!("</{key}>"));
    let caret = utf16(&insert[..caret]);
    Some((insert, caret))
}

/// The object schema of an array's items: each repeated element is one item,
/// so accepting the array key writes an item.
pub(super) fn item_object<'a>(property: &'a Value, root: &'a Value) -> Option<&'a Value> {
    let items = array_branch(property, root)?.get("items")?;
    object_branch(items, root)
}

/// `name="|"`, or with the default/const filled in. Objects and arrays have
/// no attribute form and yield `None`.
pub(super) fn attribute_insert(name: &str, property: &Value, root: &Value) -> Option<(String, usize)> {
    if object_branch(property, root).is_some() || array_branch(property, root).is_some() {
        return None;
    }
    let mut insert = format!("{name}=\"");
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&escape_attribute(&xml_text(value)));
    } else if let Some(default) = schema_default(property, root) {
        insert.push_str(&escape_attribute(&xml_text(default)));
    }
    let caret = utf16(&insert);
    insert.push('"');
    Some((insert, caret))
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
        .filter_map(|name| {
            let key = name.strip_prefix('@').unwrap_or(name);
            properties.get(key).map(|child| (key.to_string(), child))
        })
        .collect()
}

pub(super) fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;")
}

pub(super) fn escape_attribute(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}
