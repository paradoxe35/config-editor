//! `"$ref": ""`, a reference to the document it is in, read as `"#"`.
//!
//! The empty reference is the schema's own root (RFC 3986: an empty
//! reference resolves to the base document), and `"#"` says the same. The
//! validator left it unresolved, so a schema reached through it checked
//! nothing, and completion and hover, which follow `#` pointers, saw nothing
//! there either: one spelling for both.

use serde_json::Value;

/// Keywords whose value is data, not schemas: left as written.
const DATA: [&str; 4] = ["const", "enum", "default", "examples"];

/// Keywords whose value maps names to schemas.
const NAMED: [&str; 5] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
];

/// Rewrites each `"$ref": ""` in the schema (or list of schemas) `value` to
/// `"$ref": "#"`.
pub(super) fn to_root(value: &mut Value) {
    match value {
        Value::Object(members) => {
            for (key, member) in members.iter_mut() {
                let key = key.as_str();
                if key == "$ref" && member.as_str() == Some("") {
                    *member = Value::String("#".to_string());
                } else if NAMED.contains(&key) {
                    if let Value::Object(named) = member {
                        named.values_mut().for_each(to_root);
                    }
                } else if !DATA.contains(&key) {
                    to_root(member);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(to_root),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::to_root;

    #[test]
    fn an_empty_ref_is_the_root_and_data_and_property_names_stay() {
        let mut schema = json!({
            "type": "object",
            "properties": {"$ref": {"$ref": ""}, "n": {"items": [{"$ref": ""}]}},
            "$defs": {"a": {"$ref": ""}},
            "default": {"$ref": ""},
            "enum": [{"$ref": ""}],
        });
        to_root(&mut schema);
        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {"$ref": {"$ref": "#"}, "n": {"items": [{"$ref": "#"}]}},
                "$defs": {"a": {"$ref": "#"}},
                "default": {"$ref": ""},
                "enum": [{"$ref": ""}],
            })
        );
    }

    /// S.refs-1: a value under an empty reference is checked against the
    /// root, as under `"#"`.
    #[cfg(all(feature = "schema-validation", feature = "yaml"))]
    #[test]
    fn a_value_under_an_empty_ref_is_checked_against_the_root() {
        use crate::editor::diagnostics::Schema;
        use crate::editor::languages::Language;
        for reference in ["", "#"] {
            let schema = Schema::compile(
                &json!({
                    "type": "object",
                    "required": ["name"],
                    "properties": {"name": {"type": "string"}, "child": {"$ref": reference}},
                })
                .to_string(),
            )
            .expect("a schema");
            let found = |text: &str| {
                schema
                    .validate(Language::YAML, text)
                    .into_iter()
                    .map(|problem| problem.message)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                found("name: a\nchild:\n  name: b\n"),
                Vec::<String>::new(),
                "{reference:?}"
            );
            assert_eq!(
                found("name: a\nchild: 1\n").len(),
                1,
                "{reference:?}: {:?}",
                found("name: a\nchild: 1\n")
            );
            assert_eq!(
                found("name: a\nchild: {}\n").len(),
                1,
                "{reference:?}: {:?}",
                found("name: a\nchild: {}\n")
            );
        }
    }
}
