//! A recursive node reached again for the same value (an `allOf` whose
//! members each hold the node, members naming one definition) costs work
//! linear in the document, in every language: the validator
//! explained the value once for every chain of members above it, the same
//! errors each time, so with an error at every level they grew as
//! members^depth (0.8 s at depth 11, out of memory at 16, for 2 KB).
//! Counted (see `work_count`): the errors collected for each value of the
//! document stay the same at any depth.

use serde_json::{Value, json};

use crate::editor::codec::serialize;
use crate::editor::diagnostics::{Diagnostic, validate};
use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::work_count::{Work, counted};

/// The depths measured, in order: the first that grows too much fails
/// before a deeper one runs out of memory.
const DEPTHS: [usize; 4] = [6, 9, 12, 16];

/// The most errors collected for each value: three a level here (a
/// missing key of each member); 200 and more at depth 6 when each chain
/// of members explained the value again.
const ERRORS_PER_VALUE: usize = 4;

/// Three members each holding the node under `c` (in a list of nodes
/// there when `listed`), each requiring a key no level has; under the
/// draft-07 names when `legacy`.
fn all_of(listed: bool, legacy: bool) -> SchemaBundle {
    let defs = if legacy { "definitions" } else { "$defs" };
    let link = json!({"$ref": format!("#/{defs}/node")});
    let link = if listed {
        json!({"type": "array", "items": link})
    } else {
        link
    };
    let member = |key: &str, kind: &str| {
        json!({"type": "object", "required": [key],
            "properties": {key: {"type": kind}, "c": link}})
    };
    let mut schema = json!({"type": "object", "properties": {"root": link_to(defs)},
        defs: {"node": {"allOf": [
            member("mode", "string"), member("d", "integer"), member("e", "integer")]}}});
    if legacy {
        schema["$schema"] = json!("http://json-schema.org/draft-07/schema#");
    }
    build_schema(&schema.to_string()).unwrap()
}

fn link_to(defs: &str) -> Value {
    json!({"$ref": format!("#/{defs}/node")})
}

/// `depth` levels of `{a: 1, c: …}` (`c` a list of one when `listed`).
fn document(depth: usize, listed: bool) -> Value {
    let mut node = json!({"a": 1});
    for _ in 0..depth {
        let below = if listed { json!([node]) } else { node };
        node = json!({"a": 1, "c": below});
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

/// The problems of `document` written in `language`, and the errors
/// their explanation collected.
fn checked(
    schema: &SchemaBundle,
    language: Language,
    document: &Value,
) -> (Vec<Diagnostic>, usize) {
    let text = serialize(language.behavior(), document, None).unwrap();
    counted(Work::ErrorCollected, || {
        validate(&text, Some(schema), None, true, language.behavior())
    })
}

#[test]
fn a_node_every_member_holds_collects_errors_linear_in_the_document() {
    for (what, listed, legacy) in [
        ("allOf", false, false),
        ("allOf of lists", true, false),
        ("draft-07 allOf", false, true),
    ] {
        let schema = all_of(listed, legacy);
        for &language in Language::ALL {
            for depth in DEPTHS {
                let document = document(depth, listed);
                let (problems, collected) = checked(&schema, language, &document);
                let values = values(&document);
                assert!(
                    collected <= ERRORS_PER_VALUE * values,
                    "{what}, {language:?}, depth {depth}: {collected} errors for {values} values"
                );
                assert!(
                    problems.len() > 3
                        && !problems
                            .iter()
                            .any(|problem| { problem.message.contains("x-config-editor") }),
                    "{what}, {language:?}, depth {depth}: {problems:?}"
                );
            }
        }
    }
}

/// Each level's missing keys, once each, as the first explanation says
/// them (the same words as before, without the copies).
#[test]
fn a_node_every_member_holds_names_each_missing_key_once() {
    let (problems, _) = checked(&all_of(false, false), Language::JSON, &document(1, false));
    let messages: Vec<&str> = problems
        .iter()
        .map(|problem| problem.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "/root: \"mode\" is a required property",
            "/root: \"d\" is a required property",
            "/root: \"e\" is a required property",
            "/root/c: \"mode\" is a required property",
            "/root/c: \"d\" is a required property",
            "/root/c: \"e\" is a required property",
        ]
    );
}
