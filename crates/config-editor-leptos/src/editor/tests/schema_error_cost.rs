//! A wrong value deep in a recursive `anyOf` costs work linear in its
//! depth, in every language (SC37-2): the explanation of a value that
//! matches none of the options read each option's errors again for each
//! option above it (twice per level for a rule tree whose options exclude
//! each other), and the validator kept a value's errors once for every
//! chain of options above it (three per level with options that do not),
//! which ran out of memory 15 levels down. Counted (see `work_count`):
//! the work two more levels add stays the same, deeper down.

use serde_json::{Value, json};

use crate::editor::codec::serialize;
use crate::editor::diagnostics::{Diagnostic, SchemaBundle, build_schema, validate};
use crate::editor::languages::Language;
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
