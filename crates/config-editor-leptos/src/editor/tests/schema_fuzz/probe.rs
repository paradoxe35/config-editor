//! A case printed by a run (`SCHEMA_FUZZ_TRACE`), read back and measured
//! at each depth: `PROBE=<file> cargo test probe_case -- --ignored`.

use serde_json::Value;

use super::reference::compile;
use super::work::nested;
use crate::editor::Language;
use crate::editor::work_count::{Work, counted_all};

#[test]
#[ignore = "measures a case a run printed, given in PROBE"]
fn probe_case() {
    let text = std::fs::read_to_string(std::env::var("PROBE").unwrap()).unwrap();
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
