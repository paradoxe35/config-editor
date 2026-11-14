//! A map's last entry deleted in a YAML view leaves `deps:` with nothing
//! under it, which reads as null: the save keeps the empty map the host
//! held a map in, in every host (TOML and XML waited on "no null value",
//! JSON and HCL saved a null for the map), and a null the user writes
//! stays one where the host can hold it.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::report;
use serde_json::{Value, json};

/// `host_text` saved after each line of its YAML view holding the first
/// text of one of `edits` is replaced by its second (removed when empty).
fn saved_after(host: Language, host_text: &str, edits: &[(&str, &str)]) -> Result<String, String> {
    let bridge = Bridge::new(host, Language::YAML, None).ok_or("same language")?;
    let shown = bridge
        .to_edit(host_text)
        .map_err(|error| error.to_string())?
        .text;
    let edited: String = shown
        .split_inclusive('\n')
        .map(
            |shown_line| match edits.iter().find(|(line, _)| shown_line.contains(line)) {
                Some((_, with)) => with.to_string(),
                None => shown_line.to_string(),
            },
        )
        .collect();
    bridge
        .to_host(&edited, host_text)
        .map_err(|error| format!("{error:?}"))
}

/// Each host with a map `deps` holding one entry `x`, and the pointer of
/// `deps` in it.
const HOSTS: [(Language, &str, &str); 4] = [
    (
        Language::JSON,
        "{\n  \"a\": 1,\n  \"deps\": {\n    \"x\": 1\n  }\n}\n",
        "/deps",
    ),
    (Language::TOML, "a = 1\n\n[deps]\nx = 1\n", "/deps"),
    (Language::HCL, "a = 1\n\ndeps {\n  x = 1\n}\n", "/deps"),
    (
        Language::XML,
        "<r>\n  <a>1</a>\n  <deps>\n    <x>1</x>\n  </deps>\n</r>\n",
        "/r/deps",
    ),
];

#[test]
fn a_map_whose_last_entry_is_deleted_stays_an_empty_map() {
    let mut failures = Vec::new();
    for (host, text, pointer) in HOSTS {
        match saved_after(host, text, &[("x: 1", "")]) {
            Ok(saved) => {
                let read = host
                    .parse(&saved)
                    .map(|value| value.pointer(pointer).cloned());
                if !matches!(&read, Ok(Some(map)) if *map == json!({})) {
                    failures.push(format!("{host}: {read:?}\n{saved}"));
                }
            }
            Err(error) => failures.push(format!("{host}: {error}")),
        }
    }
    report(failures);
}

/// A null typed over the map stays a null where the host holds one, and
/// waits where it cannot.
#[test]
fn a_null_written_over_a_map_stays_null() {
    for written in ["null", "~"] {
        let line = format!("deps: {written}\n");
        let edits = [("deps:", line.as_str()), ("x: 1", "")];
        let saved = saved_after(Language::JSON, HOSTS[0].1, &edits);
        let read = saved
            .as_deref()
            .ok()
            .and_then(|saved| Language::JSON.parse(saved).ok());
        assert_eq!(
            read.as_ref().and_then(|value| value.pointer("/deps")),
            Some(&Value::Null),
            "{written}: {saved:?}"
        );
        let toml = saved_after(Language::TOML, HOSTS[1].1, &edits);
        assert!(
            toml.as_ref().is_err_and(|error| error.contains("no null")),
            "{written}: {toml:?}"
        );
    }
}
