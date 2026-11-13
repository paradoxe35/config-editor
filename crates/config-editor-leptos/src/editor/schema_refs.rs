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
            // The validator meets the references outside the schema in no
            // fixed order: the first one the schema writes is named, the
            // same each time.
            let ids = embedded_ids(schema);
            let (at, named) = match first_ref(schema, |reference| outside(reference, &ids)) {
                Some((at, reference)) => (Some(at), reference),
                None => (
                    site(schema, |reference| {
                        uri.ends_with(reference) || reference.starts_with(uri.as_str())
                    }),
                    uri.to_string(),
                ),
            };
            let at = at.map(|at| format!(" at {at}")).unwrap_or_default();
            format!(
                "the $ref{at} names \"{named}\", outside this schema: the editor follows references inside the schema only and fetches nothing. Pass that file in the schema's resources, put it under $defs with its $id, or inline it"
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
        ReferencingError::UnknownSpecification { specification } => format!(
            "its $schema \"{specification}\" is not a JSON Schema version the editor knows: it reads drafts 4, 6 and 7, 2019-09 and 2020-12"
        ),
        ReferencingError::CircularMetaschema { uri } => {
            format!("its $schema \"{uri}\" names a meta-schema that names itself in a cycle")
        }
        other => format!("a $ref cannot be followed: {other}"),
    })
}

/// The JSON pointer of the first `$ref` in `schema` whose text `matches`.
fn site(schema: &Value, matches: impl Fn(&str) -> bool + Copy) -> Option<String> {
    first_ref(schema, matches).map(|(at, _)| at)
}

/// The `$id`s the schema embeds (the documents a `$ref` may name by them).
fn embedded_ids(schema: &Value) -> Vec<&str> {
    fn walk<'a>(value: &'a Value, ids: &mut Vec<&'a str>) {
        match value {
            Value::Object(members) => {
                if let Some(id) = members
                    .get("$id")
                    .or_else(|| members.get("id"))
                    .and_then(Value::as_str)
                {
                    let id = id.trim_end_matches('#');
                    if !id.is_empty() && !id.starts_with('#') {
                        ids.push(id);
                    }
                }
                members.values().for_each(|member| walk(member, ids));
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, ids)),
            _ => {}
        }
    }
    let mut ids = Vec::new();
    walk(schema, &mut ids);
    ids
}

/// Whether `reference` names a document the schema does not embed.
fn outside(reference: &str, ids: &[&str]) -> bool {
    let document = reference.split('#').next().unwrap_or_default();
    !document.is_empty()
        && !ids
            .iter()
            .any(|id| id.ends_with(document) || document.ends_with(id))
}

/// The JSON pointer and the text of the first `$ref` in `schema`, in the
/// order it is written, whose text `matches`.
fn first_ref(schema: &Value, matches: impl Fn(&str) -> bool + Copy) -> Option<(String, String)> {
    fn walk(
        value: &Value,
        path: &mut Vec<String>,
        matches: &dyn Fn(&str) -> bool,
    ) -> Option<(String, String)> {
        match value {
            Value::Object(members) => {
                if let Some(reference) = members.get("$ref").and_then(Value::as_str)
                    && matches(reference)
                {
                    path.push("$ref".to_string());
                    let found = pointer(path);
                    path.pop();
                    return Some((found, reference.to_string()));
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

    /// SC30-9: with several references outside the schema, the first one
    /// it writes is named, the same on every compile.
    #[test]
    fn the_first_outside_ref_is_named_every_time() {
        let schema = r#"{"$id":"https://example.com/root.json","properties":{
            "a":{"$ref":"https://example.com/one.json"},
            "b":{"$ref":"two.json"},
            "c":{"$ref":"root.json#/$defs/x"},
            "d":{"$ref":"https://example.com/three.json#/x"},
            "e":{"$ref":"four.json"},
            "f":{"$ref":"https://example.com/five.json"}
        },"$defs":{"x":{}}}"#;
        for _ in 0..20 {
            let message = refused(schema);
            assert!(
                message.contains("at /properties/a/$ref names \"https://example.com/one.json\""),
                "{message}"
            );
        }
    }

    /// SC30-4: a `$schema` naming a version the validator does not know is
    /// told as that, not as a `$ref`.
    #[test]
    fn an_unknown_schema_version_is_named_as_one() {
        for version in [
            "http://json-schema.org/draft-03/schema#",
            "https://example.com/my-meta",
        ] {
            let message = refused(&format!(r#"{{"$schema":"{version}","type":"object"}}"#));
            assert!(
                message.contains(&format!(
                    "$schema \"{version}\" is not a JSON Schema version"
                )),
                "{message}"
            );
            assert!(!message.contains("$ref"), "{message}");
        }
    }
}
