//! Whether a value is accepted: deciding an `if` for the value at the
//! caret (so completion and hover follow the `then` or `else` validation
//! applies), reading XML text by the types allowed, matching options.
//!
//! The decision is cheap and never wrong, not a validation: the keywords a
//! condition is written with (`const`, `enum`, `type`, `required`,
//! `properties`, `patternProperties`, `additionalProperties`,
//! `propertyNames`, dependent schemas and keys, items, `contains`,
//! `pattern`, bounds, the combinators) are checked, a keyword of one kind
//! of value passes every other kind, and a condition using another
//! keyword of a draft is undecided (both branches are offered). A word no
//! draft defines (`items: {"x_one": {}}`) fails nothing, as validators
//! ignore it.

use super::node::{Entry, Node};
use super::{item_schema, named_properties, ref_target, refs_stand_alone};
use crate::editor::completion::{json_type_name, pattern_decides};
use serde_json::Value;

impl Entry<'_> {
    /// Whether every schema here allows `value` (`None` when one cannot
    /// tell, see [`decides`]): each of an `All`, one of an `Any`.
    pub(crate) fn accepts(&self, value: &Value, root: &Value) -> Option<bool> {
        accepted(&self.0, value, root, 0, false)
    }

    /// [`Self::accepts`] with each `if` not decided read as taking a
    /// branch that says something of the value: what a value meeting the
    /// schemas whichever way the `if` goes must meet.
    pub(crate) fn accepts_in_a_branch(&self, value: &Value, root: &Value) -> Option<bool> {
        accepted(&self.0, value, root, 0, true)
    }

    /// [`Self::accepts`] reading no deeper than a few levels of schemas
    /// and values (`None` past them): a check whose cost does not grow
    /// with what the value holds.
    pub(crate) fn accepts_near(&self, value: &Value, root: &Value) -> Option<bool> {
        accepted(&self.0, value, root, MAX_DEPTH - NEAR, false)
    }

    /// [`Self::accepts_near`] reading twice as deep: for a value a few
    /// levels below the options deciding it, through a `$ref` or two.
    pub(crate) fn accepts_below(&self, value: &Value, root: &Value) -> Option<bool> {
        accepted(&self.0, value, root, MAX_DEPTH - 2 * NEAR, false)
    }
}

/// Levels [`Entry::accepts_near`] reads.
const NEAR: usize = 6;

/// Levels of schemas and values [`decides`] reads at most.
const MAX_DEPTH: usize = 32;

/// `in_a_branch`: see [`Entry::accepts_in_a_branch`].
fn accepted(
    node: &Node<'_>,
    value: &Value,
    root: &Value,
    depth: usize,
    in_a_branch: bool,
) -> Option<bool> {
    let part = |part: &Node<'_>| accepted(part, value, root, depth, in_a_branch);
    match node {
        Node::Leaf(schema) => decide(schema, value, root, depth),
        Node::All(parts) => all(parts.iter().map(|one| part(one))),
        Node::Any(parts) | Node::Either(parts) => {
            let mut known = true;
            for one in parts {
                match part(one) {
                    Some(true) => return Some(true),
                    None => known = false,
                    Some(false) => {}
                }
            }
            // An `if` not decided whose `then` or `else` says nothing (left
            // out of the walk) may take the value in that branch.
            if matches!(node, Node::Either(_)) && parts.len() == 1 && !in_a_branch {
                return None;
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

/// Whether an object holding exactly `keys`, its values unknown, matches
/// `condition`: not where a key it requires is missing; yes where it asks
/// nothing but keys there (and annotations); else `None`.
pub(crate) fn decides_keys(
    condition: &Value,
    keys: &std::collections::HashSet<String>,
) -> Option<bool> {
    let members = match condition {
        Value::Bool(allowed) => return Some(*allowed),
        Value::Object(members) => members,
        _ => return None,
    };
    let required = members
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str);
    if required.clone().any(|key| !keys.contains(key)) {
        return Some(false);
    }
    members
        .iter()
        .all(|(keyword, argument)| {
            ANNOTATIONS.contains(&keyword.as_str())
                || keyword == "required"
                || (keyword == "type" && (argument == "object"))
        })
        .then_some(true)
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

/// Keywords of a JSON Schema draft that may fail a value and [`decide`]
/// does not read: a schema using one is undecided.
const UNREAD: [&str; 8] = [
    "$dynamicRef",
    "$recursiveRef",
    "contentSchema",
    "contentEncoding",
    "contentMediaType",
    "divisibleBy",
    "disallow",
    "extends",
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
    if depth > MAX_DEPTH {
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
                // How many items match (at least one unless `minContains`
                // says otherwise, at most `maxContains`).
                "contains" => match value {
                    Value::Array(items) => {
                        let bound = |keyword: &str| members.get(keyword).and_then(Value::as_u64);
                        let mut matched = 0;
                        for item in items {
                            if decide(argument, item, root, depth + 1)? {
                                matched += 1;
                            }
                        }
                        Some(
                            matched >= bound("minContains").unwrap_or(1)
                                && bound("maxContains").is_none_or(|most| matched <= most),
                        )
                    }
                    _ => Some(true),
                },
                // Read with `contains`, and nothing without it.
                "minContains" | "maxContains" => Some(true),
                "propertyNames" => match value {
                    Value::Object(present) => all(present
                        .keys()
                        .map(|key| decide(argument, &Value::String(key.clone()), root, depth + 1))),
                    _ => Some(true),
                },
                "dependentRequired" => match value {
                    Value::Object(present) => Some(
                        argument
                            .as_object()?
                            .iter()
                            .filter(|(key, _)| present.contains_key(*key))
                            .all(|(_, keys)| {
                                keys.as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .all(|key| present.contains_key(key))
                            }),
                    ),
                    _ => Some(true),
                },
                // Items told apart as JSON Schema does (`1` is `1.0`), in a
                // short list only: undecided for a long one.
                "uniqueItems" => match value {
                    Value::Array(items) if *argument == Value::Bool(true) => {
                        (items.len() <= MAX_UNIQUE_ITEMS).then(|| {
                            items.iter().enumerate().all(|(at, item)| {
                                items[..at].iter().all(|before| !equal(before, item))
                            })
                        })
                    }
                    _ => Some(true),
                },
                // What other keywords leave of a list or an object: nothing of
                // another value, else undecided here.
                "unevaluatedItems" => (!value.is_array()).then_some(true),
                "unevaluatedProperties" => (!value.is_object()).then_some(true),
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
                keyword if UNREAD.contains(&keyword) => None,
                // A word no draft defines fails nothing: validators ignore it.
                _ => Some(true),
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

/// Most items [`decides`] tells apart for `uniqueItems`.
const MAX_UNIQUE_ITEMS: usize = 64;

/// Whether two values are equal as JSON Schema compares them: numbers by
/// their value (`1` is `1.0`), lists and objects member by member.
fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| equal(a, b))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, a)| right.get(key).is_some_and(|b| equal(a, b)))
        }
        _ => left == right,
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
            decides(&json!({"unevaluatedItems": false}), &json!([1]), &root),
            None,
            "a keyword left unread decides nothing"
        );
        assert_eq!(
            decides(
                &json!({"const": 1, "unevaluatedItems": false}),
                &json!(2),
                &root
            ),
            Some(false),
            "a failed keyword decides"
        );
        let unique = json!({"uniqueItems": true});
        assert_eq!(decides(&unique, &json!([1, 2]), &root), Some(true));
        assert_eq!(decides(&unique, &json!([1, 1.0]), &root), Some(false));
        assert_eq!(
            decides(&unique, &json!([{"a": [1]}, {"a": [1.0]}]), &root),
            Some(false)
        );
        assert_eq!(
            decides(&unique, &json!((0..65).collect::<Vec<_>>()), &root),
            None
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
