//! `ranges_of` (one read for many paths) gives what `range_of` gives for
//! each path, for every path of a set of documents in every language, their
//! lines ended by LF, CRLF or a lone CR.

use serde_json::{Value, json};

use super::super::Language;
use super::path_matrix::documents;

/// Every path of `value` (the root first), as pointers.
fn pointers(value: &Value, at: &str, out: &mut Vec<String>) {
    out.push(at.to_string());
    let child = |key: &str| format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                pointers(member, &child(key), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                pointers(item, &child(&index.to_string()), out);
            }
        }
        _ => {}
    }
}

/// Documents written by each language's serializer, and the hand-written
/// ones of the path matrix.
fn cases() -> Vec<(Language, String)> {
    let values = [
        json!({
            "web": {"hosts": ["a", "b", {"x": 1, "we/ird~k": 2}], "listen": 80, "k é😀": "v😀"},
            "list": [{"n": 1}, {"n": 2, "m": [1, 2]}],
            "empty": {},
            "ea": [],
            "s": "multi\nline"
        }),
        json!({"a": {"b": {"c": {"d": true}}}, "z": "x"}),
        json!({"servers": [{"name": "a", "port": 1}, {"name": "b", "tags": ["t", "u"]}]}),
    ];
    let mut cases = Vec::new();
    for &language in Language::ALL {
        for value in &values {
            let value = if language == Language::XML {
                json!({ "root": value })
            } else {
                value.clone()
            };
            if let Ok(text) = language.serialize(&value) {
                cases.push((language, text));
            }
        }
    }
    cases.extend(
        documents()
            .into_iter()
            .map(|(language, text)| (language, text.to_string())),
    );
    cases
}

#[test]
fn ranges_of_gives_range_of_for_every_path_and_line_ending() {
    let mut failures = Vec::new();
    for (language, lf) in cases() {
        let Ok(value) = language.parse(&lf) else {
            failures.push(format!("{language}: does not parse:\n{lf}"));
            continue;
        };
        let mut paths = Vec::new();
        pointers(&value, "", &mut paths);
        let missing: Vec<String> = paths.iter().map(|path| format!("{path}/nope")).collect();
        paths.extend(missing);
        for ending in ["\n", "\r\n", "\r"] {
            let text = lf.replace('\n', ending);
            let many = language.ranges_of(&text, paths.iter().map(String::as_str));
            for (path, many) in paths.iter().zip(many) {
                let one = language.range_of(&text, path.as_str());
                if many != one {
                    failures.push(format!(
                        "{language} {ending:?} {path}: ranges_of {many:?}, range_of {one:?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
