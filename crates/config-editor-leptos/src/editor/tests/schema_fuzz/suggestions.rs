//! Suggestions accepted in a valid document add no schema problem: a key
//! suggested at the root adds none outside its own value (whose empty slot
//! is left to type), a value suggested for a field is one its schema
//! accepts; and a hover's type label names each type once. A key is
//! judged where a witness document holds it validly (its schema can be
//! met), in schemas whose defaults and examples fit.

use crate::editor::language::pointer_key;
use std::collections::{BTreeSet, HashSet};

use serde_json::{Value, json};

use super::documents::{Shape, document};
use super::list_search::{Search, unmet};
use super::random::Rng;
use super::reference::{Both, annotations_fit, compile, holds_empty};
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, schema};
use crate::editor::Language;
use crate::editor::TextPosition;

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: false,
        docs: rng.chance(60),
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    let shape = Shape {
        wrong: 3,
        secrets: false,
        every_key: false,
    };
    let doc = document(rng, &schema, shape);
    let witness = Shape {
        wrong: 0,
        every_key: true,
        ..shape
    };
    let other = document(rng, &schema, witness);
    Case {
        language: language(index),
        pick: rng.below(8),
        schema,
        doc,
        other,
    }
}

/// `text` (the document in `language`) with a new first entry being typed
/// at the root: its key `key` (the value to type at the caret), or a key
/// itself when `key` is `None`; with the caret's byte offset.
pub(super) fn with_entry(language: Language, text: &str, key: Option<&str>) -> (String, usize) {
    let (before, after) = match (language.label(), key) {
        ("JSON", _) => {
            let rest = text.trim_start().strip_prefix('{').unwrap_or("}\n");
            let empty = rest.trim_start().starts_with('}');
            let comma = if empty { "" } else { "," };
            match key {
                Some(key) => (format!("{{\n  \"{key}\": "), format!("{comma}{rest}")),
                None => ("{\n  ".to_string(), format!("{comma}{rest}")),
            }
        }
        ("XML", Some(key)) => (format!("<{key}>"), format!("</{key}>\n{text}")),
        ("XML", None) => ("<".to_string(), format!("\n{text}")),
        ("YAML", Some(key)) => (format!("{key}: "), format!("\n{text}")),
        (_, Some(key)) => (format!("{key} = "), format!("\n{text}")),
        (_, None) => (String::new(), format!("\n{text}")),
    };
    (format!("{before}{after}"), before.len())
}

/// The reference's errors of `value`, as (place, keyword), in order: the
/// first one names a failure's class, which must be the same for the same
/// errors however they were found (a schema through a format is judged
/// by its class matching the plain schema's).
fn errors(both: &Both, value: &Value) -> BTreeSet<(String, String)> {
    both.reference
        .iter_errors(value)
        .map(|error| {
            (
                error.instance_path().as_str().to_string(),
                error.kind().keyword().to_string(),
            )
        })
        .collect()
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
    let Some(read) = both.read(language, &text) else {
        return Verdict::Skipped;
    };
    if !both.valid(&read) || !annotations_fit(&case.schema) {
        return Verdict::Skipped;
    }
    if let Some(failed) = hover_labels(both, language, &text) {
        return Verdict::Fails(failed);
    }
    // Keys at the root.
    let (typed, caret) = with_entry(language, &text, None);
    let caret = typed[..caret].encode_utf16().count();
    for offered in both.editor.suggestions(language, &typed, caret) {
        // Judged where a witness (`other`) meets the key's schema.
        if offered.is_value() || !both.met(&case.other, offered.label()) {
            continue;
        }
        let (accepted, _) = offered.accept(&typed);
        let Some(after) = both.read(language, &accepted) else {
            continue;
        };
        let key = format!("/{}", pointer_key(offered.label()));
        for (at, keyword) in errors(both, &after) {
            let inside = at == key || at.starts_with(&format!("{key}/"));
            if inside && (keyword == "required" || after.pointer(&at).is_none_or(holds_empty)) {
                continue;
            }
            let own = at == key && after.pointer(&at).is_some_and(super::fill::own_list);
            // A problem at or above an empty list no list there meets is
            // that list's.
            let unmet = || unmet(both, language, (&case.schema, &case.other), &after, &at);
            let class = if inside && own {
                // Each language's insert writes a key's own list empty or
                // with its first item, the caret inside to type the items.
                format!("a suggested key writes its own list empty, the caret inside ({keyword})")
            } else if let Some(found) = inside.then(unmet).flatten() {
                match found {
                    Search::Undecided => {
                        "a suggested key writes an empty list where a bounded search finds none"
                    }
                    _ => "a suggested key writes an empty list where no list meets every schema",
                }
                .to_string()
            } else if inside {
                format!("a suggested key writes a value its schema refuses ({keyword})")
            } else {
                format!("a suggested key adds a problem elsewhere ({keyword})")
            };
            return Verdict::Fails(failure(
                class,
                format!(
                    "{language}, accepting {:?} in\n{typed}gives\n{accepted}reads {after}\n{at}: {keyword}",
                    offered.label()
                ),
            ));
        }
    }
    // Values of a root field, typed again from empty.
    let Some(map) = case.doc.as_object() else {
        return Verdict::Holds;
    };
    let scalars: Vec<&String> = map
        .iter()
        .filter(|(_, value)| !value.is_object() && !value.is_array())
        .map(|(key, _)| key)
        .collect();
    if scalars.is_empty() {
        return Verdict::Holds;
    }
    let key = scalars[case.pick % scalars.len()];
    let mut without = case.doc.clone();
    without.as_object_mut().unwrap().remove(key);
    let rest = if without.as_object().unwrap().is_empty() {
        String::new()
    } else {
        let Some(rest) = both.written(language, &without) else {
            return Verdict::Holds;
        };
        rest
    };
    let (typed, caret) = with_entry(language, &rest, Some(key));
    let caret = typed[..caret].encode_utf16().count();
    for offered in both.editor.suggestions(language, &typed, caret) {
        if !offered.is_value() {
            continue;
        }
        let (accepted, _) = offered.accept(&typed);
        let Some(after) = both.read(language, &accepted) else {
            return Verdict::Fails(failure(
                "a suggested value does not read",
                format!(
                    "{language}, accepting {:?} in\n{typed}gives\n{accepted}",
                    offered.label()
                ),
            ));
        };
        let pointer = format!("/{key}");
        let refused: Vec<(String, String)> = errors(both, &after)
            .into_iter()
            .filter(|(at, _)| *at == pointer || at.starts_with(&format!("{pointer}/")))
            .collect();
        if let Some((at, keyword)) = refused.first() {
            return Verdict::Fails(failure(
                format!("a suggested value is refused ({keyword})"),
                format!(
                    "{language}, accepting {:?} in\n{typed}gives\n{accepted}reads {after}\n{at}: {keyword}",
                    offered.label()
                ),
            ));
        }
    }
    Verdict::Holds
}

/// A hover's type label naming a type twice, on the first or last word of
/// any line.
fn hover_labels(both: &Both, language: Language, text: &str) -> Option<super::runner::Failure> {
    for (index, line) in text.lines().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let first = chars
            .iter()
            .position(|character| !character.is_whitespace());
        let last = chars
            .iter()
            .rposition(|character| !character.is_whitespace());
        for column in [first, last].into_iter().flatten() {
            let position = TextPosition {
                line: index + 1,
                column: column + 1,
            };
            let Some(docs) = both.editor.hover(language, text, position) else {
                continue;
            };
            let mut seen = HashSet::new();
            let twice = docs
                .type_label
                .split(" | ")
                .find(|name| !seen.insert(name.to_string()));
            if let Some(name) = twice {
                return Some(failure(
                    "a hover type label names a type twice",
                    format!(
                        "{language}, line {} column {}: {} names {name} twice\n{text}",
                        position.line, position.column, docs.type_label
                    ),
                ));
            }
        }
    }
    None
}

#[test]
fn accepted_suggestions_add_no_problem_and_hovers_name_types_once() {
    run(&Property {
        name: "suggestions",
        check_path: "super::suggestions::check",
        cases: 200,
        shrink_budget: 1500,
        draw,
        check,
    });
}

/// One more item of a list written by repeating its key (an XML element,
/// an HCL block) never moves the items written to other places: a list
/// whose items have places (`prefixItems`, a tuple `items`, given by the
/// property, another schema at the key or a `$ref`) takes it after its
/// items where the caret follows them, and is not offered before them.
#[test]
fn another_item_never_moves_the_items_written() {
    let scalars = json!({"a": ["x"]});
    let objects = json!({"a": [{"n": "x"}]});
    let cases = [
        (
            json!({"properties": {"a": {"type": "array"}},
                "allOf": [{"properties": {"a": {"items": {"const": 0}, "prefixItems": [{}]}}}]}),
            &scalars,
        ),
        (
            json!({"properties": {"a": {"type": "array",
                "prefixItems": [{"type": "string"}], "items": {"type": "integer"}}}}),
            &scalars,
        ),
        (
            json!({"$schema": "http://json-schema.org/draft-07/schema#",
                "properties": {"a": {"type": "array",
                "items": [{"type": "string"}], "additionalItems": {"type": "integer"}}}}),
            &scalars,
        ),
        (
            json!({"$defs": {"l": {"type": "array",
                "prefixItems": [{"type": "string"}], "items": {"type": "integer"}}},
                "properties": {"a": {"$ref": "#/$defs/l"}}}),
            &scalars,
        ),
        (
            json!({"properties": {"a": {"type": "array",
                "prefixItems": [{"type": "object", "properties": {"n": {"type": "string"}}}],
                "items": {"type": "object", "properties": {"n": {"type": "integer"}}}}}}),
            &objects,
        ),
    ];
    let mut failures = Vec::new();
    for (schema, doc) in &cases {
        let both = compile(schema).expect("the schema compiles");
        for &language in Language::ALL {
            let Some(text) = both.written(language, doc) else {
                continue;
            };
            let Some(read) = both.read(language, &text) else {
                continue;
            };
            let written = match read.get("a") {
                Some(Value::Array(items)) => items.clone(),
                Some(lone) => vec![lone.clone()],
                None => continue,
            };
            let xml = language.label() == "XML";
            let opener = if xml { "<" } else { "" };
            let before = with_entry(language, &text, None);
            let after = (format!("{text}{opener}"), text.len() + opener.len());
            for (place, (typed, caret)) in [("before", before), ("after", after)] {
                let caret = typed[..caret].encode_utf16().count();
                let offered = both.editor.suggestions(language, &typed, caret);
                let Some(another) = offered
                    .iter()
                    .find(|offered| !offered.is_value() && offered.label() == "a")
                else {
                    if place == "after" && xml {
                        failures.push(format!(
                            "{language} {schema}: `a` not offered after\n{text}"
                        ));
                    }
                    continue;
                };
                let (accepted, _) = another.accept(&typed);
                let items = both
                    .read(language, &accepted)
                    .and_then(|after| match after.get("a") {
                        Some(Value::Array(items)) => Some(items.clone()),
                        _ => None,
                    });
                if items.is_none_or(|items| !items.starts_with(&written)) {
                    failures.push(format!(
                        "{language} {schema}: accepting `a` {place} the items in\n{typed}gives\n{accepted}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
