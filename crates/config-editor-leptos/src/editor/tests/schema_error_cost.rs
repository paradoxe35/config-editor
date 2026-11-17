//! A wrong value deep in a recursive `anyOf` costs work linear in its
//! depth, in every language: the explanation of a value that
//! matches none of the options read each option's errors again for each
//! option above it (twice per level for a rule tree whose options exclude
//! each other), and the validator kept a value's errors once for every
//! chain of options above it (three per level with options that do not),
//! which ran out of memory 15 levels down. Counted (see `work_count`):
//! the work two more levels add stays the same, deeper down.

use serde_json::{Value, json};

use crate::editor::codec::serialize;
use crate::editor::diagnostics::{Diagnostic, validate};
use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::work_count::{Work, counted};

/// A rule tree: a node is all of some nodes, any of some nodes, or a
/// field comparison; each option allows only its own keys.
fn rule_tree() -> SchemaBundle {
    let node = json!({"anyOf": [
        {"type": "object", "additionalProperties": false, "properties": {
            "all": {"type": "array", "items": {"$ref": "#/$defs/node"}}, "name": {"type": "string"}}},
        {"type": "object", "additionalProperties": false, "properties": {
            "any": {"type": "array", "items": {"$ref": "#/$defs/node"}}, "name": {"type": "string"}}},
        {"type": "object", "additionalProperties": false, "properties": {
            "field": {"type": "string"}, "eq": {"type": "integer"}}}]});
    build_schema(
        &json!({"type": "object", "properties": {"rule": {"$ref": "#/$defs/node"}},
            "$defs": {"node": node}})
        .to_string(),
    )
    .unwrap()
}

/// `depth` nodes of `all` around a comparison with a text for its number.
fn rule_document(depth: usize) -> Value {
    let mut node = json!({"field": "x", "eq": "notint"});
    for _ in 0..depth {
        node = json!({ "all": [node] });
    }
    json!({ "rule": node })
}

/// Options that do not exclude each other, each holding the node again.
fn overlapping() -> SchemaBundle {
    overlapping_in(None)
}

/// [`overlapping`], under the draft `draft` names (`$schema`) when given.
fn overlapping_in(draft: Option<&str>) -> SchemaBundle {
    let node = json!({"anyOf": [
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "integer"}, {"type": "string"}]}, "c": {"$ref": "#/$defs/node"}}},
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "boolean"}, {"type": "string"}]}, "c": {"$ref": "#/$defs/node"}}},
        {"type": "object", "properties": {
            "v": {"oneOf": [{"type": "number"}, {"enum": ["n"]}]},
            "c": {"anyOf": [{"$ref": "#/$defs/node"}, {"type": "integer"}]}}}]});
    let mut schema = json!({"type": "object", "properties": {"root": {"$ref": "#/definitions/node"}},
        "definitions": {"node": node}});
    if let Some(draft) = draft {
        schema["$schema"] = json!(draft);
    }
    build_schema(
        &schema
            .to_string()
            .replace("#/$defs/node", "#/definitions/node"),
    )
    .unwrap()
}

/// `depth` nodes around one whose `v` is an object no option allows.
fn overlapping_document(depth: usize) -> Value {
    let mut node = json!({"v": {"x": 1}});
    for _ in 0..depth {
        node = json!({ "c": node });
    }
    json!({ "root": node })
}

/// The problems of `document` written in `language`, and the work their
/// explanation took: errors read and options tried.
fn checked(
    schema: &SchemaBundle,
    language: Language,
    document: &Value,
) -> (Vec<Diagnostic>, usize) {
    let text = serialize(language.behavior(), document, None).unwrap();
    let ((problems, read), tried) = counted(Work::OptionTried, || {
        counted(Work::ErrorVisit, || {
            validate(&text, Some(schema), None, true, language.behavior())
        })
    });
    (problems, read + tried)
}

/// The work at each of `depths` (evenly spaced): two more levels add no
/// more than twice what the two before added (twice and four times as
/// much each level before).
fn grows_linearly(
    schema: &SchemaBundle,
    document: fn(usize) -> Value,
    depths: [usize; 3],
    what: &str,
) {
    for &language in Language::ALL {
        let work = depths.map(|depth| {
            let (problems, work) = checked(schema, language, &document(depth));
            assert_eq!(problems.len(), 1, "{what}, {language:?}: {problems:?}");
            work
        });
        assert!(
            work[2] - work[1] <= 2 * (work[1] - work[0]),
            "{what}, {language:?}: work at depths {depths:?}: {work:?}"
        );
    }
}

#[test]
fn a_wrong_value_deep_in_a_rule_tree_costs_work_linear_in_its_depth() {
    grows_linearly(&rule_tree(), rule_document, [8, 12, 16], "rule tree");
}

#[test]
fn a_wrong_value_deep_under_overlapping_options_costs_work_linear_in_its_depth() {
    grows_linearly(
        &overlapping(),
        overlapping_document,
        [4, 6, 8],
        "overlapping options",
    );
}

#[test]
fn deep_recursive_options_give_one_short_problem_and_never_run_out_of_memory() {
    for (what, schema, document) in [
        ("rule tree", rule_tree(), rule_document(25)),
        (
            "overlapping options",
            overlapping(),
            overlapping_document(20),
        ),
    ] {
        for &language in Language::ALL {
            let (problems, _) = checked(&schema, language, &document);
            assert_eq!(problems.len(), 1, "{what}, {language:?}: {problems:?}");
            let message = &problems[0].message;
            assert!(
                message.starts_with("/")
                    && message.chars().count() < 4000
                    && !message.contains("x-config-editor")
                    && !message.contains("\"else\""),
                "{what}, {language:?}: {} characters: {message}",
                message.chars().count()
            );
        }
    }
}

#[test]
fn the_explanation_names_the_wrong_value_in_a_rule_tree() {
    let (problems, _) = checked(&rule_tree(), Language::JSON, &rule_document(3));
    assert_eq!(
        problems[0].message,
        "/rule/all/0/all/0/all/0/eq: \"notint\" is not of type \"integer\""
    );
}

/// Options that do not exclude each other, each holding a list of nodes.
fn listed() -> SchemaBundle {
    let node = json!({"anyOf": [
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "integer"}, {"type": "string"}]},
            "c": {"type": "array", "items": {"$ref": "#/$defs/node"}}}},
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "boolean"}, {"type": "string"}]},
            "c": {"type": "array", "items": {"$ref": "#/$defs/node"}}}}]});
    build_schema(
        &json!({"type": "object", "properties": {"root": {"$ref": "#/$defs/node"}},
            "$defs": {"node": node}})
        .to_string(),
    )
    .unwrap()
}

/// `depth` nodes of [`overlapping`] around one another, each with an
/// object for its `v`, which no option allows.
fn wrong_at_every_level(depth: usize) -> Value {
    let mut node = json!({"v": {"x": 1}});
    for _ in 0..depth {
        node = json!({"c": node, "v": {"x": 1}});
    }
    json!({ "root": node })
}

/// `depth` nodes of [`listed`] in lists of one, each with an object for
/// its `v`, which no option allows.
fn listed_wrong_at_every_level(depth: usize) -> Value {
    let mut node = json!({"v": {"x": 1}});
    for _ in 0..depth {
        node = json!({"v": {"x": 1}, "c": [node]});
    }
    json!({ "root": node })
}

/// How many values `value` holds, itself too.
fn values(value: &Value) -> usize {
    1 + match value {
        Value::Object(members) => members.values().map(values).sum(),
        Value::Array(items) => items.iter().map(values).sum(),
        _ => 0,
    }
}

/// A wrong value at every level of recursive options that do not exclude
/// each other: each value was explained by up to 256 chains of
/// options, each explanation holding the errors of the values below it,
/// so the time grew with the depth times the values below (1.2 s a key at
/// 120 levels). Each option now explains each value once: the work for
/// each value of the document stays the same at any depth, in every
/// language (at each depth it reads: TOML and HCL read 120 levels of
/// tables as too deep).
#[test]
fn wrong_values_at_every_level_of_overlapping_options_cost_the_same_work_per_value() {
    for (what, schema, document, depths) in [
        (
            "overlapping options",
            overlapping(),
            wrong_at_every_level as fn(usize) -> Value,
            [30, 60, 120],
        ),
        (
            "options holding lists",
            listed(),
            listed_wrong_at_every_level,
            [15, 30, 60],
        ),
    ] {
        for &language in Language::ALL {
            for (index, depth) in depths.into_iter().enumerate() {
                let document = document(depth);
                let (problems, work) = checked(&schema, language, &document);
                assert_eq!(problems.len(), 1, "{what}, {language:?}: {problems:?}");
                let message = &problems[0].message;
                if index == depths.len() - 1 && !message.starts_with("/root: ") {
                    // Deeper than the language reads.
                    assert_eq!(work, 0, "{what}, {language:?}: {message}");
                    continue;
                }
                assert!(
                    message.starts_with("/root: ") && message.chars().count() < 4000,
                    "{what}, {language:?}: {message}"
                );
                let values = values(&document);
                assert!(
                    work <= WORK_PER_VALUE * values,
                    "{what}, {language:?}, depth {depth}: work {work} for {values} values"
                );
            }
        }
    }
}

/// The most work [`checked`] counts for each value of a document whose
/// values are all wrong (options tried and errors read): about 230 for
/// [`overlapping`], 20 for [`listed`]; 1000 to 2200 when each value was
/// explained by up to 256 chains of options.
const WORK_PER_VALUE: usize = 400;
