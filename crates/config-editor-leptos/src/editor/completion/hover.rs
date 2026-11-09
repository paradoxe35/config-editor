//! Schema docs for the key or documented value under the pointer.

use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeyHover {
    pub(crate) key: String,
    pub(crate) start_column: usize,
    pub(crate) type_label: String,
    /// The property's `title`, shown above its description.
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) required: bool,
    pub(crate) deprecated: bool,
    /// What to use instead, when the schema says (`deprecationMessage`).
    pub(crate) deprecation: Option<String>,
    /// The property's `x-docs-url`, a web address.
    pub(crate) docs_url: Option<String>,
    /// The `oneOf`/`anyOf` option the field's object is in, or all of
    /// them while none matches (see `choices`).
    pub(crate) choice: Option<ChoiceHover>,
    /// Set when the pointer is over one of the key's documented values
    /// rather than the key.
    pub(crate) value: Option<HoverValue>,
}

/// A documented value under the pointer: an `enum` entry with its
/// `x-enum-descriptions` line, or a `const` branch with its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HoverValue {
    pub(crate) label: String,
    pub(crate) doc: ValueDoc,
}

/// The key under character `column` of zero-based `line_index`, with its
/// schema documentation; or, over a value the schema documents, that value
/// with its key's.
pub(crate) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let bundle = schema?;
    if let Some((path, key, start_column)) = hovered_key(source, line_index, column, behavior) {
        return property_hover(source, &path, key, start_column, bundle, behavior);
    }
    // No key under the pointer: maybe a value the schema documents.
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    if behavior.hover_path(source, line_index, column).is_some()
        || behavior.hover_key(line, column).is_some()
    {
        return None;
    }
    value_hover(source, line_start, line, column, bundle, behavior)
}

/// The key under character `column` of zero-based `line_index`: its path in
/// the document, the key, and the column it starts at. Hover docs and the
/// caret's path (see `paths`) both read it.
pub(crate) fn hovered_key(
    source: &str,
    line_index: usize,
    column: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    let resolved = behavior.hover_path(source, line_index, column);
    let (key, start_column) = match &resolved {
        Some((_, key, start)) => (key.clone(), *start),
        None => behavior.hover_key(line, column)?,
    };
    let cursor = source[..line_start + behavior.hover_cursor(line, column)]
        .encode_utf16()
        .count();
    let context = behavior.completion_context(source, cursor);
    // The hovered key is either the key whose value the context is in, or the
    // container the context is inside (a TOML `[table]` header, an HCL block
    // name, an XML element with attributes).
    let path = if let Some((path, _, _)) = resolved {
        path
    } else if context.value_key.as_deref() == Some(key.as_str()) {
        path_with_key(&context.path, &key)
    } else {
        let mut container = context.path.clone();
        while matches!(container.last(), Some(SchemaPathSegment::ArrayItem(_))) {
            container.pop();
        }
        if context.value_key.is_some()
            || container.last() != Some(&SchemaPathSegment::Key(key.clone()))
        {
            return None;
        }
        container
    };
    (!path.is_empty()).then_some((path, key, start_column))
}

/// Over a value (`fast` of `mode: fast`) that the schema documents on its
/// own: the value's title and description, with its key's docs.
fn value_hover(
    source: &str,
    line_start: usize,
    line: &str,
    column: usize,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let (start, end) = value_token(line, column)?;
    let token = &line[start..end];
    // Where completion would offer this value: its context names the key.
    let cursor = source[..line_start + end].encode_utf16().count();
    let context = behavior.completion_context(source, cursor);
    let (path, key) = match context.value_key {
        Some(key) => (path_with_key(&context.path, &key), key),
        None if context.is_array_item => {
            let key = context
                .path
                .iter()
                .rev()
                .find_map(|segment| match segment {
                    SchemaPathSegment::Key(key) => Some(key.clone()),
                    SchemaPathSegment::ArrayItem(_) => None,
                })?;
            (context.path.clone(), key)
        }
        None => return None,
    };
    let root = &bundle.schema;
    let repeats = behavior.repetition() != Repetition::None;
    let doc = schema_at_path(root, &path, repeats)
        .into_iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .find_map(|branch| {
            schema_value_candidates(branch, false)
                .into_iter()
                .find(|candidate| value_label(candidate) == token)
                .and_then(|candidate| value_doc(branch, &candidate))
        })?;
    // The docs of the property the value belongs to (an array's, for an item).
    let mut hover = property_hover(
        source,
        property_path(&path),
        key,
        line[..start].chars().count(),
        bundle,
        behavior,
    )?;
    hover.value = Some(HoverValue {
        label: token.to_string(),
        doc,
    });
    Some(hover)
}

/// The path of the property `path` is in: itself, or the array an item
/// (a repeated block) belongs to.
fn property_path(path: &[SchemaPathSegment]) -> &[SchemaPathSegment] {
    let kept = path
        .iter()
        .rposition(|segment| !matches!(segment, SchemaPathSegment::ArrayItem(_)))
        .map_or(0, |last| last + 1);
    &path[..kept]
}

/// The byte range in `line` of the bare word at character `column`: a
/// value as written, without its quotes or the punctuation around it.
fn value_token(line: &str, column: usize) -> Option<(usize, usize)> {
    let stop = |character: char| character.is_whitespace() || "\"'`,:=[]{}<>#;".contains(character);
    let at = line.char_indices().nth(column)?.0;
    if line[at..].chars().next().is_none_or(stop) {
        return None;
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .find(|(_, character)| stop(*character))
        .map_or(0, |(index, character)| index + character.len_utf8());
    let end = line[at..]
        .find(stop)
        .map_or(line.len(), |offset| at + offset);
    Some((start, end))
}

/// The docs of the property at `path` in `source`, named `key`, shown at
/// `start_column`.
fn property_hover(
    source: &str,
    path: &[SchemaPathSegment],
    key: String,
    start_column: usize,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    // A repeated block (TOML `[[svc]]`, HCL `svc {`, XML `<svc>`) is an
    // item of the array its key names: the array's own docs come first,
    // the item's after.
    let property = property_path(path);
    let parent = &property[..property.len().checked_sub(1)?];
    let root = &bundle.schema;
    let repeats = behavior.repetition() != Repetition::None;
    let schemas = schema_at_path(root, path, repeats);
    if schemas.is_empty() {
        return None;
    }
    let owners = if property.len() < path.len() {
        schema_at_path(root, property, repeats)
    } else {
        Vec::new()
    };
    let documented: Vec<&Value> = owners.iter().chain(&schemas).copied().collect();
    let branches: Vec<&Value> = documented
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .collect();
    // A value option (a `const` branch) documents that value, not the key.
    let text_of = |name: &str| {
        branches
            .iter()
            .filter(|branch| branch.get("const").is_none())
            .find_map(|branch| branch.get(name).and_then(Value::as_str).map(str::to_string))
    };
    let description = text_of("description");
    let title = text_of("title").filter(|title| !title.trim().is_empty());
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
    let required = schema_at_path(root, parent, repeats)
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(&key)))
        });
    // A key of `oneOf`/`anyOf` options of the object holding it: the
    // option it is in (or those offering it), and its schema there.
    let parent_choices = ChoicesAt::in_object(&schema_at_path(root, parent, repeats), root, || {
        object_at(source, parent, behavior)
    });
    let holding = parent_choices.holding(&key);
    let type_label = if holding.is_empty() {
        schema_type_label(schemas[0], root, "property")
    } else {
        let schemas: Vec<&Value> = holding.iter().map(|(_, schema)| *schema).collect();
        type_label_of_all(&schemas, root, "property")
    };
    let choice = ChoicesAt::in_object(&schemas, root, || object_at(source, path, behavior))
        .hover()
        .or_else(|| parent_choices.key_hover(&key))
        // The option's description when it is not the property's own.
        .and_then(|choice| match choice {
            ChoiceHover::Matched(mut doc) => {
                if doc.description == description {
                    doc.description = None;
                }
                (doc != ValueDoc::default()).then_some(ChoiceHover::Matched(doc))
            }
            other => Some(other),
        });
    Some(KeyHover {
        key,
        start_column,
        type_label,
        title,
        description,
        default,
        examples,
        required,
        deprecated: documented.iter().any(|schema| is_deprecated(schema, root)),
        deprecation: documented
            .iter()
            .find_map(|schema| deprecation_message(schema, root)),
        docs_url: documented.iter().find_map(|schema| docs_url(schema, root)),
        choice,
        value: None,
    })
}

#[cfg(test)]
mod tests {
    use super::value_token;

    #[test]
    fn value_tokens_leave_out_quotes_and_markup() {
        let word = |line: &str, column: usize| {
            value_token(line, column).map(|(start, end)| line[start..end].to_string())
        };
        assert_eq!(word("mode: fast", 7).as_deref(), Some("fast"));
        assert_eq!(word("mode = \"fast\"", 9).as_deref(), Some("fast"));
        assert_eq!(word("<mode>fast</mode>", 7).as_deref(), Some("fast"));
        assert_eq!(word("  \"mode\": \"fast\",", 12).as_deref(), Some("fast"));
        assert_eq!(word("mode: fast", 4), None, "the separator is no value");
        assert_eq!(word("mode: fast", 40), None);
    }
}
