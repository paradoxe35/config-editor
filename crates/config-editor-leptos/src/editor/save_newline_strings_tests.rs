//! M23-E4: strings of line breaks only (`"\n"`), which no YAML block
//! scalar holds (`|+` over blank lines reads back as `""`): YAML writes
//! them double-quoted, so serializing, converting, showing and saving them
//! keeps them.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use super::save_matrix::save_value;
use serde_json::{Value, json};

const STRINGS: [&str; 4] = ["\n", "\n\n", "\r\n", "\n\n\n"];

#[test]
fn yaml_writes_strings_of_line_breaks_double_quoted() {
    let mut failures = Vec::new();
    let yaml = Language::YAML.behavior();
    for string in STRINGS {
        let value = json!({"k": string, "l": [string, "a"], "m": {"n": string}});
        match codec::serialize(yaml, &value, None) {
            Ok(text) if yaml.parse(&text).ok() == Some(value.clone()) && !text.contains('|') => {}
            other => failures.push(format!("serialize {string:?}: {other:?}")),
        }
        let json = serde_json::to_string(&value).unwrap();
        match Language::JSON.convert(&json, Language::YAML) {
            Ok(converted) if converted.losses.is_empty() => {}
            other => failures.push(format!("convert {string:?}: {other:?}")),
        }
        // A JSON host shown as YAML, and saved after another edit.
        let bridge = Bridge::new(Language::JSON, Language::YAML, None).unwrap();
        let host = format!("{{\n  \"b\": 1,\n  \"a\": {}\n}}\n", Value::from(string));
        match bridge.to_edit(&host) {
            Ok(shown) if shown.losses.is_empty() && !shown.text.contains('|') => {
                let edited = shown.text.replace("b: 1", "b: 10");
                match bridge.to_host(&edited, &host) {
                    Ok(saved) if saved == host.replace("\"b\": 1", "\"b\": 10") => {}
                    other => failures.push(format!("JSON host saved {string:?}: {other:?}")),
                }
            }
            other => failures.push(format!("JSON host shown {string:?}: {other:?}")),
        }
        // A YAML host given the string from another view.
        for edit in [Language::JSON, Language::TOML, Language::HCL] {
            let change = |value: &mut Value| value["k"] = json!(string);
            let expected = format!("# c\nk: {} # k\nn: 1\n", Value::from(string));
            match save_value(Language::YAML, edit, "# c\nk: a # k\nn: 1\n", &change) {
                Ok(saved) if saved == expected => {}
                other => failures.push(format!("YAML host via {edit} {string:?}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
