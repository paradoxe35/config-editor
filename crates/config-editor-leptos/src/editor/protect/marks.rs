//! What the schema marks on a document's fields: `readOnly` (with
//! `x-readonly-reason`) and secrets (`writeOnly`, `x-secret`).

use serde_json::Value;

use super::super::applies::{Applies, Entry, Here, Next, Seen, Walking, each_value};
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
    // Whether what applies to a field (blind, as locks read it), or to its
    // items or entries at any depth, says `readOnly`; each schema is read
    // once, so a reference cycle ends.
    let holds_read_only = |field: &Value| {
        let mut read = std::collections::HashSet::new();
        let mut next = vec![Entry::schema(field)];
        while let Some(entry) = next.pop() {
            let here = Here::walk(entry, root, Seen::default());
            let branches = here.branches();
            if read_only_in(&branches).is_some() {
                return true;
            }
            if branches.iter().fold(false, |new, branch| {
                read.insert(std::ptr::from_ref(*branch)) | new
            }) {
                next.push(here.unnamed_children());
            }
        }
        false
    };
    let mut names: Vec<String> = Vec::new();
    crate::editor::schema::each_schema(root, &(), &mut |_, members, ()| {
        if let Some(Value::Object(properties)) = members.get("properties") {
            for (key, field) in properties {
                if !names.contains(key) && holds_read_only(field) {
                    names.push(key.clone());
                }
            }
        }
        Some(())
    });
    names
}

/// The read-only reason the schemas applying to a field (`branches`)
/// give: `Some` when one says `readOnly: true` (the inner `Option` is its
/// `x-readonly-reason`).
pub(crate) fn read_only_in(branches: &[&Value]) -> Option<Option<String>> {
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

/// Whether the schemas applying to a field (`branches`) make it a secret.
fn secret_in(branches: &[&Value]) -> bool {
    branches.iter().any(|branch| {
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
    let applies = Applies::blind(root, repeats_arrays);
    for depth in 0..=path.len() {
        let branches = applies.at(&path[..depth]).branches();
        if let Some(reason) = read_only_in(&branches) {
            return (Some(reason), false);
        }
        if depth == path.len() {
            return (None, secret_in(&branches));
        }
    }
    (None, false)
}

/// The marked fields of `document`, walking it with the schema: what the
/// schema may say at each field whatever the document decides, so a lock
/// never depends on what is written beside it.
pub(crate) fn schema_marks(document: &Value, root: &Value) -> SchemaMarks {
    let mut marks = SchemaMarks::default();
    each_value(document, root, Walking::default(), &mut |visit| {
        let branches = visit.here.branches();
        if let Some(reason) = read_only_in(&branches) {
            marks.read_only.push((visit.pointer.to_string(), reason));
            return Next::Past;
        }
        if secret_in(&branches) {
            marks.secret.push(visit.pointer.to_string());
        }
        Next::Into
    });
    marks
}

/// The secret fields of `document` a message must not quote: every field
/// a schema that may apply there marks secret (inside a read-only field
/// too), a key naming a list also read as its item where the language
/// writes lists by repeating keys (`repeats_arrays`).
#[cfg(feature = "schema-validation")]
pub(crate) fn secret_fields(document: &Value, root: &Value, repeats_arrays: bool) -> Vec<String> {
    let mut secrets = Vec::new();
    let walking = Walking {
        decided: false,
        repeats_arrays,
    };
    each_value(document, root, walking, &mut |visit| {
        if secret_in(&visit.here.branches()) {
            secrets.push(visit.pointer.to_string());
        }
        Next::Into
    });
    secrets
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

    /// The keys that may name a read-only field: the field's own, by what
    /// applies to it, its items' or its entries' at any depth; never a name
    /// inside data, nor a list read-only only by `contains`.
    #[test]
    fn read_only_names_follow_what_applies_to_each_field() {
        let schema = json!({
            "$defs": {"node": {"type": "array", "items": {"$ref": "#/$defs/node"}},
                      "locked": {"readOnly": true}},
            "properties": {
                "id": {"allOf": [{"$ref": "#/$defs/locked"}]},
                "tokens": {"items": {"items": {"readOnly": true}}},
                "labels": {"patternProperties": {"^x": {"readOnly": true}}},
                "tree": {"$ref": "#/$defs/node"},
                "some": {"contains": {"readOnly": true}},
                "free": {"type": "string"}
            },
            "default": {"properties": {"inData": {"readOnly": true}}}
        });
        assert_eq!(read_only_names(&schema), ["id", "tokens", "labels"]);
    }
}
