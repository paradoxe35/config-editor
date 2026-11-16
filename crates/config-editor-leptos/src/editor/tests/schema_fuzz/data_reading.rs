//! XML and HCL documents read as data one way for every feature: the
//! codec's value is the data reading's, the reading is the language's
//! parse plus what it notes (each list of one and each scalar read
//! otherwise), and every value of the data is placed where the text
//! writes it, as problems and locks place theirs.

use serde_json::Value;

use super::documents::{Shape, document};
use super::random::Rng;
use super::reference::compile;
use super::runner::{Case, Property, Verdict, failure, run};
use super::schemas::{Draw, schema};
use crate::editor::Language;
use crate::editor::codec;
use crate::editor::data_reading;
use crate::editor::language::pointer_key;
use crate::editor::paths;

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: rng.chance(20),
        docs: false,
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    let shape = Shape {
        wrong: 5 + rng.below(30),
        secrets: false,
        every_key: false,
    };
    let doc = document(rng, &schema, shape);
    Case {
        language: if index.is_multiple_of(2) {
            Language::XML
        } else {
            Language::HCL
        },
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

/// Every scalar's pointer in `value`.
fn scalars(value: &Value, pointer: &mut String, out: &mut Vec<String>) {
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&pointer_key(key));
                scalars(child, pointer, out);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                scalars(child, pointer, out);
                pointer.truncate(length);
            }
        }
        _ => out.push(pointer.clone()),
    }
}

pub(super) fn check(case: &Case) -> Verdict {
    let Some(both) = compile(&case.schema) else {
        return Verdict::Skipped;
    };
    let language = case.language;
    let behavior = language.behavior();
    let Some(text) = both.written(language, &case.doc) else {
        return Verdict::Skipped;
    };
    if text.trim().is_empty() {
        return Verdict::Skipped;
    }
    let schema = both.read_schema();
    let (Ok(reading), Ok(parsed)) = (
        data_reading::read(behavior, &text, Some(schema)),
        behavior.parse(&text),
    ) else {
        return Verdict::Skipped;
    };
    let seen = || format!("{language}:\n{text}reads {}", reading.value());
    if codec::read(behavior, &text, Some(schema)).ok().as_ref() != Some(reading.value()) {
        return Verdict::Fails(failure(
            "the codec reads otherwise than the data reading",
            seen(),
        ));
    }
    if reading.as_parsed().as_ref().unwrap_or(reading.value()) != &parsed {
        return Verdict::Fails(failure(
            "the data reading changes the parse beyond what it notes",
            format!(
                "{}\nparsed {parsed}\nback {:?}",
                seen(),
                reading.as_parsed()
            ),
        ));
    }
    let mut pointers = Vec::new();
    scalars(reading.value(), &mut String::new(), &mut pointers);
    pointers.retain(|pointer| !pointer.is_empty());
    let placed = paths::with_spans(behavior, &text, &pointers, || {
        pointers
            .iter()
            .filter(|pointer| {
                let written = reading.written(pointer);
                let written = written.as_deref().unwrap_or(pointer);
                // The parse holds it where the reading says it is written.
                parsed.pointer(written).is_some()
                    && paths::schema_problem_span(behavior, &text, &reading, pointer, false, false)
                        .is_none()
            })
            .cloned()
            .collect::<Vec<_>>()
    });
    if let Some(pointer) = placed.first() {
        return Verdict::Fails(failure(
            "a value of the data is not placed where the text writes it",
            format!("{}\nat {pointer}", seen()),
        ));
    }
    let unwritten: Vec<&String> = pointers
        .iter()
        .filter(|pointer| {
            let written = reading.written(pointer);
            parsed
                .pointer(written.as_deref().unwrap_or(pointer))
                .is_none()
        })
        .collect();
    if let Some(pointer) = unwritten.first() {
        return Verdict::Fails(failure(
            "a value of the data has no written pointer in the parse",
            format!("{}\nat {pointer}", seen()),
        ));
    }
    Verdict::Holds
}

#[test]
fn every_feature_reads_xml_and_hcl_as_the_one_data_reading() {
    run(&Property {
        name: "data-reading",
        check_path: "super::data_reading::check",
        cases: 300,
        shrink_budget: 1500,
        draw,
        check,
    });
}
