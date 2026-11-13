//! What a `$ref` the editor cannot follow is told: the place of the `$ref`
//! in the schema and why, not the resolver's internals (a registry, a base
//! URI, the platform the editor is built for).
//!
//! The editor resolves references inside the schema only: a `$ref` to
//! another file or a URL is refused when the schema is compiled, used or not
//! (the validator collects every resource up front, and the browser fetches
//! nothing for it). A document it needs goes in the schema itself, under
//! `$defs`, with its `$id`.

use jsonschema::ReferencingError;
use jsonschema::error::ValidationErrorKind;
use serde_json::Value;

/// The message for an error compiling the schema `schema`, when it is a
/// `$ref` the editor cannot follow.
pub(super) fn message(kind: &ValidationErrorKind, schema: &Value) -> Option<String> {
    let ValidationErrorKind::Referencing(error) = kind else {
        return None;
    };
    Some(match error {
        ReferencingError::Unretrievable { uri, .. } => {
            let at = site(schema, |reference| {
                uri.ends_with(reference) || reference.starts_with(uri.as_str())
            })
            .map(|at| format!(" at {at}"))
            .unwrap_or_default();
            format!(
                "the $ref{at} names \"{uri}\", outside this schema: the editor follows references inside the schema only. Put that schema under $defs with its $id, or inline it"
            )
        }
        ReferencingError::PointerToNowhere { pointer } => {
            let at = site(schema, |reference| reference.ends_with(pointer.as_str()))
                .map(|at| format!(" at {at}"))
                .unwrap_or_default();
            format!("the $ref{at} points to \"#{pointer}\", which is not in the schema")
        }
        ReferencingError::NoSuchAnchor { anchor } => {
            let at = site(schema, |reference| {
                reference.ends_with(&format!("#{anchor}"))
            })
            .map(|at| format!(" at {at}"))
            .unwrap_or_default();
            format!(
                "the $ref{at} names the anchor \"{anchor}\", which no $anchor in the schema declares"
            )
        }
        other => format!("a $ref cannot be followed: {other}"),
    })
}

/// The JSON pointer of the first `$ref` in `schema` whose text `matches`.
fn site(schema: &Value, matches: impl Fn(&str) -> bool + Copy) -> Option<String> {
    fn walk(
        value: &Value,
        path: &mut Vec<String>,
        matches: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        match value {
            Value::Object(members) => {
                if let Some(reference) = members.get("$ref").and_then(Value::as_str)
                    && matches(reference)
                {
                    path.push("$ref".to_string());
                    let found = pointer(path);
                    path.pop();
                    return Some(found);
                }
                for (key, member) in members {
                    path.push(key.clone());
                    let found = walk(member, path, matches);
                    path.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
                path.push(index.to_string());
                let found = walk(item, path, matches);
                path.pop();
                found
            }),
            _ => None,
        }
    }
    fn pointer(path: &[String]) -> String {
        path.iter()
            .map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1")))
            .collect()
    }
    walk(schema, &mut Vec::new(), &matches)
}

#[cfg(test)]
mod tests {
    use crate::editor::diagnostics::Schema;

    fn refused(schema: &str) -> String {
        Schema::compile(schema).expect_err("refused").0
    }

    /// S.refs-2/3/4/7: the `$ref`'s place and the real cause, no resolver
    /// internals, for an external reference used or not, a missing pointer
    /// and a missing anchor.
    #[test]
    fn a_ref_the_editor_cannot_follow_names_its_place_and_why() {
        let absolute = refused(r#"{"properties":{"a":{"$ref":"https://example.com/s.json"}}}"#);
        assert!(absolute.contains("at /properties/a/$ref"), "{absolute}");
        assert!(
            absolute.contains("references inside the schema only"),
            "{absolute}"
        );
        assert!(!absolute.contains("wasm32"), "{absolute}");
        let relative = refused(r##"{"properties":{"a":{"$ref":"other.json#/$defs/x"}}}"##);
        assert!(relative.contains("at /properties/a/$ref"), "{relative}");
        assert!(!relative.contains("base URI"), "{relative}");
        let unused = refused(
            r#"{"$defs":{"extra":{"$ref":"https://example.com/missing.json"}},"type":"object"}"#,
        );
        assert!(unused.contains("at /$defs/extra/$ref"), "{unused}");
        let missing = refused(r##"{"properties":{"a":{"$ref":"#/$defs/nope"}}}"##);
        assert!(missing.contains("at /properties/a/$ref"), "{missing}");
        assert!(missing.contains("not in the schema"), "{missing}");
    }

    /// The README's way out: the other schema under `$defs` with its `$id`.
    #[cfg(feature = "yaml")]
    #[test]
    fn a_schema_embedded_with_its_id_is_followed() {
        use crate::editor::languages::Language;
        let schema = Schema::compile(
            r#"{"$defs":{"ext":{"$id":"https://example.com/s.json","type":"integer"}},"properties":{"a":{"$ref":"https://example.com/s.json"}}}"#,
        )
        .expect("compiles");
        assert_eq!(schema.validate(Language::YAML, "a: 1\n"), Vec::new());
        assert_eq!(schema.validate(Language::YAML, "a: x\n").len(), 1);
    }
}
