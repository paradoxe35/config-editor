//! Which of two values the schemas at a node tell apart, part by part: a
//! part refusing both (the document fails there whichever is saved) tells
//! nothing, so a failure elsewhere in the same schemas does not hide the
//! part that takes one value and refuses the other.

use super::accepts::{BELOW, MAX_DEPTH, accepted, decide};
use super::node::{Entry, Node};
use super::{ref_target, refs_stand_alone};
use serde_json::{Map, Value};

/// What the parts of the schemas at a node make of two values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Refusing {
    /// A part takes the second value and refuses the first.
    pub(crate) first: bool,
    /// A part takes the first value and refuses the second.
    pub(crate) second: bool,
}

impl Refusing {
    fn note(&mut self, first: Option<bool>, second: Option<bool>) {
        match (first, second) {
            (Some(false), Some(true)) => self.first = true,
            (Some(true), Some(false)) => self.second = true,
            _ => {}
        }
    }
}

impl Entry<'_> {
    /// Which of `first` and `second` (two values of the node) a part of
    /// the schemas here refuses where it takes the other: each schema
    /// applying, each of its `allOf` and references, each property and
    /// each other keyword (those read together, as `if` with its branches,
    /// together), read as deep as `BELOW` lets.
    pub(crate) fn refusing(&self, first: &Value, second: &Value, root: &Value) -> Refusing {
        let mut refusing = Refusing::default();
        node(&self.0, (first, second), root, &mut refusing);
        refusing
    }
}

fn node(node_here: &Node<'_>, values: (&Value, &Value), root: &Value, out: &mut Refusing) {
    match node_here {
        Node::Leaf(schema) => schema_parts(schema, values, root, BELOW, out),
        Node::All(parts) => {
            for part in parts {
                node(part, values, root, out);
            }
        }
        whole => out.note(
            accepted(whole, values.0, root, BELOW, false),
            accepted(whole, values.1, root, BELOW, false),
        ),
    }
}

/// Keywords read together: each group is one part.
const GROUPS: [&[&str]; 4] = [
    &["items", "prefixItems", "additionalItems"],
    &["contains", "minContains", "maxContains"],
    &["if", "then", "else"],
    &["patternProperties", "additionalProperties"],
];

fn schema_parts(
    schema: &Value,
    (first, second): (&Value, &Value),
    root: &Value,
    depth: usize,
    out: &mut Refusing,
) {
    let whole = |out: &mut Refusing, schema: &Value| {
        out.note(
            decide(schema, first, root, depth),
            decide(schema, second, root, depth),
        );
    };
    let Value::Object(members) = schema else {
        return whole(out, schema);
    };
    // A schema reading what the others leave (`unevaluated*`) is read
    // whole, and so is a reference standing alone.
    let alone = members.contains_key("$ref") && refs_stand_alone(root);
    if depth >= MAX_DEPTH
        || members.contains_key("unevaluatedProperties")
        || members.contains_key("unevaluatedItems")
    {
        return whole(out, schema);
    }
    let deeper = depth + 1;
    for (keyword, argument) in members {
        let keyword = keyword.as_str();
        if alone && keyword != "$ref" {
            continue;
        }
        match keyword {
            "allOf" => {
                for part in argument.as_array().into_iter().flatten() {
                    schema_parts(part, (first, second), root, deeper, out);
                }
            }
            "$ref" => match argument.as_str().and_then(|to| ref_target(to, root)) {
                Some(target) => schema_parts(target, (first, second), root, deeper, out),
                None => whole(out, &single(keyword, argument)),
            },
            // Each property, of the values' own.
            "properties" => {
                for (key, property) in argument.as_object().into_iter().flatten() {
                    let mut one = Map::new();
                    one.insert(key.clone(), property.clone());
                    whole(out, &single(keyword, &Value::Object(one)));
                }
            }
            _ => match GROUPS.iter().find(|group| group.contains(&keyword)) {
                Some(group) if group[0] == keyword || !members.contains_key(group[0]) => {
                    whole(out, &grouped(members, group));
                }
                Some(_) => {}
                None => whole(out, &single(keyword, argument)),
            },
        }
    }
}

fn single(keyword: &str, argument: &Value) -> Value {
    let mut one = Map::new();
    one.insert(keyword.to_string(), argument.clone());
    Value::Object(one)
}

/// The keywords of `group` in `members`, with the names `properties`
/// gives (which `additionalProperties` does not judge) taking anything.
fn grouped(members: &Map<String, Value>, group: &[&str]) -> Value {
    let mut part = Map::new();
    for keyword in group {
        if let Some(argument) = members.get(*keyword) {
            part.insert((*keyword).to_string(), argument.clone());
        }
    }
    if part.contains_key("additionalProperties")
        && let Some(Value::Object(named)) = members.get("properties")
    {
        let any: Map<String, Value> = named
            .keys()
            .map(|key| (key.clone(), Value::Bool(true)))
            .collect();
        part.insert("properties".to_string(), Value::Object(any));
    }
    Value::Object(part)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refusing(schema: Value, first: Value, second: Value) -> Refusing {
        Entry::schema(&schema).refusing(&first, &second, &schema)
    }

    #[test]
    fn a_part_refusing_both_values_tells_nothing() {
        // `port` must be a number whichever `d` is: only the `oneOf`
        // tells the two apart.
        let schema = json!({
            "dependentSchemas": {"mode": {"properties": {"port": {"type": "number"}}}},
            "properties": {"port": {"oneOf": [
                {"properties": {"d": {"type": "integer"}}},
                {"required": ["n"]},
            ]}},
        });
        let told = refusing(
            schema,
            json!({"mode": 0, "port": {"d": "0"}}),
            json!({"mode": 0, "port": {"d": 0}}),
        );
        assert_eq!(
            told,
            Refusing {
                first: true,
                second: false
            }
        );
    }

    #[test]
    fn a_sibling_property_failing_both_ways_tells_nothing() {
        let schema = json!({"properties": {"a": {"type": "integer"}, "b": {"type": "string"}}});
        let told = refusing(schema, json!({"a": "1", "b": 2}), json!({"a": 1, "b": 2}));
        assert_eq!(
            told,
            Refusing {
                first: true,
                second: false
            }
        );
    }

    #[test]
    fn parts_refusing_one_value_each_say_so_both() {
        let schema = json!({"allOf": [
            {"properties": {"a": {"type": "integer"}}},
            {"properties": {"a": {"maximum": 0}}},
        ]});
        let told = refusing(schema, json!({"a": "1"}), json!({"a": 1}));
        assert_eq!(
            told,
            Refusing {
                first: true,
                second: true
            }
        );
    }

    #[test]
    fn additional_properties_judge_only_the_names_properties_does_not_give() {
        let schema = json!({"properties": {"a": {}}, "additionalProperties": {"type": "integer"}});
        let told = refusing(
            schema,
            json!({"a": "x", "b": "1"}),
            json!({"a": "x", "b": 1}),
        );
        assert_eq!(
            told,
            Refusing {
                first: true,
                second: false
            }
        );
    }
}
