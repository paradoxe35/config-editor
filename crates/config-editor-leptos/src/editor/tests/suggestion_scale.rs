//! The work a list of suggestions costs grows with the number of values
//! (or keys) the schema offers plus the size of the document, never with
//! their product: what each suggestion needs of the text (where it starts
//! and ends, the other places of a key it renames, its docs) is read once
//! for the list. Counted (see `work_count`) for every kind of work, in
//! every language, opening the list, filtering it and renaming a key.

use serde_json::{Value, json};

use crate::editor::completion::completions_for;
use crate::editor::diagnostics::{SchemaBundle, build_schema};
use crate::editor::languages::Language;
use crate::editor::work_count::{Work, counted_all};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// A schema whose items offer `values` protocols (each documented) and
/// `values` more keys.
fn schema(values: usize) -> SchemaBundle {
    let protocols: Vec<Value> = (0..values)
        .map(|value| json!(format!("proto-{value:04}")))
        .collect();
    let docs: Vec<Value> = (0..values)
        .map(|value| json!(format!("Protocol number {value}")))
        .collect();
    let mut properties = serde_json::Map::new();
    properties.insert("name".into(), json!({ "type": "string" }));
    properties.insert(
        "protocol".into(),
        json!({ "type": "string", "enum": protocols, "x-enum-descriptions": docs }),
    );
    properties.insert("port".into(), json!({ "type": "integer" }));
    for key in 0..values {
        properties.insert(
            format!("option{key:04}"),
            json!({ "type": "string", "description": "An option" }),
        );
    }
    let schema = json!({
        "type": "object",
        "properties": {
            "tunnels": {
                "type": "array",
                "items": { "type": "object", "properties": properties }
            }
        }
    });
    build_schema(&schema.to_string()).expect("the schema builds")
}

/// A document of `items` tunnels, written in `language`.
fn document(language: Language, items: usize) -> String {
    let tunnels: Vec<Value> = (0..items)
        .map(|item| json!({ "name": format!("tunnel{item}"), "protocol": "udp", "port": 7000 + item }))
        .collect();
    language
        .behavior()
        .serialize(&json!({ "tunnels": tunnels }), None)
        .expect("the document serializes")
}

/// `text` with the middle `udp` replaced by `typed`, and the UTF-16 caret
/// after it.
fn typing_value(text: &str, typed: &str) -> (String, usize) {
    let places: Vec<usize> = text.match_indices("udp").map(|(at, _)| at).collect();
    let at = places[places.len() / 2];
    let text = format!("{}{typed}{}", &text[..at], &text[at + 3..]);
    let caret = text[..at + typed.len()].encode_utf16().count();
    (text, caret)
}

/// The UTF-16 caret at the end of the middle tunnel's `name` key.
fn on_middle_name(text: &str) -> usize {
    let places: Vec<usize> = text.match_indices("name").map(|(at, _)| at).collect();
    let at = places[places.len() / 2] + "name".len();
    text[..at].encode_utf16().count()
}

/// The work noted once per byte or character read; any other is noted
/// once per pass.
const PER_BYTE: [Work; 5] = [
    Work::OffsetSeek,
    Work::OffsetBytes,
    Work::ParsedBytes,
    Work::RowText,
    Work::CharacterStep,
];

/// Work done once per schema node a suggestion reads (a branch walk per
/// value's schema): it grows with the values, never with the document.
const PER_VALUE: [Work; 1] = [Work::BranchWalk];

/// What one way of asking for suggestions costs, each kind of work counted,
/// with how many suggestions it lists.
type Ask = fn(&SchemaBundle, Language, &str) -> usize;

fn open_values(schema: &SchemaBundle, language: Language, text: &str) -> usize {
    let (text, caret) = typing_value(text, "");
    let options = completions_for(
        &text,
        caret,
        true,
        Some(schema),
        true,
        language.behavior(),
        false,
    );
    options.len()
}

fn filter_values(schema: &SchemaBundle, language: Language, text: &str) -> usize {
    let (text, caret) = typing_value(text, "proto-0");
    let options = completions_for(
        &text,
        caret,
        false,
        Some(schema),
        true,
        language.behavior(),
        false,
    );
    options.len()
}

fn rename_key(schema: &SchemaBundle, language: Language, text: &str) -> usize {
    let caret = on_middle_name(text);
    let options = completions_for(
        text,
        caret,
        true,
        Some(schema),
        true,
        language.behavior(),
        true,
    );
    options.len()
}

#[test]
fn suggestions_cost_their_values_plus_the_document_not_their_product() {
    let (few, many) = (20, 200);
    let (short, long) = (10, 100);
    let schemas = [schema(few), schema(many)];
    let asks: [(&str, Ask); 3] = [
        ("opening the values", open_values),
        ("filtering the values", filter_values),
        ("renaming a key", rename_key),
    ];
    let mut growing = Vec::new();
    for language in LANGUAGES {
        let documents = [document(language, short), document(language, long)];
        for (ask_name, ask) in asks {
            // counts[values][size]
            let mut counts = [[[0usize; Work::ALL.len()]; 2]; 2];
            for (values, schema) in schemas.iter().enumerate() {
                for (size, text) in documents.iter().enumerate() {
                    let (listed, work) = counted_all(|| ask(schema, language, text));
                    assert!(
                        listed > 0,
                        "{} {ask_name}: lists suggestions",
                        language.label()
                    );
                    counts[values][size] = work;
                }
            }
            for (kind, work) in Work::ALL.into_iter().enumerate() {
                let at = |values: usize, size: usize| counts[values][size][kind] as i64;
                let more = |size: usize| at(1, size) - at(0, size);
                // Bytes read, and schema nodes walked, cost as much more for
                // more values in a long document as in a short one; any other
                // work is a pass over the text, which more values never add.
                let grows = if PER_BYTE.contains(&work) || PER_VALUE.contains(&work) {
                    more(1) != more(0)
                } else {
                    more(0) != 0 || more(1) != 0
                };
                if grows {
                    growing.push(format!(
                        "{} {ask_name}: {work:?} {:?}",
                        language.label(),
                        [at(0, 0), at(0, 1), at(1, 0), at(1, 1)],
                    ));
                }
            }
        }
    }
    assert!(
        growing.is_empty(),
        "work for {many} values rather than {few} grows with the document \
         ({short} items to {long}), or passes over it (counts for [{few} values in {short}, in {long}, {many} in {short}, in {long}]):\n{}",
        growing.join("\n")
    );
}
