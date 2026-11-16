//! `"$ref": ""`, a reference to the document it is in, read as `"#"`.
//!
//! The empty reference is the schema's own root (RFC 3986: an empty
//! reference resolves to the base document), and `"#"` says the same. The
//! validator left it unresolved, so a schema reached through it checked
//! nothing, and completion and hover, which follow `#` pointers, saw nothing
//! there either: one spelling for both.

use serde_json::Value;

use super::subschemas::each_schema_mut;

/// Rewrites each `"$ref": ""` in the schema (or list of schemas) `value` to
/// `"$ref": "#"`.
pub(super) fn to_root(value: &mut Value) {
    each_schema_mut(value, &mut |members| {
        if let Some(reference) = members.get_mut("$ref")
            && reference.as_str() == Some("")
        {
            *reference = Value::String("#".to_string());
        }
    });
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
        use crate::editor::languages::Language;
        use crate::editor::schema::Schema;
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
