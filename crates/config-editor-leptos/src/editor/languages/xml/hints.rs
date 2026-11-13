//! What the schema says of how XML writes a value: the OpenAPI XML object's
//! `"xml": {"attribute": true}` on a property makes it an attribute.
//! Other views ignore the keyword (as the validator does), so one schema
//! serves every language.

use super::*;
use super::super::super::completion::{child_schemas, resolve_branches, schema_at_path};
use super::super::super::language::SchemaPathSegment;

/// Whether one of `schemas` (the schemas of one value) marks it an
/// attribute: `"xml": {"attribute": true}` on a branch.
pub(super) fn hinted_attribute(schemas: &[&Value], root: &Value) -> bool {
    schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .any(|branch| branch.pointer("/xml/attribute") == Some(&Value::Bool(true)))
}

/// The key paths (array indices left out, as [`AttributePaths`] holds
/// them) of the scalars of `value` that `schema` marks attributes, but not
/// those `old` (the document before) holds as elements: the text keeps
/// what it writes, the schema only decides what is new.
pub(super) fn schema_attribute_paths(value: &Value, schema: &Value, old: Option<&Value>) -> AttributePaths {
    let mut found = AttributePaths::new();
    walk(value, &[schema], schema, old, &mut Vec::new(), &mut found, 0);
    found
}

fn walk(
    value: &Value,
    schemas: &[&Value],
    root: &Value,
    old: Option<&Value>,
    path: &mut Vec<String>,
    found: &mut AttributePaths,
    depth: usize,
) {
    if depth > 64 || schemas.is_empty() {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let segment = SchemaPathSegment::Key(key.clone());
                let child_schemas: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                let old_child = old.and_then(|old| old.get(key));
                path.push(key.clone());
                let scalar = matches!(child, Value::String(_) | Value::Number(_) | Value::Bool(_));
                if scalar && old_child.is_none() && hinted_attribute(&child_schemas, root) {
                    found.insert(path.clone());
                } else {
                    walk(child, &child_schemas, root, old_child, path, found, depth + 1);
                }
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let segment = SchemaPathSegment::ArrayItem(index);
                let item_schemas: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                let old_item = old.and_then(|old| match old {
                    Value::Array(items) => items.get(index),
                    // A lone element read where the list now is.
                    other if index == 0 => Some(other),
                    _ => None,
                });
                walk(item, &item_schemas, root, old_item, path, found, depth + 1);
            }
        }
        _ => {}
    }
}

/// Whether the element at `path` has a property the schema marks an
/// attribute.
pub(super) fn holder_has_attributes(root: &Value, path: &[SchemaPathSegment]) -> bool {
    schema_at_path(root, path, true)
        .into_iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .filter_map(|branch| branch.get("properties").and_then(Value::as_object))
        .flat_map(|properties| properties.values())
        .any(|property| hinted_attribute(&[property], root))
}
