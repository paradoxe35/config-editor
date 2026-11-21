//! Which keywords of a schema hold schemas and which hold data, and the
//! one walk over every schema written inside a schema document. Every pass
//! over the schema text (the rewrites when it is compiled, the facts read
//! from it, the explaining validator's wrappers) reads keywords through
//! [`holds`], so none reads a property named `enum` as data or an
//! `examples` value as a schema.

use serde_json::{Map, Value};

/// What the value of a schema keyword holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holds {
    /// Values, never read as schemas (an example may look like one).
    Data,
    /// Names mapped to schemas (a property may be named like a keyword).
    /// Draft-07 `dependencies` maps a name to a schema or to a list of
    /// names: the lists hold no schema.
    Named,
    /// A schema, or a list of schemas: the applicators, and every keyword
    /// the editor does not know (a `$ref` may point into one, as into
    /// OpenAPI's `components`).
    Schemas,
}

/// What the value of `keyword`, in a schema object, holds.
pub(crate) fn holds(keyword: &str) -> Holds {
    match keyword {
        // `example` is OpenAPI's single example; `xml`, `discriminator` and
        // `externalDocs` are OpenAPI's notes on a schema (how XML writes it,
        // which property names its option, where it is documented).
        "const" | "enum" | "default" | "examples" | "example" | "xml" | "discriminator"
        | "externalDocs" => Holds::Data,
        "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas"
        | "dependencies" => Holds::Named,
        _ => Holds::Schemas,
    }
}

/// The schema values `members` (one schema object) holds, each with the
/// pointer tokens from `members` to it (a keyword, then a name for a
/// [`Holds::Named`] one). A list of schemas is one value here: [`each`]
/// reads its items.
fn held<'v>(
    members: &'v Map<String, Value>,
) -> impl Iterator<Item = (&'v str, Option<&'v str>, &'v Value)> {
    members
        .iter()
        .flat_map(|(keyword, member)| -> Box<dyn Iterator<Item = _> + 'v> {
            match (holds(keyword), member) {
                (Holds::Data, _) => Box::new(std::iter::empty()),
                (Holds::Named, Value::Object(named)) => Box::new(
                    named
                        .iter()
                        .map(move |(name, schema)| (keyword.as_str(), Some(name.as_str()), schema)),
                ),
                (Holds::Named, _) => Box::new(std::iter::empty()),
                (Holds::Schemas, member) => {
                    Box::new(std::iter::once((keyword.as_str(), None, member)))
                }
            }
        })
}

/// A JSON pointer token for `key` (`~0`, `~1`).
pub(crate) fn escaped(key: &str) -> String {
    crate::editor::language::pointer_key(key)
}

/// A visit of one schema object by [`each_schema`]: its JSON pointer, its
/// members and the state its parent's visit gave; it returns the state for
/// the schemas inside it, or `None` to skip them.
pub(crate) type Visit<'v, 'f, S> = dyn FnMut(&str, &'v Map<String, Value>, &S) -> Option<S> + 'f;

/// Visits each schema object of `document` (a schema, or a list of them),
/// parents first, in written order, with its JSON pointer and the state
/// its parent's visit gave (`state` for the document); a visit returns the
/// state for the schemas inside it, or `None` to skip them. Data is never
/// visited.
pub(crate) fn each_schema<'v, S>(document: &'v Value, state: &S, visit: &mut Visit<'v, '_, S>) {
    each(document, &mut String::new(), state, visit);
}

fn each<'v, S>(value: &'v Value, pointer: &mut String, state: &S, visit: &mut Visit<'v, '_, S>) {
    let length = pointer.len();
    match value {
        Value::Object(members) => {
            let Some(inner) = visit(pointer, members, state) else {
                return;
            };
            for (keyword, name, schema) in held(members) {
                pointer.push('/');
                pointer.push_str(&escaped(keyword));
                if let Some(name) = name {
                    pointer.push('/');
                    pointer.push_str(&escaped(name));
                }
                each(schema, pointer, &inner, visit);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                each(item, pointer, state, visit);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}

/// Visits each schema object of `document` as [`each_schema`] does (no
/// pointer), to rewrite it; what a visit adds is walked too.
pub(crate) fn each_schema_mut(
    document: &mut Value,
    visit: &mut dyn FnMut(&mut Map<String, Value>),
) {
    match document {
        Value::Object(members) => {
            visit(members);
            for (keyword, member) in members.iter_mut() {
                match (holds(keyword), member) {
                    (Holds::Data, _) => {}
                    (Holds::Named, Value::Object(named)) => {
                        named
                            .values_mut()
                            .for_each(|schema| each_schema_mut(schema, visit));
                    }
                    (Holds::Named, _) => {}
                    (Holds::Schemas, member) => each_schema_mut(member, visit),
                }
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| each_schema_mut(item, visit)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every keyword name, used as a property name, a definition name and a
    /// dependency name, is a schema there: no walk reads it as a keyword.
    #[test]
    fn keyword_names_used_as_names_are_schemas_and_data_is_never_walked() {
        let keywords = [
            "const",
            "enum",
            "default",
            "examples",
            "example",
            "properties",
            "patternProperties",
            "$defs",
            "definitions",
            "dependentSchemas",
            "dependencies",
            "items",
            "oneOf",
            "$ref",
        ];
        for keyword in keywords {
            for holder in [
                "properties",
                "patternProperties",
                "$defs",
                "definitions",
                "dependentSchemas",
                "dependencies",
            ] {
                let schema = json!({ holder: { keyword: {"title": "inside"} } });
                let mut pointers = Vec::new();
                each_schema(&schema, &(), &mut |pointer, members, ()| {
                    if members.get("title").is_some() {
                        pointers.push(pointer.to_string());
                    }
                    Some(())
                });
                assert_eq!(
                    pointers,
                    vec![format!("/{holder}/{}", escaped(keyword))],
                    "{holder}/{keyword}"
                );
            }
        }
        for data in [
            "const",
            "enum",
            "default",
            "examples",
            "example",
            "xml",
            "discriminator",
            "externalDocs",
        ] {
            let schema = json!({ data: {"title": "data", "properties": {"a": {}}} });
            let mut visited = 0;
            each_schema(&schema, &(), &mut |_, _, ()| {
                visited += 1;
                Some(())
            });
            assert_eq!(visited, 1, "{data}");
        }
    }

    #[test]
    fn lists_and_unknown_keywords_hold_schemas_with_their_pointers() {
        let schema = json!({
            "allOf": [{"title": "a"}, {"title": "b"}],
            "components": {"schemas": {"Pet": {"title": "c"}}},
            "dependencies": {"x": ["y"], "z": {"title": "d"}},
        });
        let mut found = Vec::new();
        each_schema(&schema, &(), &mut |pointer, members, ()| {
            if let Some(title) = members.get("title").and_then(Value::as_str) {
                found.push(format!("{pointer}={title}"));
            }
            Some(())
        });
        assert_eq!(
            found,
            [
                "/allOf/0=a",
                "/allOf/1=b",
                "/components/schemas/Pet=c",
                "/dependencies/z=d"
            ]
        );
    }
}
