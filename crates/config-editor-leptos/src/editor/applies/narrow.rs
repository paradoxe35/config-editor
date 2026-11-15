//! Which `oneOf`/`anyOf` option an object is in, by a quick look that is
//! never wrong about what it tells (not a validation): the options it
//! cannot be in are refuted, and the one its keys and values pick is
//! matched. Only an object (or a value known to be one by its keys) is
//! narrowed.

use super::node::{State, Tree, Walked};
use super::query::leaves;
use super::{Seen, ref_target, type_includes};
use serde_json::{Map, Value};
use std::collections::HashSet;

/// What `seen` tells of each of `options` (each with its walked tree).
pub(super) fn states(options: &[Tree<'_>], seen: Seen<'_>, root: &Value) -> Vec<State> {
    let open = || vec![State::Open; options.len()];
    let map = seen.value.and_then(Value::as_object);
    if map.is_none() && seen.keys.is_none() {
        return open();
    }
    let looks: Vec<Look<'_>> = options
        .iter()
        .map(|tree| Look::of(leaves(tree, Walked::Certain)))
        .collect();
    let refuted: Vec<bool> = looks.iter().map(|look| look.refuted(map, root)).collect();
    let left: Vec<usize> = (0..options.len()).filter(|&at| !refuted[at]).collect();
    if left.is_empty() {
        return open();
    }
    let mut states: Vec<State> = refuted
        .iter()
        .map(|&refuted| if refuted { State::Refuted } else { State::Open })
        .collect();
    if let Some(matched) = matched(&looks, &left, map, seen.keys, root) {
        states[matched] = State::Matched;
    }
    states
}

/// What the certain schemas of one option say of an object.
struct Look<'s> {
    leaves: Vec<&'s Value>,
    required: Vec<&'s str>,
}

impl<'s> Look<'s> {
    fn of(leaves: Vec<&'s Value>) -> Self {
        let mut required: Vec<&str> = Vec::new();
        for name in leaves
            .iter()
            .filter_map(|leaf| leaf.get("required").and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
        {
            if !required.contains(&name) {
                required.push(name);
            }
        }
        Self { leaves, required }
    }

    fn properties(&self) -> impl Iterator<Item = (&'s String, &'s Value)> + '_ {
        self.leaves
            .iter()
            .filter_map(|leaf| leaf.get("properties").and_then(Value::as_object))
            .flatten()
    }

    /// Whether the object is not in this option: one of its schemas has a
    /// type that is no object, or a property listing the values allowed
    /// has a scalar of the object's that is none of them, also as text
    /// (XML's `1` is the `"1"` listed).
    fn refuted(&self, map: Option<&Map<String, Value>>, root: &Value) -> bool {
        if self
            .leaves
            .iter()
            .any(|leaf| leaf.get("type").is_some() && !type_includes(leaf, "object"))
        {
            return true;
        }
        let Some(map) = map else {
            return false;
        };
        self.properties().any(|(key, property)| {
            let Some(present) = map
                .get(key)
                .filter(|present| !present.is_object() && !present.is_array())
            else {
                return false;
            };
            let mut allowed = listed(target(property, root)).peekable();
            allowed.peek().is_some()
                && !allowed.any(|allowed| {
                    allowed == present || scalar_text(allowed) == scalar_text(present)
                })
        })
    }

    /// The properties holding one `const` value.
    fn consts<'a>(&'a self, root: &'a Value) -> Vec<(&'a str, &'a Value)> {
        self.properties()
            .filter_map(|(key, property)| {
                Some((key.as_str(), target(property, root).get("const")?))
            })
            .collect()
    }

    /// Whether the object fits the option as far as a quick look tells:
    /// its required keys are there, no key a closed option shuts out is,
    /// and no declared value is refused.
    fn fits(&self, map: &Map<String, Value>, root: &Value) -> bool {
        let closed = self
            .leaves
            .iter()
            .any(|leaf| leaf.get("additionalProperties") == Some(&Value::Bool(false)));
        self.required.iter().all(|key| map.contains_key(*key))
            && map.iter().all(|(key, value)| {
                let mut declared = self
                    .properties()
                    .filter(|(name, _)| *name == key)
                    .map(|(_, property)| property)
                    .peekable();
                if declared.peek().is_none() {
                    return !closed;
                }
                declared.all(|property| value_fits(property, value, root))
            })
    }

    fn declares(&self, key: &str) -> bool {
        self.properties().any(|(name, _)| name == key)
    }
}

/// The one of `left` the object is in: the only one; else the one whose
/// `required` keys are all there; else the one whose `const` properties
/// it has; else the one it fits, or the one fitting that declares every
/// key it has.
fn matched(
    looks: &[Look<'_>],
    left: &[usize],
    map: Option<&Map<String, Value>>,
    keys: Option<&HashSet<String>>,
    root: &Value,
) -> Option<usize> {
    let only = |found: &[usize]| match found {
        [only] => Some(*only),
        _ => None,
    };
    if let Some(only) = only(left) {
        return Some(only);
    }
    let has = |key: &str| {
        map.is_some_and(|map| map.contains_key(key)) || keys.is_some_and(|keys| keys.contains(key))
    };
    let by_required: Vec<usize> = left
        .iter()
        .copied()
        .filter(|&at| {
            !looks[at].required.is_empty() && looks[at].required.iter().all(|key| has(key))
        })
        .collect();
    if let Some(only) = only(&by_required) {
        return Some(only);
    }
    let map = map?;
    let pool = if by_required.is_empty() {
        left.to_vec()
    } else {
        by_required
    };
    let by_const: Vec<usize> = pool
        .into_iter()
        .filter(|&at| {
            let consts = looks[at].consts(root);
            !consts.is_empty()
                && consts
                    .iter()
                    .all(|(key, expected)| map.get(*key) == Some(*expected))
        })
        .collect();
    if let Some(only) = only(&by_const) {
        return Some(only);
    }
    let fitting: Vec<usize> = left
        .iter()
        .copied()
        .filter(|&at| looks[at].fits(map, root))
        .collect();
    only(&fitting).or_else(|| {
        let declaring: Vec<usize> = fitting
            .into_iter()
            .filter(|&at| map.keys().all(|key| looks[at].declares(key)))
            .collect();
        only(&declaring)
    })
}

/// Whether `value` fits a property's `schema` (through a `$ref`) by a
/// quick look at the value alone: its `const`, `enum`, type and numeric
/// bounds (what is inside it is not read, so the look costs the same for
/// any value). `false` alone fits nothing. A language without number
/// types (XML) reads `9` as text: text is also read as the number,
/// boolean or `null` it writes.
fn value_fits(schema: &Value, value: &Value, root: &Value) -> bool {
    let schema = target(schema, root);
    if crate::editor::completion::admits_nothing(schema) {
        return false;
    }
    let read;
    let value = match value {
        Value::String(text) if schema.get("type").is_some() && !kind_fits(schema, value) => {
            read = serde_json::from_str::<Value>(text.trim())
                .ok()
                .filter(|read| !read.is_string());
            read.as_ref().unwrap_or(value)
        }
        _ => value,
    };
    let number = value.as_f64();
    let bound = |keyword: &str, holds: fn(f64, f64) -> bool| match (
        schema.get(keyword).and_then(Value::as_f64),
        number,
    ) {
        (Some(limit), Some(number)) => holds(number, limit),
        _ => true,
    };
    schema.get("const").is_none_or(|expected| expected == value)
        && schema
            .get("enum")
            .and_then(Value::as_array)
            .is_none_or(|allowed| allowed.contains(value))
        && (schema.get("type").is_none() || kind_fits(schema, value))
        && bound("minimum", |number, limit| number >= limit)
        && bound("maximum", |number, limit| number <= limit)
        && bound("exclusiveMinimum", |number, limit| number > limit)
        && bound("exclusiveMaximum", |number, limit| number < limit)
}

/// Whether `schema`'s type (or the object keywords it has) allows
/// `value`'s kind.
fn kind_fits(schema: &Value, value: &Value) -> bool {
    match value {
        Value::Object(_) => {
            type_includes(schema, "object")
                || ["properties", "additionalProperties", "patternProperties"]
                    .iter()
                    .any(|keyword| schema.get(*keyword).is_some())
        }
        Value::String(_) => type_includes(schema, "string"),
        Value::Bool(_) => type_includes(schema, "boolean"),
        Value::Null => type_includes(schema, "null"),
        Value::Number(number) => {
            type_includes(schema, "number")
                || (number.is_i64() || number.is_u64()) && type_includes(schema, "integer")
        }
        Value::Array(_) => true,
    }
}

/// The schema `schema`'s `$ref` names, else `schema`.
fn target<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| ref_target(reference, root))
        .unwrap_or(schema)
}

/// The values `schema` lists as the only ones allowed (`const`, `enum`).
pub(crate) fn listed(schema: &Value) -> impl Iterator<Item = &Value> {
    schema.get("const").into_iter().chain(
        schema
            .get("enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    )
}

/// A scalar as text writes it.
pub(crate) fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
