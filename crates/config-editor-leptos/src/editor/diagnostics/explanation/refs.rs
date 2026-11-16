//! Each `$ref` target explained once for each value, in each option's
//! explanation.
//!
//! A schema that reaches the same target twice for one value (an `allOf`
//! whose members each hold the node again, or members that each name the
//! same definition) makes the validator explain the value twice: the
//! same errors, at the same instance and schema paths (a `$ref`'s errors
//! are placed where its target is written). Through a recursive node,
//! the copies multiply at every level: three members each holding the
//! node, with an error at every level, gave 3^depth errors and ran out of
//! memory 16 levels down.
//!
//! So each `$ref` of the explaining validator's schema names, instead of
//! its target, a definition added beside it (in the same resource, so the
//! target resolves the same): `{"allOf": [{MARK: n}, {"if": {KEYWORD: n},
//! "then": {"$ref": target}}]}`. Its errors are collected only the first
//! time it is asked for a value in the explanation of one option (or of
//! the document): after, it holds the same values and says nothing more,
//! the first time having said it. Only values that are objects or lists
//! (whose addresses are their own; a key under `propertyNames` is a text
//! made for the check) are kept; a scalar's errors are few.
//!
//! A `$recursiveRef` may only name `#`, so it is not pointed elsewhere:
//! it moves into its definition as written, the site naming the
//! definition by a `$ref` (a `$ref` in one resource enters no other, so
//! it resolves the same there). Left as written: the references under
//! `not` (whose schema a problem words, see `unwrapped`). In a schema
//! with a `$dynamicRef` or `$recursiveRef`, whose target depends on the
//! way there, a target is explained each time it is reached
//! ([`Form::Counted`]). Every form spends the budget of `bound` for each
//! explanation.

use std::collections::HashMap;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::{Map, Value, json};

use super::{MARK, mentions_key};
use crate::editor::schema::{Holds, holds};

/// The keyword of [`Form::Explained`].
pub(super) const KEYWORD: &str = "x-config-editor-ref-explained";

/// The keyword of [`Form::Counted`].
pub(super) const SPENT: &str = "x-config-editor-ref-spent";

/// The keyword of [`Form::Stopping`].
pub(super) const STOP: &str = "x-config-editor-ref-stop";

/// The prefix of the definitions' names.
const NAME: &str = "x-config-editor-ref-";

/// What a reference's definition does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Form {
    /// Explains its target once per value in each option's explanation
    /// (`{"if": {KEYWORD: n}, "then": …}`): the explaining validator's.
    Explained,
    /// Explains its target each time, while the budget lasts (`{"if":
    /// {SPENT: n}, "then": …}`): an explaining validator's whose schema
    /// has a dynamic reference.
    Counted,
    /// The same, in any draft (`{"anyOf": [{STOP: n}, …]}`: no `if`); the
    /// option it adds shows as its target's errors (see [`is_stop`]).
    Stopping,
}

/// Points each `$ref` of `schema` (each `$dynamicRef` and
/// `$recursiveRef` too, but in [`Form::Explained`]) at a definition
/// explaining its target as `form` says, numbering them on from
/// `count`. [`Form::Explained`] is [`Form::Counted`] for a schema with a
/// dynamic reference.
pub(super) fn wrap(schema: &mut Value, count: &mut u64, form: Form) {
    let form = if form == Form::Explained && mentions_key(schema, &["$dynamicRef", "$recursiveRef"])
    {
        Form::Counted
    } else {
        form
    };
    let legacy = schema
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|draft| {
            ["draft-04", "draft-06", "draft-07"]
                .iter()
                .any(|old| draft.contains(old))
        });
    Walk {
        legacy,
        form,
        count,
    }
    .in_resource(schema);
}

/// Whether `error` is the option a [`Form::Stopping`] definition adds:
/// its errors are its second option's, its target's.
pub(in crate::editor::diagnostics) fn is_stop(error: &ValidationError<'_>) -> bool {
    if !matches!(error.kind(), ValidationErrorKind::AnyOf { context } if context.len() == 2) {
        return false;
    }
    let segments: Vec<&str> = error.schema_path().as_str().rsplit('/').take(4).collect();
    matches!(segments.as_slice(), ["anyOf", "1", "allOf", name] if name.starts_with(NAME))
}

/// The definition, numbered `number`, of `reference` (`$ref`,
/// `$dynamicRef` or `$recursiveRef`) to `target`, in `form`.
fn definition(form: Form, reference: &str, target: &str, number: u64) -> Value {
    let to_target = json!({ reference: target });
    match form {
        Form::Explained => json!({"allOf": [{ MARK: number },
            {"if": { KEYWORD: number }, "then": to_target}]}),
        Form::Counted => json!({"allOf": [{ MARK: number },
            {"if": { SPENT: number }, "then": to_target}]}),
        Form::Stopping => json!({"allOf": [{ MARK: number },
            {"anyOf": [{ STOP: number }, to_target]}]}),
    }
}

/// The definitions added to one resource, by reference and target.
#[derive(Default)]
struct Definitions {
    by_target: HashMap<(String, String), String>,
    added: Vec<(String, Value)>,
}

/// One rewrite of a schema.
struct Walk<'c> {
    legacy: bool,
    form: Form,
    count: &'c mut u64,
}

impl Walk<'_> {
    /// The pointer to the definition of `reference` to `target` in
    /// `definitions` (added the first time).
    fn named(&mut self, definitions: &mut Definitions, reference: &str, target: &str) -> String {
        let holder = if self.legacy { "definitions" } else { "$defs" };
        let key = (reference.to_string(), target.to_string());
        let name = definitions.by_target.entry(key).or_insert_with(|| {
            *self.count += 1;
            let name = format!("{NAME}{}", self.count);
            let written = definition(self.form, reference, target, *self.count);
            definitions.added.push((name.clone(), written));
            name
        });
        format!("#/{holder}/{name}")
    }

    /// Wraps the references of `resource` (a schema starting a
    /// resource), adding their definitions to it.
    fn in_resource(&mut self, resource: &mut Value) {
        let mut definitions = Definitions::default();
        self.visit(resource, &mut definitions, true);
        if definitions.added.is_empty() {
            return;
        }
        let holder = if self.legacy { "definitions" } else { "$defs" };
        let Some(members) = resource.as_object_mut() else {
            return;
        };
        let held = members
            .entry(holder)
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(held) = held {
            held.extend(definitions.added);
        }
    }

    /// Moves the `$recursiveRef` of `members` into its definition: the
    /// schema names it by its `$ref`, or by a `$ref` in its `allOf` when
    /// it has a `$ref` of its own.
    fn recursive_to_definition(
        &mut self,
        members: &mut Map<String, Value>,
        definitions: &mut Definitions,
    ) {
        let Some(Value::String(target)) = members.get("$recursiveRef") else {
            return;
        };
        let target = target.clone();
        let pointer = self.named(definitions, "$recursiveRef", &target);
        let to_definition = json!({ "$ref": pointer });
        if !members.contains_key("$ref") {
            // In place, keeping the order of the keywords.
            let held: Vec<(String, Value)> = std::mem::take(members)
                .into_iter()
                .map(|(key, member)| {
                    if key == "$recursiveRef" {
                        ("$ref".to_string(), to_definition["$ref"].clone())
                    } else {
                        (key, member)
                    }
                })
                .collect();
            members.extend(held);
            return;
        }
        members.remove("$recursiveRef");
        match members.get_mut("allOf") {
            Some(Value::Array(all)) => all.push(to_definition),
            _ => {
                members.insert("allOf".to_string(), json!([to_definition]));
            }
        }
    }

    fn visit(&mut self, value: &mut Value, definitions: &mut Definitions, resource_root: bool) {
        match value {
            Value::Object(members) => {
                if !resource_root && starts_resource(members, self.legacy) {
                    self.in_resource(value);
                    return;
                }
                for (key, member) in members.iter_mut() {
                    let key = key.as_str();
                    if holds(key) == Holds::Data || key == "not" {
                        continue;
                    }
                    if key == "$ref" || (key == "$dynamicRef" && self.form != Form::Explained) {
                        // A pointer names no dynamic anchor: a
                        // `$dynamicRef` to the definition reads as a
                        // `$ref`, the one in it as written.
                        if let Value::String(target) = member {
                            *target = self.named(definitions, key, target);
                        }
                        continue;
                    }
                    if holds(key) == Holds::Named {
                        if let Value::Object(named) = member {
                            for schema in named.values_mut() {
                                self.visit(schema, definitions, false);
                            }
                        }
                        continue;
                    }
                    self.visit(member, definitions, false);
                }
                // After the others: its `$ref` names a definition already.
                if self.form != Form::Explained {
                    self.recursive_to_definition(members, definitions);
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.visit(item, definitions, false);
                }
            }
            _ => {}
        }
    }
}

/// Whether the schema `members` starts a resource of its own (as
/// `explanation::wrap` reads it; beside a `$ref`, a draft-07 `$id` is
/// ignored).
fn starts_resource(members: &Map<String, Value>, legacy: bool) -> bool {
    members
        .get("$id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty() && !id.starts_with('#'))
        && !(legacy && members.contains_key("$ref"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Form, wrap};

    #[test]
    fn each_target_gets_one_definition_in_its_resource() {
        let mut schema = json!({
            "properties": {
                "a": {"$ref": "#/$defs/n"},
                "b": {"$ref": "#/$defs/n"},
                "$ref": {"type": "string"},
                "c": {"not": {"$ref": "#/$defs/n"}},
                "d": {"const": {"$ref": "#/$defs/n"}},
                "e": {"$id": "https://example.test/e", "$ref": "#/$defs/m",
                    "$defs": {"m": {}}}
            },
            "$defs": {"n": {}}
        });
        wrap(&mut schema, &mut 0, Form::Explained);
        assert_eq!(
            schema["properties"]["a"]["$ref"],
            "#/$defs/x-config-editor-ref-1"
        );
        assert_eq!(
            schema["properties"]["b"]["$ref"],
            "#/$defs/x-config-editor-ref-1"
        );
        assert_eq!(schema["properties"]["$ref"], json!({"type": "string"}));
        assert_eq!(schema["properties"]["c"]["not"]["$ref"], "#/$defs/n");
        assert_eq!(schema["properties"]["d"]["const"]["$ref"], "#/$defs/n");
        assert_eq!(
            schema["$defs"]["x-config-editor-ref-1"]["allOf"][1]["then"]["$ref"],
            "#/$defs/n"
        );
        let nested = &schema["properties"]["e"];
        assert_eq!(nested["$ref"], "#/$defs/x-config-editor-ref-2");
        assert_eq!(
            nested["$defs"]["x-config-editor-ref-2"]["allOf"][1]["then"]["$ref"],
            "#/$defs/m"
        );
    }

    #[test]
    fn a_legacy_draft_holds_them_in_definitions() {
        let mut schema = json!({"$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"a": {"$ref": "#/definitions/n"}}, "definitions": {"n": {}}});
        wrap(&mut schema, &mut 0, Form::Stopping);
        assert_eq!(
            schema["properties"]["a"]["$ref"],
            "#/definitions/x-config-editor-ref-1"
        );
        assert_eq!(
            schema["definitions"]["x-config-editor-ref-1"]["allOf"][1]["anyOf"][1]["$ref"],
            "#/definitions/n"
        );
    }

    #[test]
    fn with_a_dynamic_reference_each_is_counted_and_the_dynamic_one_kept() {
        let mut schema = json!({"$dynamicAnchor": "node",
            "properties": {"a": {"$ref": "#/$defs/n"}, "b": {"$dynamicRef": "#node"},
                "r": {"$recursiveRef": "#"}},
            "$defs": {"n": {}}});
        wrap(&mut schema, &mut 0, Form::Explained);
        let definitions = &schema["$defs"];
        assert_eq!(
            definitions["x-config-editor-ref-1"]["allOf"][1]["if"],
            json!({"x-config-editor-ref-spent": 1})
        );
        assert_eq!(
            schema["properties"]["b"]["$dynamicRef"],
            "#/$defs/x-config-editor-ref-2"
        );
        assert_eq!(
            definitions["x-config-editor-ref-2"]["allOf"][1]["then"],
            json!({"$dynamicRef": "#node"})
        );
        assert_eq!(
            schema["properties"]["r"],
            json!({"$ref": "#/$defs/x-config-editor-ref-3"})
        );
        assert_eq!(
            definitions["x-config-editor-ref-3"]["allOf"][1]["then"],
            json!({"$recursiveRef": "#"})
        );
    }

    #[test]
    fn a_recursive_reference_beside_a_ref_is_named_in_its_all_of() {
        let mut schema = json!({"$recursiveAnchor": true, "properties": {"r":
            {"$ref": "#/$defs/n", "$recursiveRef": "#", "allOf": [{"type": "object"}]}},
            "$defs": {"n": {}}});
        wrap(&mut schema, &mut 0, Form::Stopping);
        let site = &schema["properties"]["r"];
        assert_eq!(site["$ref"], "#/$defs/x-config-editor-ref-1");
        assert!(site.get("$recursiveRef").is_none());
        assert_eq!(
            site["allOf"],
            json!([{"type": "object"}, {"$ref": "#/$defs/x-config-editor-ref-2"}])
        );
        assert_eq!(
            schema["$defs"]["x-config-editor-ref-2"]["allOf"][1]["anyOf"][1],
            json!({"$recursiveRef": "#"})
        );
    }

    /// A `$recursiveRef` moved into its definition still reaches the
    /// outermost `$recursiveAnchor`: a tree extended by another resource
    /// checks its nodes by the extension.
    #[test]
    fn a_moved_recursive_reference_resolves_as_written() {
        let schema = json!({"$schema": "https://json-schema.org/draft/2019-09/schema",
            "$id": "https://example.test/strict", "$recursiveAnchor": true,
            "$ref": "tree", "unevaluatedProperties": false,
            "$defs": {"tree": {"$id": "https://example.test/tree", "$recursiveAnchor": true,
                "type": "object",
                "properties": {"data": true, "kids": {"type": "array",
                    "items": {"$recursiveRef": "#"}}}}}});
        let plain = jsonschema::validator_for(&schema).unwrap();
        let explaining = super::super::build(&schema).unwrap();
        let bounded = super::super::bounded(&schema).unwrap();
        for document in [
            json!({"kids": [{"data": 1}]}),
            json!({"kids": [{"data": 1, "extra": 2}]}),
            json!({"kids": [{"kids": [{"extra": 2}]}]}),
            json!({"kids": [{"kids": 3}]}),
        ] {
            let valid = plain.is_valid(&document);
            assert_eq!(explaining.is_valid(&document), valid, "{document}");
            assert_eq!(bounded.is_valid(&document), valid, "{document}");
            let (errors, _) = super::super::errors(&explaining, &document);
            assert_eq!(errors.is_empty(), valid, "{document}");
            let (errors, _) = super::super::errors(&bounded, &document);
            assert_eq!(errors.is_empty(), valid, "{document}");
        }
        assert!(!plain.is_valid(&json!({"kids": [{"extra": 2}]})));
    }
}
