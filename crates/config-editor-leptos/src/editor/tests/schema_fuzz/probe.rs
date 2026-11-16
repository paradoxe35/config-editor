//! Looking into a case by hand: one printed by a run (`SCHEMA_FUZZ_TRACE`),
//! read back and measured at each depth (`PROBE=<file> cargo test
//! probe_case -- --ignored`); and a schema's fill, reading or suggestions
//! (`probe_fill`, `probe_read`, `probe_suggestions`, given in files).

use serde_json::Value;

use super::reference::compile;
use super::work::nested;
use crate::editor::Language;
use crate::editor::work_count::{Work, counted_all};

#[test]
#[ignore = "measures a case a run printed, given in PROBE"]
fn probe_case() {
    let text = file_in("PROBE");
    let (schema, rest) = text.split_once(" doc ").unwrap();
    let (doc, _) = rest.split_once(" other ").unwrap();
    let schema: Value = serde_json::from_str(schema).unwrap();
    let doc: Value = serde_json::from_str(doc).unwrap();
    let both = compile(&schema).unwrap();
    let most: usize = std::env::var("PROBE_DEPTH")
        .ok()
        .and_then(|depth| depth.parse().ok())
        .unwrap_or(8);
    for depth in 1..=most {
        for &language in Language::ALL {
            let in_list = std::env::var_os("PROBE_LIST").is_some();
            let Some(text) = both.written(language, &nested(&doc, depth, in_list)) else {
                continue;
            };
            let start = std::time::Instant::now();
            let (_, read) = counted_all(|| both.read(language, &text));
            let read_time = start.elapsed();
            let (_, valid) = counted_all(|| both.problems(language, &text));
            let busy: Vec<String> = Work::ALL
                .iter()
                .zip(read.iter().zip(valid.iter()))
                .filter(|(_, (read, valid))| **read + **valid > 100)
                .map(|(kind, (read, valid))| format!("{kind:?} {read}/{valid}"))
                .collect();
            let total = start.elapsed();
            let value = both.read(language, &text).unwrap_or(Value::Null);
            let start = std::time::Instant::now();
            let valid = both.valid(&value);
            let checked = start.elapsed();
            let errors = both.reference.iter_errors(&value).count();
            eprintln!(
                "{depth} {language}: read {read_time:?} total {total:?} {busy:?}; reference {valid} in {checked:?}, {errors} errors in {:?}",
                start.elapsed()
            );
        }
    }
}

/// The file the environment variable `name` names, read.
fn file_in(name: &str) -> String {
    std::fs::read_to_string(std::env::var(name).unwrap()).unwrap()
}

/// The schema in the file `PROBE_SCHEMA` names, and the language
/// `PROBE_LANGUAGE` names (`fallback` when unset).
fn schema_and_language(fallback: &str) -> (Value, Language) {
    let schema: Value = serde_json::from_str(&file_in("PROBE_SCHEMA")).unwrap();
    let label = std::env::var("PROBE_LANGUAGE").unwrap_or_else(|_| fallback.to_string());
    let language = *Language::ALL
        .iter()
        .find(|language| language.label() == label)
        .unwrap();
    (schema, language)
}

/// What accepting the root key `PROBE_KEY` fills, in `PROBE_LANGUAGE`,
/// under the schema in `PROBE_SCHEMA` (a file).
#[test]
#[ignore = "prints a fill given in PROBE_SCHEMA"]
fn probe_fill() {
    let (schema, language) = schema_and_language("HCL");
    let key = std::env::var("PROBE_KEY").unwrap();
    let property = &schema["properties"][&key];
    let children =
        crate::editor::completion::required_children(language.behavior(), property, &schema);
    eprintln!("{children:#?}");
}

/// How a text reads: the schema in `PROBE_SCHEMA` and the text in
/// `PROBE_TEXT` (files), in `PROBE_LANGUAGE` (a label).
#[test]
#[ignore = "reads a text given in PROBE_TEXT"]
fn probe_read() {
    let (schema, language) = schema_and_language("XML");
    let both = compile(&schema).unwrap();
    let value = both.read(language, &file_in("PROBE_TEXT"));
    eprintln!("reads {value:?}");
    eprintln!("valid {:?}", value.as_ref().map(|value| both.valid(value)));
}

/// Suggestions at the end of a text: the schema in `PROBE_SCHEMA` (a
/// file), the text in `PROBE_TEXT` (a file; else `{root: {}}` nested
/// `PROBE_DEPTH` levels in lists), in `PROBE_LANGUAGE` (a label).
#[test]
#[ignore = "runs suggestions on a case given in PROBE_SCHEMA"]
fn probe_suggestions() {
    let (schema, language) = schema_and_language("TOML");
    let both = compile(&schema).unwrap();
    let text = if std::env::var_os("PROBE_TEXT").is_some() {
        file_in("PROBE_TEXT")
    } else {
        let depth = std::env::var("PROBE_DEPTH")
            .ok()
            .and_then(|depth| depth.parse().ok())
            .unwrap_or(4);
        both.written(language, &nested(&serde_json::json!({}), depth, true))
            .unwrap()
    };
    eprintln!("{text}");
    let caret = text.encode_utf16().count();
    let offered = both.editor.suggestions(language, &text, caret);
    eprintln!("{} suggestions", offered.len());
}
