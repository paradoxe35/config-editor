//! What the schema marks on a document's fields: `readOnly` (with
//! `x-readonly-reason`) and secrets (`writeOnly`, `x-secret`).

use serde_json::Value;

use super::super::completion::{child_schemas, resolve_branches, schema_at_path};
use super::super::language::SchemaPathSegment;

/// The fields of one document the schema marks, by JSON pointer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SchemaMarks {
    /// Read-only fields, each with its reason when the schema gives one. A
    /// field inside one is not listed again.
    pub(crate) read_only: Vec<(String, Option<String>)>,
    /// Secret fields.
    pub(crate) secret: Vec<String>,
}

/// Whether `schema` mentions any of the marks at all, so documents under a
/// schema without them skip the walk.
pub(crate) fn mentions_marks(schema: &Value) -> bool {
    match schema {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (matches!(key.as_str(), "readOnly" | "writeOnly" | "x-secret")
                && value == &Value::Bool(true))
                || mentions_marks(value)
        }),
        Value::Array(items) => items.iter().any(mentions_marks),
        _ => false,
    }
}

/// The keys of the fields `root` marks read-only, wherever they are (a
/// field whose items or entries are read-only by its own key), to find the
/// lines that may hold one in a text too large to read every line of.
pub(crate) fn read_only_names(root: &Value) -> Vec<String> {
    let mut names = Vec::new();
    collect_names(root, root, &mut names, 0);
    names
}

fn collect_names(schema: &Value, root: &Value, names: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    match schema {
        Value::Object(map) => {
            if let Some(Value::Object(properties)) = map.get("properties") {
                for (key, child) in properties {
                    if holds_read_only(child, root, 0) && !names.contains(key) {
                        names.push(key.clone());
                    }
                }
            }
            for value in map.values() {
                collect_names(value, root, names, depth + 1);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_names(item, root, names, depth + 1);
            }
        }
        _ => {}
    }
}

/// Whether the field `schema` describes is read-only, or its items or
/// entries are.
fn holds_read_only(schema: &Value, root: &Value, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    if read_only_reason(&[schema], root).is_some() {
        return true;
    }
    resolve_branches(schema, root, 0).iter().any(|branch| {
        ["items", "additionalProperties", "contains"]
            .iter()
            .filter_map(|key| branch.get(*key))
            .chain(
                ["prefixItems", "patternProperties"]
                    .iter()
                    .filter_map(|key| branch.get(*key))
                    .flat_map(|inner| match inner {
                        Value::Array(items) => items.iter().collect::<Vec<_>>(),
                        Value::Object(map) => map.values().collect(),
                        _ => Vec::new(),
                    }),
            )
            .any(|inner| holds_read_only(inner, root, depth + 1))
    })
}

/// The read-only reason the branches of a field's schemas give: `Some` when
/// one says `readOnly: true` (the inner `Option` is its
/// `x-readonly-reason`).
pub(crate) fn read_only_reason(schemas: &[&Value], root: &Value) -> Option<Option<String>> {
    let branches: Vec<&Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .collect();
    branches
        .iter()
        .any(|branch| branch.get("readOnly") == Some(&Value::Bool(true)))
        .then(|| {
            branches.iter().find_map(|branch| {
                branch
                    .get("x-readonly-reason")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        })
}

/// Whether the branches of a field's schemas make it a secret.
pub(crate) fn is_secret(schemas: &[&Value], root: &Value) -> bool {
    schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| {
            branch.get("writeOnly") == Some(&Value::Bool(true))
                || branch.get("x-secret") == Some(&Value::Bool(true))
        })
}

/// The schema's verdict for the field at `path`, written or not: read-only
/// (with the reason), and whether it is a secret. A field inside a
/// read-only one is read-only with that one's reason.
pub(crate) fn marks_at(
    root: &Value,
    path: &[SchemaPathSegment],
    repeats_arrays: bool,
) -> (Option<Option<String>>, bool) {
    for depth in 0..=path.len() {
        let schemas = schema_at_path(root, &path[..depth], repeats_arrays);
        if let Some(reason) = read_only_reason(&schemas, root) {
            return (Some(reason), false);
        }
        if depth == path.len() {
            return (None, is_secret(&schemas, root));
        }
    }
    (None, false)
}

/// The marked fields of `document`, walking it with the schema.
pub(crate) fn schema_marks(document: &Value, root: &Value) -> SchemaMarks {
    let mut marks = SchemaMarks::default();
    walk(document, vec![root], root, String::new(), &mut marks, 0);
    marks
}

/// Fields walked at most, so a huge document cannot stall an edit.
const MAX_DEPTH: usize = 64;

fn walk(
    value: &Value,
    schemas: Vec<&Value>,
    root: &Value,
    pointer: String,
    marks: &mut SchemaMarks,
    depth: usize,
) {
    if depth > MAX_DEPTH || schemas.is_empty() {
        return;
    }
    if let Some(reason) = read_only_reason(&schemas, root) {
        marks.read_only.push((pointer, reason));
        return;
    }
    if is_secret(&schemas, root) {
        marks.secret.push(pointer.clone());
    }
    let children: Vec<(String, &Value, SchemaPathSegment)> = match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, child)| (escape(key), child, SchemaPathSegment::Key(key.clone())))
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, child)| {
                (
                    index.to_string(),
                    child,
                    SchemaPathSegment::ArrayItem(index),
                )
            })
            .collect(),
        _ => return,
    };
    for (step, child, segment) in children {
        let next: Vec<&Value> = schemas
            .iter()
            .flat_map(|schema| child_schemas(schema, root, &segment))
            .collect();
        walk(
            child,
            next,
            root,
            format!("{pointer}/{step}"),
            marks,
            depth + 1,
        );
    }
}

/// A key as a JSON pointer segment.
pub(crate) fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn read_only_and_secret_fields_are_found_through_refs_and_combinators() {
        let schema = json!({
            "$defs": {"id": {"type": "string", "readOnly": true, "x-readonly-reason": "Set by the server"}},
            "properties": {
                "id": {"$ref": "#/$defs/id"},
                "clients": {"additionalProperties": {"properties": {
                    "secret": {"anyOf": [{"type": "string", "writeOnly": true}]},
                    "token": {"type": "string", "x-secret": true}
                }}},
                "meta": {"type": "object", "readOnly": true, "properties": {"a": {"x-secret": true}}}
            }
        });
        assert!(mentions_marks(&schema));
        let document = json!({
            "id": "x",
            "clients": {"acme": {"secret": "s", "token": "t", "other": 1}},
            "meta": {"a": "1"}
        });
        let marks = schema_marks(&document, &schema);
        assert_eq!(
            marks.read_only,
            vec![
                ("/id".to_string(), Some("Set by the server".to_string())),
                ("/meta".to_string(), None)
            ]
        );
        assert_eq!(
            marks.secret,
            vec![
                "/clients/acme/secret".to_string(),
                "/clients/acme/token".to_string()
            ]
        );
        let path = |pointer: &str| {
            super::super::super::paths::FieldPath::from(pointer)
                .segments()
                .to_vec()
        };
        assert_eq!(
            marks_at(&schema, &path("/meta/a"), false),
            (Some(None), false)
        );
        assert_eq!(
            marks_at(&schema, &path("/clients/x/token"), false),
            (None, true)
        );
        assert!(!mentions_marks(
            &json!({"properties": {"readOnly": {"type": "boolean"}}})
        ));
    }
}
