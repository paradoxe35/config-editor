//! The editor's problems agree with the reference validator: none on a
//! document the `jsonschema` crate finds valid (as the editor reads it),
//! at least one on a document it finds invalid; and writing a document in
//! a language changes its verdict only where XML text is typed by the
//! schema (a valid document still reads valid there, unless it holds what
//! XML cannot write).

use serde_json::{Value, json};

use super::documents::{Shape, document};
use super::random::Rng;
use super::reference::{Both, compile, messages, same, xml_cannot_hold};
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, schema};
use crate::editor::Language;

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: rng.chance(30),
        docs: rng.chance(30),
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    let shape = Shape {
        wrong: 5 + rng.below(20),
        secrets: false,
        every_key: false,
    };
    let doc = document(rng, &schema, shape);
    Case {
        language: language(index),
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

pub(super) fn check(case: &Case) -> Verdict {
    compile(&case.schema).map_or(Verdict::Skipped, |both| check_with(&both, case))
}

/// The check, with the editor's schema from `both` (`case.schema` given
/// as JSON Schema or through a schema format) and its reference.
pub(super) fn check_with(both: &Both, case: &Case) -> Verdict {
    let language = case.language;
    let Some(text) = both.written(language, &case.doc) else {
        return Verdict::Skipped;
    };
    // An empty document has no problems (the editor's decision).
    if text.trim().is_empty() {
        return Verdict::Skipped;
    }
    let Some(read) = both.read(language, &text) else {
        return Verdict::Fails(failure(
            "a written document does not read",
            format!("{language}:\n{text}"),
        ));
    };
    let problems = both.problems(language, &text);
    let valid = both.valid(&read);
    let seen = || {
        format!(
            "{language}:\n{text}reads {read}\nreference: {}\nproblems:\n{}",
            if valid { "valid" } else { "invalid" },
            messages(&problems)
        )
    };
    if valid && !problems.is_empty() {
        return Verdict::Fails(failure("a problem on a valid document", seen()));
    }
    if !valid && problems.is_empty() {
        return Verdict::Fails(failure("no problem on an invalid document", seen()));
    }
    let valid_written = both.valid(&case.doc);
    if language == Language::XML {
        if valid_written && !valid && !xml_cannot_hold(&case.doc) {
            return Verdict::Fails(failure(
                "XML reads a valid document as an invalid one",
                format!("{}\nwritten from {}", seen(), case.doc),
            ));
        }
    } else if !same(&read, &case.doc) && valid_written != valid {
        return Verdict::Fails(failure(
            "writing a document changes its verdict",
            format!("{}\nwritten from {}", seen(), case.doc),
        ));
    }
    Verdict::Holds
}

#[test]
fn random_documents_get_problems_exactly_when_the_reference_finds_them_invalid() {
    run(&Property {
        name: "validation",
        check_path: "super::validation::check",
        cases: 400,
        shrink_budget: 1500,
        draw,
        check,
    });
}

/// A field the base properties only describe, typed
/// by an `allOf`, written as XML text.
#[test]
fn a_field_typed_in_an_all_of_reads_as_its_type_in_xml() {
    let case = Case {
        language: Language::XML,
        pick: 0,
        schema: json!({"type": "object", "properties": {"port": {"description": "d"}},
            "allOf": [{"properties": {"port": {"type": "integer"}}}]}),
        doc: json!({"port": 8}),
        other: Value::Null,
    };
    assert!(matches!(check(&case), Verdict::Holds));
}
