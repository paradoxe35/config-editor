//! Schema docs for the key under the pointer.

use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeyHover {
    pub(crate) key: String,
    pub(crate) start_column: usize,
    pub(crate) type_label: String,
    pub(crate) description: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) required: bool,
}

/// The key under character `column` of zero-based `line_index`, with its
/// schema documentation.
pub(crate) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let bundle = schema?;
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
    let parent = &path[..path.len() - 1];
    let root = &bundle.schema;
    let schemas = schema_at_path(root, &path, behavior.repetition() != Repetition::None);
    if schemas.is_empty() {
        return None;
    }
    let branches: Vec<&Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .collect();
    let description = branches.iter().find_map(|branch| {
        branch
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
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
    let required = schema_at_path(root, parent, behavior.repetition() != Repetition::None)
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(&key)))
        });
    Some(KeyHover {
        key,
        start_column,
        type_label: schema_type_label(schemas[0], root, "property"),
        description,
        default,
        examples,
        required,
    })
}
