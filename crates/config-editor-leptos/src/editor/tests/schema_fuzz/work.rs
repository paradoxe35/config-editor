//! Validation, suggestions and hover do work that grows with a document
//! no faster than the document, on random recursive schemas: a random
//! node schema (options or not) holding itself under `c` (or in a list
//! there), and a random value of it nested 4, 8 and 16 levels deep. Each
//! kind of work (see `work_count`) may grow at most 2.5 times when the
//! depth doubles (bytes read, against the text's bytes): super-linear
//! work like QA 37/38's SC37-2, SC38-1 or SC38-3 fails here.

use serde_json::{Value, json};

use super::documents::{Shape, document};
use super::random::Rng;
use super::reference::{Both, compile};
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, KEYS, schema};
use crate::editor::Language;
use crate::editor::TextPosition;
use crate::editor::work_count::{Work, counted_all};

/// The depths measured.
const DEPTHS: [usize; 3] = [4, 8, 16];

/// Counts this small at the last depth are no growth worth naming.
const FLOOR: usize = 64;

/// How many times longer validation may take three levels deeper, from
/// depth 6 to 9 (linear work: 1.5 times; quadratic: 2.25 times).
const TIME_RATIO: u32 = 5;

/// Kinds counted per byte, compared with the text's bytes.
const BYTE_KINDS: &[Work] = &[
    Work::ParsedBytes,
    Work::ConformedBytes,
    Work::OffsetBytes,
    Work::OffsetSeek,
    Work::CharacterStep,
    Work::RowText,
];

/// A node schema from a random object schema: as it is, or as one of two
/// or three options (`anyOf`, `oneOf`, `allOf`), each holding the node
/// under `c` (`pick` 1: a list of nodes there).
pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: false,
        docs: rng.chance(30),
        depth: 2,
    };
    let base = schema(rng, draw);
    let defs = base.get("$defs").cloned().unwrap_or(json!({}));
    let mut own = base.clone();
    own.as_object_mut().unwrap().remove("$defs");
    let pick = rng.below(2);
    let options = rng.below(4);
    let link = if pick == 1 {
        json!({"type": "array", "items": {"$ref": "#/$defs/node"}})
    } else {
        json!({"$ref": "#/$defs/node"})
    };
    let with_link = |mut object: Value| {
        object["properties"]["c"] = link.clone();
        object
    };
    let node = if options < 2 {
        with_link(own)
    } else {
        let keyword = *rng.pick(&["anyOf", "oneOf", "allOf"]);
        let mut members = vec![with_link(own)];
        for _ in 1..options {
            let other = schema(rng, Draw { depth: 1, ..draw });
            let mut other = other;
            other.as_object_mut().unwrap().remove("$defs");
            members.push(with_link(other));
        }
        json!({ keyword: members })
    };
    let mut defs = defs;
    defs["node"] = node;
    let schema = json!({"type": "object", "properties": {"root": {"$ref": "#/$defs/node"}},
        "$defs": defs});
    let template_schema = json!({"$ref": "#/$defs/node", "$defs": schema["$defs"].clone()});
    let shape = Shape {
        wrong: 10 + rng.below(20),
        secrets: false,
        every_key: false,
    };
    let mut doc = document(rng, &template_schema, shape);
    doc.as_object_mut().unwrap().remove("c");
    Case {
        language: language(index),
        pick,
        schema,
        doc,
        other: Value::Null,
    }
}

/// `{root: …}` with `template` nested `depth` levels under `c`.
pub(super) fn nested(template: &Value, depth: usize, in_list: bool) -> Value {
    let mut node = template.clone();
    for _ in 0..depth {
        let mut level = template.clone();
        if let Value::Object(map) = &mut level {
            map.insert("c".to_string(), if in_list { json!([node]) } else { node });
        }
        node = level;
    }
    json!({ "root": node })
}

fn nodes(value: &Value) -> usize {
    1 + match value {
        Value::Array(items) => items.iter().map(nodes).sum(),
        Value::Object(map) => map.values().map(nodes).sum(),
        _ => 0,
    }
}

/// The work of each operation on `text`: validation, suggestions at three
/// carets (after the last key written, at the end of its line, at the end
/// of the text) and hover on the last key.
fn operations(both: &Both, language: Language, text: &str) -> Vec<(&'static str, Vec<usize>)> {
    let utf16 = |byte: usize| text[..byte].encode_utf16().count();
    let last_key = KEYS
        .iter()
        .chain(&["c"])
        .filter_map(|key| text.rfind(key).map(|at| (at, at + key.len())))
        .max()
        .unwrap_or((0, 0));
    let line_end = text[last_key.1..]
        .find('\n')
        .map_or(text.len(), |end| last_key.1 + end);
    let line = text[..last_key.0].matches('\n').count() + 1;
    let column = text[..last_key.0]
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count()
        + 1;
    let mut out = Vec::new();
    let trace = std::env::var_os("SCHEMA_FUZZ_TRACE").is_some();
    let all = |what: &str, run: &dyn Fn()| {
        if trace {
            eprintln!("  {what}, {} bytes", text.len());
        }
        counted_all(run).1.to_vec()
    };
    out.push((
        "validate",
        all("validate", &|| drop(both.problems(language, text))),
    ));
    for (what, caret) in [
        ("key suggestions", last_key.1),
        ("line-end suggestions", line_end),
        ("end suggestions", text.len()),
    ] {
        let caret = utf16(caret);
        out.push((
            what,
            all(what, &|| {
                drop(both.editor.suggestions(language, text, caret))
            }),
        ));
    }
    out.push((
        "hover",
        all("hover", &|| {
            drop(
                both.editor
                    .hover(language, text, TextPosition { line, column }),
            );
        }),
    ));
    out
}

/// Validation timed at depths 6 and 9 before the deeper ones run: the
/// validator's errors, which the editor's validation reads (no count notes
/// them), may grow as options^depth and run out of memory at depth 16.
/// Linear work takes 1.5 times as long three levels down; exponential
/// work over two options 8 times, over three 27 times.
fn validation_blows(both: &Both, case: &Case) -> Option<super::runner::Failure> {
    let text =
        |depth: usize| both.written(case.language, &nested(&case.doc, depth, case.pick == 1));
    let (shallow, deep) = (text(6)?, text(9)?);
    // Both depths in turn, the fastest of three each: a busy stretch slows
    // both alike.
    let growth = crate::editor::test_timing::growth(3, (&shallow, &deep), |text: &String| {
        both.problems(case.language, text)
    });
    (growth.ratio() > f64::from(TIME_RATIO)).then(|| {
        failure(
            "validate: its time grows faster than the document",
            format!("{}: depth 6, then 9: {growth}\n{deep}", case.language),
        )
    })
}

pub(super) fn check(case: &Case) -> Verdict {
    let Some(both) = compile(&case.schema) else {
        return Verdict::Skipped;
    };
    if !case.doc.is_object() {
        return Verdict::Skipped;
    }
    let language = case.language;
    if let Some(failed) = validation_blows(&both, case) {
        return Verdict::Fails(failed);
    }
    let mut measured = Vec::new();
    for depth in DEPTHS {
        let doc = nested(&case.doc, depth, case.pick == 1);
        let Some(text) = both.written(language, &doc) else {
            return Verdict::Skipped;
        };
        if both.read(language, &text).is_none() {
            return Verdict::Skipped;
        }
        let size = (nodes(&doc), text.len());
        let work = operations(&both, language, &text);
        // Two levels of options doubling at each: stop before the next.
        let blown = measured
            .last()
            .is_some_and(|(_, before): &(_, Vec<(&str, Vec<usize>)>)| {
                before.iter().zip(&work).any(|((_, before), (_, now))| {
                    let (before, now): (usize, usize) = (before.iter().sum(), now.iter().sum());
                    now > 40 * before.max(FLOOR)
                })
            });
        measured.push((size, work));
        if blown {
            break;
        }
    }
    let [.., (small_size, small), (large_size, large)] = &measured[..] else {
        return Verdict::Skipped;
    };
    for ((operation, before), (_, after)) in small.iter().zip(large) {
        for (index, kind) in Work::ALL.iter().enumerate() {
            let (before, after) = (before[index], after[index]);
            if after < FLOOR {
                continue;
            }
            let (from, to) = if BYTE_KINDS.contains(kind) {
                (small_size.1, large_size.1)
            } else {
                (small_size.0, large_size.0)
            };
            // 2.5 times when the size doubles (in proportion otherwise).
            if after * 2 * from > 5 * before.max(FLOOR / 4) * to {
                return Verdict::Fails(failure(
                    format!("{operation}: {kind:?} grows faster than the document"),
                    format!(
                        "{language}, sizes (values, bytes) {small_size:?} -> {large_size:?}: {kind:?} {before} -> {after}"
                    ),
                ));
            }
        }
    }
    Verdict::Holds
}

#[test]
fn work_on_random_recursive_schemas_grows_with_the_document() {
    run(&Property {
        name: "work",
        check_path: "super::work::check",
        cases: 12,
        shrink_budget: 80,
        draw,
        check,
    });
}
