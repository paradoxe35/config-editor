//! Whether a value is accepted: deciding an `if` for the value at the
//! caret (so completion and hover follow the `then` or `else` validation
//! applies), reading XML text by the types allowed, matching options.
//!
//! The decision is cheap and never wrong, not a validation: the keywords a
//! condition is written with (`const`, `enum`, `type`, `required`,
//! `properties`, `patternProperties`, `additionalProperties`, dependent
//! schemas, items, `pattern`, bounds, the combinators) are checked, and a
//! condition using anything else is undecided (both branches are offered).

use super::node::{Entry, Node};
use super::{item_schema, named_properties, ref_target, refs_stand_alone};
use crate::editor::completion::{json_type_name, pattern_decides};
use serde_json::Value;

impl Entry<'_> {
    /// Whether every schema here allows `value` (`None` when one cannot
    /// tell, see [`decides`]): each of an `All`, one of an `Any`.
    pub(crate) fn accepts(&self, value: &Value, root: &Value) -> Option<bool> {
        accepted(&self.0, value, root)
    }
}

fn accepted(node: &Node<'_>, value: &Value, root: &Value) -> Option<bool> {
    match node {
        Node::Leaf(schema) => decides(schema, value, root),
        Node::All(parts) => all(parts.iter().map(|part| accepted(part, value, root))),
        Node::Any(parts) | Node::Either(parts) => {
            let mut known = true;
            for part in parts {
                match accepted(part, value, root) {
                    Some(true) => return Some(true),
                    None => known = false,
                    Some(false) => {}
                }
            }
            // No reading at all allows anything.
            (known || parts.is_empty()).then_some(parts.is_empty())
        }
        Node::Options(_) | Node::Cut => None,
    }
}

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
    // Up to draft-07 the keywords beside a `$ref` are ignored.
    let alone = members.contains_key("$ref") && refs_stand_alone(root);
    all(members
        .iter()
        .filter(|(keyword, _)| !alone || *keyword == "$ref")
        .map(|(keyword, argument)| {
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
                "patternProperties" => match value {
                    Value::Object(present) => {
                        let patterns = argument.as_object()?;
                        all(present.iter().flat_map(|(key, child)| {
                            patterns.iter().map(|(pattern, property)| {
                                match pattern_decides(pattern, key)? {
                                    true => decide(property, child, root, depth + 1),
                                    false => Some(true),
                                }
                            })
                        }))
                    }
                    _ => Some(true),
                },
                // Keys that neither `properties` nor a pattern names (unknown
                // while a pattern cannot be read).
                "additionalProperties" => match value {
                    Value::Object(present) => {
                        let patterns = members.get("patternProperties").and_then(Value::as_object);
                        if patterns.is_some_and(|patterns| {
                            patterns
                                .keys()
                                .any(|pattern| pattern_decides(pattern, "").is_none())
                        }) {
                            return None;
                        }
                        all(present
                            .iter()
                            .filter(|(key, _)| named_properties(schema, key).is_empty())
                            .map(|(_, child)| decide(argument, child, root, depth + 1)))
                    }
                    _ => Some(true),
                },
                "dependentSchemas" | "dependencies" => match value {
                    Value::Object(present) => all(argument
                        .as_object()?
                        .iter()
                        .filter(|(key, _)| present.contains_key(*key))
                        .map(|(_, dependent)| dependency(dependent, present, &sub))),
                    _ => Some(true),
                },
                // The items, each by the schema its position has (decided
                // once, under the keyword that gives the positions).
                "items" | "prefixItems" | "additionalItems" => match value {
                    Value::Array(items) if keyword == items_keyword(members) => {
                        all(items.iter().enumerate().filter_map(|(index, item)| {
                            item_schema(schema, index).map(|at| decide(at, item, root, depth + 1))
                        }))
                    }
                    _ => Some(true),
                },
                "pattern" => match value {
                    Value::String(text) => pattern_decides(argument.as_str()?, text),
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
                "multipleOf" => {
                    let (Some(number), Some(step)) = (value.as_f64(), argument.as_f64()) else {
                        return (!value.is_number()).then_some(true);
                    };
                    let times = number / step;
                    (step > 0.0 && times.is_finite())
                        .then(|| (times - times.round()).abs() < 1e-9 * times.abs().max(1.0))
                }
                "minLength" | "maxLength" | "minItems" | "maxItems" | "minProperties"
                | "maxProperties" => {
                    let bound = argument.as_u64()? as usize;
                    let size = match (keyword, value) {
                        ("minLength" | "maxLength", Value::String(text)) => text.chars().count(),
                        ("minItems" | "maxItems", Value::Array(items)) => items.len(),
                        ("minProperties" | "maxProperties", Value::Object(present)) => {
                            present.len()
                        }
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
                // A conditional is its `then` where its `if` holds, else its
                // `else` (each matching anything when absent).
                "if" => {
                    let branch = members.get(if sub(argument)? { "then" } else { "else" });
                    branch.map_or(Some(true), sub)
                }
                "then" | "else" => Some(true),
                _ => None,
            }
        }))
}

/// Whether an object holding a key meets what the key's dependency asks:
/// a schema (`dependentSchemas`), or draft-07's list of keys it requires.
fn dependency(
    dependent: &Value,
    present: &serde_json::Map<String, Value>,
    sub: &dyn Fn(&Value) -> Option<bool>,
) -> Option<bool> {
    match dependent {
        Value::Array(keys) => Some(
            keys.iter()
                .filter_map(Value::as_str)
                .all(|key| present.contains_key(key)),
        ),
        dependent => sub(dependent),
    }
}

/// The keyword that gives an array's item positions: `prefixItems`, else
/// `items` (`additionalItems` only continues an `items` list).
fn items_keyword(members: &serde_json::Map<String, Value>) -> &'static str {
    if members.contains_key("prefixItems") {
        "prefixItems"
    } else {
        "items"
    }
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

/// Whether some schema in `schema` is decided by the document: a
/// conditional (`if`, a dependent schema), or `oneOf`/`anyOf` options
/// (the one an object is in decides the keys of the objects inside it).
/// Only then is the document read to decide them.
pub(crate) fn mentions_conditionals(schema: &Value) -> bool {
    match schema {
        Value::Object(members) => members.iter().any(|(keyword, member)| {
            matches!(
                keyword.as_str(),
                "if" | "dependentSchemas" | "oneOf" | "anyOf"
            ) || (keyword == "dependencies" && member.is_object())
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
        let thousands = json!({"multipleOf": 1000});
        assert_eq!(decides(&thousands, &json!(3000), &root), Some(true));
        assert_eq!(decides(&thousands, &json!(3), &root), Some(false));
        assert_eq!(
            decides(&json!({"multipleOf": 0.1}), &json!(0.3), &root),
            Some(true)
        );
        let keyed = json!({
            "properties": {"a": {}},
            "patternProperties": {"^n": {"type": "integer"}},
            "additionalProperties": {"type": "string"},
            "dependentSchemas": {"a": {"required": ["b"]}},
            "dependencies": {"c": ["a"]}
        });
        for (object, decided) in [
            (json!({"n1": 1, "s": "x"}), Some(true)),
            (json!({"n1": "x"}), Some(false)),
            (json!({"s": 1}), Some(false)),
            (json!({"a": 1, "b": "x"}), Some(true)),
            (json!({"a": 1}), Some(false)),
            (json!({"c": "x"}), Some(false)),
        ] {
            assert_eq!(decides(&keyed, &object, &root), decided, "{object}");
        }
        let tuple = json!({"prefixItems": [{"type": "string"}], "items": {"type": "integer"}});
        assert_eq!(decides(&tuple, &json!(["a", 1, 2]), &root), Some(true));
        assert_eq!(decides(&tuple, &json!(["a", "b"]), &root), Some(false));
        assert_eq!(decides(&tuple, &json!([1]), &root), Some(false));
        let draft7 = json!({"items": [{"type": "string"}], "additionalItems": {"type": "integer"}});
        assert_eq!(decides(&draft7, &json!(["a", 1]), &root), Some(true));
        assert_eq!(decides(&draft7, &json!(["a", "b"]), &root), Some(false));
        let cycle = json!({"$defs": {"c": {"$ref": "#/$defs/c"}}});
        assert_eq!(
            decides(&json!({"$ref": "#/$defs/c"}), &json!(1), &cycle),
            None
        );
    }
}
