//! Deciding an `if` for the value at the caret, so completion and hover
//! follow the `then` or `else` that validation applies.
//!
//! The decision is cheap and never wrong, not a validation: the keywords a
//! condition is written with (`const`, `enum`, `type`, `required`,
//! `properties`, `pattern`, bounds, the combinators) are checked, and a
//! condition using anything else is undecided (both branches are offered).

use super::*;

/// Whether `value` matches `schema`: `None` when that cannot be told
/// cheaply (a keyword left unread, a reference cycle).
pub(crate) fn decides(schema: &Value, value: &Value, root: &Value) -> Option<bool> {
    decide(schema, value, root, 0)
}

/// Keywords that never fail a value: annotations and definitions.
const ANNOTATIONS: [&str; 16] = [
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$comment",
    "$defs",
    "definitions",
    "title",
    "description",
    "default",
    "examples",
    "example",
    "deprecated",
    "readOnly",
    "writeOnly",
    "format",
];

/// `Some(false)` if any is false, else `None` if any is unknown, else true.
fn all(results: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
    let mut known = true;
    for result in results {
        match result {
            Some(false) => return Some(false),
            None => known = false,
            Some(true) => {}
        }
    }
    known.then_some(true)
}

fn decide(schema: &Value, value: &Value, root: &Value, depth: usize) -> Option<bool> {
    if depth > 32 {
        return None;
    }
    let members = match schema {
        Value::Bool(allowed) => return Some(*allowed),
        Value::Object(members) => members,
        _ => return None,
    };
    let sub = |schema: &Value| decide(schema, value, root, depth + 1);
    all(members.iter().map(|(keyword, argument)| {
        let keyword = keyword.as_str();
        if ANNOTATIONS.contains(&keyword) || keyword.starts_with("x-") {
            return Some(true);
        }
        match keyword {
            "$ref" => argument
                .as_str()
                .and_then(|reference| ref_target(reference, root))
                .and_then(sub),
            "const" => Some(argument == value),
            "enum" => argument.as_array().map(|values| values.contains(value)),
            "type" => Some(match argument {
                Value::String(name) => is_of_type(value, name),
                Value::Array(names) => names
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|name| is_of_type(value, name)),
                _ => return None,
            }),
            "required" => Some(match value {
                Value::Object(present) => argument
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .all(|key| present.contains_key(key)),
                _ => true,
            }),
            "properties" => match value {
                Value::Object(present) => {
                    all(argument.as_object()?.iter().filter_map(|(key, property)| {
                        present
                            .get(key)
                            .map(|child| decide(property, child, root, depth + 1))
                    }))
                }
                _ => Some(true),
            },
            "pattern" => match value {
                Value::String(text) => patterns::pattern_decides(argument.as_str()?, text),
                _ => Some(true),
            },
            "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                let (Some(number), Some(bound)) = (value.as_f64(), argument.as_f64()) else {
                    return (!value.is_number()).then_some(true);
                };
                Some(match keyword {
                    "minimum" => number >= bound,
                    "maximum" => number <= bound,
                    "exclusiveMinimum" => number > bound,
                    _ => number < bound,
                })
            }
            "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
            | "maxProperties" => {
                let bound = argument.as_u64()? as usize;
                let size = match (keyword, value) {
                    ("minLength" | "maxLength", Value::String(text)) => text.chars().count(),
                    ("minItems" | "maxItems", Value::Array(items)) => items.len(),
                    ("minProperties" | "maxProperties", Value::Object(present)) => present.len(),
                    _ => return Some(true),
                };
                Some(if keyword.starts_with("min") {
                    size >= bound
                } else {
                    size <= bound
                })
            }
            "allOf" => all(argument.as_array()?.iter().map(sub)),
            "anyOf" => {
                let results: Vec<Option<bool>> = argument.as_array()?.iter().map(sub).collect();
                if results.contains(&Some(true)) {
                    Some(true)
                } else if results.contains(&None) {
                    None
                } else {
                    Some(false)
                }
            }
            "oneOf" => {
                let results: Vec<Option<bool>> = argument.as_array()?.iter().map(sub).collect();
                if results.contains(&None) {
                    return None;
                }
                Some(
                    results
                        .iter()
                        .filter(|result| **result == Some(true))
                        .count()
                        == 1,
                )
            }
            "not" => sub(argument).map(|matched| !matched),
            _ => None,
        }
    }))
}

fn is_of_type(value: &Value, name: &str) -> bool {
    match name {
        "integer" => value
            .as_f64()
            .is_some_and(|number| number.fract() == 0.0 && number.is_finite()),
        "number" => value.is_number(),
        other => json_type_name(value) == other,
    }
}

/// Whether some schema in `schema` is conditional (`if`, or a dependent
/// schema): only then is the document read to decide them.
pub(crate) fn mentions_conditionals(schema: &Value) -> bool {
    match schema {
        Value::Object(members) => members.iter().any(|(keyword, member)| {
            matches!(keyword.as_str(), "if" | "dependentSchemas")
                || (keyword == "dependencies" && member.is_object())
                || (!matches!(keyword.as_str(), "const" | "enum" | "default" | "examples")
                    && mentions_conditionals(member))
        }),
        Value::Array(items) => items.iter().any(mentions_conditionals),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conditions_are_decided_only_when_their_keywords_tell() {
        let root = json!({"$defs": {"a": {"const": "a"}}});
        let kind_is_a =
            json!({"properties": {"kind": {"$ref": "#/$defs/a"}}, "required": ["kind"]});
        assert_eq!(
            decides(&kind_is_a, &json!({"kind": "a"}), &root),
            Some(true)
        );
        assert_eq!(
            decides(&kind_is_a, &json!({"kind": "b"}), &root),
            Some(false)
        );
        assert_eq!(decides(&kind_is_a, &json!({}), &root), Some(false));
        assert_eq!(
            decides(
                &json!({"properties": {"n": {"minimum": 3}}}),
                &json!({"n": 2}),
                &root
            ),
            Some(false)
        );
        assert_eq!(
            decides(&json!({"type": "integer"}), &json!(2.0), &root),
            Some(true)
        );
        assert_eq!(
            decides(&json!({"not": {"enum": [1, 2]}}), &json!(3), &root),
            Some(true)
        );
        assert_eq!(
            decides(&json!({"uniqueItems": true}), &json!([1]), &root),
            None,
            "a keyword left unread decides nothing"
        );
        assert_eq!(
            decides(&json!({"const": 1, "uniqueItems": true}), &json!(2), &root),
            Some(false),
            "a failed keyword decides"
        );
        let cycle = json!({"$defs": {"c": {"$ref": "#/$defs/c"}}});
        assert_eq!(
            decides(&json!({"$ref": "#/$defs/c"}), &json!(1), &cycle),
            None
        );
    }
}
