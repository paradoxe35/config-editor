//! Which kinds of value (string, integer, object, …) everything applying
//! to a value allows, combined as the schemas combine: each schema of an
//! `allOf` (and a schema beside its `$ref` target) must allow a kind, one
//! option of a `oneOf`/`anyOf` that the value is not known to be outside
//! of is enough, and a schema without `type` (or a cycle) allows any.

use super::node::{Node, State, Tree};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// A set of JSON value kinds. Integers and other numbers are apart, so
/// `integer` is a part of `number`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Types(u8);

const NULL: u8 = 1;
const BOOLEAN: u8 = 1 << 1;
const OBJECT: u8 = 1 << 2;
const ARRAY: u8 = 1 << 3;
const STRING: u8 = 1 << 4;
const INTEGER: u8 = 1 << 5;
/// A number that is no integer.
const FRACTION: u8 = 1 << 6;

impl Types {
    /// Every kind: what a schema without `type` allows.
    pub(crate) const ANY: Types =
        Types(NULL | BOOLEAN | OBJECT | ARRAY | STRING | INTEGER | FRACTION);
    const NONE: Types = Types(0);

    /// The kinds a `type` name stands for (none for a name JSON Schema
    /// does not have).
    fn named(name: &str) -> Types {
        Types(match name {
            "null" => NULL,
            "boolean" => BOOLEAN,
            "object" => OBJECT,
            "array" => ARRAY,
            "string" => STRING,
            "integer" => INTEGER,
            "number" => INTEGER | FRACTION,
            _ => 0,
        })
    }

    /// The kind of `value` (a number is an integer when it is written as
    /// one).
    fn of(value: &Value) -> Types {
        Types(match value {
            Value::Null => NULL,
            Value::Bool(_) => BOOLEAN,
            Value::Object(_) => OBJECT,
            Value::Array(_) => ARRAY,
            Value::String(_) => STRING,
            Value::Number(number) if number.is_i64() || number.is_u64() => INTEGER,
            Value::Number(_) => FRACTION,
        })
    }

    /// Whether a value of `value`'s kind is allowed.
    pub(crate) fn allows(self, value: &Value) -> bool {
        self.0 & Types::of(value).0 != 0
    }

    /// Whether some value of the `type` named `name` is allowed (`number`:
    /// an integer or another number).
    pub(crate) fn allows_type(self, name: &str) -> bool {
        self.0 & Types::named(name).0 != 0
    }

    /// Whether only values of the `type` named `name` are allowed.
    pub(crate) fn is_only(self, name: &str) -> bool {
        self.0 != 0 && self.0 & !Types::named(name).0 == 0
    }

    fn and(self, other: Types) -> Types {
        Types(self.0 & other.0)
    }

    fn or(self, other: Types) -> Types {
        Types(self.0 | other.0)
    }
}

/// The kinds one schema's own keywords allow: its `type`, narrowed to the
/// kinds of the values its `const` or `enum` lists.
fn own(schema: &Value) -> Types {
    let members = match schema {
        Value::Bool(false) => return Types::NONE,
        Value::Object(members) => members,
        _ => return Types::ANY,
    };
    let mut types = match members.get("type") {
        Some(Value::String(name)) => Types::named(name),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .fold(Types::NONE, |types, name| types.or(Types::named(name))),
        _ => Types::ANY,
    };
    if let Some(value) = members.get("const") {
        types = types.and(Types::of(value));
    }
    if let Some(Value::Array(values)) = members.get("enum") {
        types = types.and(
            values
                .iter()
                .fold(Types::NONE, |listed, value| listed.or(Types::of(value))),
        );
    }
    types
}

/// The kinds `tree` (walked for a value) allows: see the module doc.
pub(super) fn types(tree: &Tree<'_>) -> Types {
    Combine::default().of(tree)
}

#[derive(Default)]
struct Combine<'s> {
    done: HashMap<*const Node<'s>, Types>,
}

impl<'s> Combine<'s> {
    fn of(&mut self, tree: &Tree<'s>) -> Types {
        let key = Rc::as_ptr(tree);
        if let Some(known) = self.done.get(&key) {
            return *known;
        }
        let types = match &**tree {
            Node::Leaf(schema) => own(schema),
            Node::All(parts) => parts
                .iter()
                .fold(Types::ANY, |types, part| types.and(self.of(part))),
            Node::Any(parts) if !parts.is_empty() => self.union(parts.iter()),
            // An `if` the value does not decide: its `then` or its `else`
            // (one missing, or cut, allows any).
            Node::Either(parts) if parts.len() == 2 => self.union(parts.iter()),
            // Every option the value is not known to be outside of: one it
            // is matched to by its keys may still not be the one it meets.
            Node::Options(choices) => {
                let kept: Vec<&Tree<'s>> = choices
                    .iter()
                    .filter(|choice| choice.state != State::Refuted)
                    .map(|choice| &choice.tree)
                    .collect();
                if kept.is_empty() {
                    Types::ANY
                } else {
                    self.union(kept.into_iter())
                }
            }
            Node::Any(_) | Node::Either(_) | Node::Cut => Types::ANY,
        };
        self.done.insert(key, types);
        types
    }

    fn union<'t>(&mut self, parts: impl Iterator<Item = &'t Tree<'s>>) -> Types
    where
        's: 't,
    {
        parts.fold(Types::NONE, |types, part| types.or(self.of(part)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Here, Seen};
    use serde_json::{Value, json};

    fn allowed(schema: &Value, value: &Value) -> bool {
        Here::of(schema, schema).types().allows(value)
    }

    fn allowed_for(schema: &Value, document: &Value, value: &Value) -> bool {
        Here::walk(
            super::super::Entry::schema(schema),
            schema,
            Seen::value(Some(document)),
        )
        .types()
        .allows(value)
    }

    #[test]
    fn kinds_combine_as_the_schemas_do() {
        let text = json!("x");
        // Every schema of an `allOf` must allow the kind.
        let all = json!({"allOf": [{"type": ["string", "array"]}, {"type": "array"}]});
        assert!(!allowed(&all, &text));
        assert!(allowed(&all, &json!([])));
        // One option of a `oneOf` is enough.
        let one = json!({"oneOf": [{"type": "array"}, {"type": "string"}]});
        assert!(allowed(&one, &text));
        assert!(!allowed(&one, &json!(1)));
        // A schema without `type` allows any kind; an empty property too.
        assert!(allowed(&json!({"description": "d"}), &text));
        assert!(allowed(&json!({"properties": {"a": {}}}), &json!(1)));
        // Listed values narrow the kinds.
        assert!(!allowed(&json!({"enum": ["a", "b"]}), &json!(4)));
        assert!(allowed(&json!({"type": "number", "const": 1}), &json!(2)));
        assert!(!allowed(&json!({"type": "integer"}), &json!(1.5)));
        assert!(allowed(&json!({"type": "number"}), &json!(1.5)));
        assert!(!allowed(&json!(false), &text));
        // An `if` not decided: its `then` or its `else`; one missing allows
        // any.
        let either = json!({"if": {"required": ["a"]}, "then": {"type": "object"}, "else": {"type": "string"}});
        assert!(allowed(&either, &text));
        assert!(!allowed(&either, &json!(1)));
        let then_only = json!({"if": {"required": ["a"]}, "then": {"type": "object"}});
        assert!(allowed(&then_only, &json!(1)));
        // A `$ref` cycle says nothing more.
        let cycle = json!({"$defs": {"a": {"$ref": "#/$defs/a"}}, "$ref": "#/$defs/a"});
        assert!(allowed(&cycle, &text));
    }

    #[test]
    fn an_option_the_object_is_outside_of_allows_nothing_of_its_own() {
        let schema = json!({"oneOf": [
            {"type": "object", "required": ["kind"], "properties": {"kind": {"const": "a"}}},
            {"type": "object", "required": ["kind"], "properties": {"kind": {"const": "b"}, "x": {}}},
            {"type": "string"}
        ]});
        // The object refutes the string option.
        assert!(!allowed_for(&schema, &json!({"kind": "a"}), &json!("s")));
        assert!(allowed_for(&schema, &json!({"kind": "a"}), &json!({})));
        // Unknown, any option's kind is allowed.
        assert!(allowed(&schema, &json!("s")));
    }
}
