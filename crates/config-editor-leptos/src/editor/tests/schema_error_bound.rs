//! Collecting a document's schema errors stops at a bound on every path,
//! and the problems then say they are incomplete (P5). A draft-04 schema
//! naming a key draft-07 reads otherwise (a property `id`) has no
//! explaining validator (see `diagnostics::explanation`): the plain one
//! explained a recursive `allOf` node once for every chain of members
//! above it, 3^depth times, which took seconds 12 levels down and ran
//! out of memory at 16. A 2019-09 node whose recursion runs only through
//! `$recursiveRef` did the same on every path: its sites named no
//! definition that spends the budget (see `explanation::refs`).

use serde_json::{Value, json};

use crate::editor::codec::serialize;
use crate::editor::diagnostics::{Diagnostic, MORE_PROBLEMS_CODE, validate};
use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::work_count::{Work, counted};
use crate::strings::default_strings;

/// The depths checked, in order: the first too slow fails before a
/// deeper one runs out of memory.
const DEPTHS: [usize; 4] = [6, 9, 12, 16];

/// Three members each holding the node under `c` and requiring a key no
/// level has, in draft-04, with a property `id`.
fn draft_04_all_of() -> SchemaBundle {
    let member = |key: &str| {
        json!({"type": "object", "required": [key], "properties": {
            key: {"type": "integer"}, "id": {"type": "string"},
            "c": {"$ref": "#/definitions/node"}}})
    };
    build_schema(
        &json!({"$schema": "http://json-schema.org/draft-04/schema#",
            "type": "object", "properties": {"root": {"$ref": "#/definitions/node"}},
            "definitions": {"node": {"allOf": [member("m"), member("d"), member("e")]}}})
        .to_string(),
    )
    .unwrap()
}

/// The same node in draft 2019-09, its recursion only through
/// `$recursiveRef` (whose value may only be `#`, so no rewrite of it
/// bounds the work).
fn recursive_ref_all_of() -> SchemaBundle {
    let member = |key: &str| {
        json!({"type": "object", "required": [key], "properties": {
            key: {"type": "integer"}, "id": {"type": "string"},
            "c": {"$recursiveRef": "#"}}})
    };
    build_schema(
        &json!({"$schema": "https://json-schema.org/draft/2019-09/schema",
            "$recursiveAnchor": true,
            "allOf": [member("m"), member("d"), member("e")]})
        .to_string(),
    )
    .unwrap()
}

/// `depth` levels of `{id: "x", c: …}`.
fn document(depth: usize) -> Value {
    let mut node = json!({"id": "x"});
    for _ in 0..depth {
        node = json!({"id": "x", "c": node});
    }
    json!({ "root": node })
}

fn checked(schema: &SchemaBundle, language: Language, depth: usize) -> (Vec<Diagnostic>, usize) {
    let text = serialize(language.behavior(), &document(depth), None).unwrap();
    counted(Work::ErrorCollected, || {
        validate(&text, Some(schema), None, true, language.behavior())
    })
}

#[test]
fn a_recursive_all_of_without_an_explaining_validator_stops_at_the_bound() {
    let schema = draft_04_all_of();
    let incomplete = &default_strings().problems_incomplete;
    for &language in Language::ALL {
        for depth in DEPTHS {
            // Counted, not timed: the errors collected are the work (3^depth
            // of them unbounded).
            let (problems, collected) = checked(&schema, language, depth);
            let notes: Vec<&Diagnostic> = problems
                .iter()
                .filter(|problem| problem.code.as_deref() == Some(MORE_PROBLEMS_CODE))
                .collect();
            if depth == 16 {
                // The bound was used, and the list says so.
                assert!(
                    notes.len() == 1 && &notes[0].message == incomplete,
                    "{language:?}, depth {depth}: {problems:?}"
                );
                assert!(collected > 1000, "{language:?}: {collected} errors");
            }
            assert!(
                problems.len() > 3 && collected < 200_000,
                "{language:?}, depth {depth}: {collected} errors, {problems:?}"
            );
        }
    }
}

#[test]
fn a_recursion_only_through_recursive_ref_stops_at_the_bound() {
    let schema = recursive_ref_all_of();
    let incomplete = &default_strings().problems_incomplete;
    for &language in Language::ALL {
        for depth in DEPTHS {
            let started = std::time::Instant::now();
            let text = serialize(language.behavior(), &document(depth)["root"], None).unwrap();
            let (problems, collected) = counted(Work::ErrorCollected, || {
                validate(&text, Some(&schema), None, true, language.behavior())
            });
            let elapsed = started.elapsed();
            assert!(
                elapsed < std::time::Duration::from_secs(4),
                "{language:?}, depth {depth}: {elapsed:?}"
            );
            assert!(
                problems.len() > 3 && collected < 200_000,
                "{language:?}, depth {depth}: {collected} errors, {problems:?}"
            );
            if depth == 16 {
                assert!(
                    problems.iter().any(|problem| problem.code.as_deref()
                        == Some(MORE_PROBLEMS_CODE)
                        && &problem.message == incomplete),
                    "{language:?}, depth {depth}: {problems:?}"
                );
            }
        }
    }
}

/// Under the bound, the same words as before, once each.
#[test]
fn a_small_document_without_an_explaining_validator_reads_as_before() {
    let (problems, _) = checked(&draft_04_all_of(), Language::JSON, 1);
    let messages: Vec<&str> = problems
        .iter()
        .map(|problem| problem.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "/root: \"m\" is a required property",
            "/root: \"d\" is a required property",
            "/root: \"e\" is a required property",
            "/root/c: \"m\" is a required property",
            "/root/c: \"d\" is a required property",
            "/root/c: \"e\" is a required property",
        ]
    );
}
