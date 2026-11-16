//! The keys offered inside an object follow the `oneOf`/`anyOf` option
//! (or `if`/`then`) its values match while the document has a syntax
//! error elsewhere, in every language: what still parses around the caret
//! decides it. `|` marks the caret.

use super::super::schema::Schema;
use super::Language;
use super::harness::{At, completing, report, write};
use serde_json::json;

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "any": {"anyOf": [
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "a"},
            "sub": {"type": "object", "properties": {"sa": {"type": "string"}}}}},
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "b"},
            "sub": {"type": "object", "properties": {"sb": {"type": "string"}}}}}
    ]},
    "cond": {"type": "object", "properties": {"kind": {"type": "string"}},
        "if": {"properties": {"kind": {"const": "b"}}},
        "then": {"properties": {"sub": {"type": "object", "properties": {"sb": {"type": "string"}}}}},
        "else": {"properties": {"sub": {"type": "object", "properties": {"sa": {"type": "string"}}}}}},
    "broken": {}
}}"#;

/// Documents with the caret in `<key>.sub` of an object whose `kind` is
/// `b`, and a syntax error after it (`broken`); `KEY` stands for the key.
const CASES: &[(Language, &str)] = &[
    (
        Language::YAML,
        "KEY:\n  kind: b\n  sub:\n    |\nbroken: [\n",
    ),
    (Language::YAML, "KEY: {kind: b, sub: {|}}\nbroken: [\n"),
    (
        Language::JSON,
        r#"{"KEY": {"kind": "b", "sub": {"|"}}, "broken": }"#,
    ),
    (
        Language::JSON,
        "{\n  \"KEY\": {\n    \"kind\": \"b\",\n    \"sub\": {\n      \"|\"\n    }\n  },\n  \"broken\": \n}\n",
    ),
    (
        Language::TOML,
        "[KEY]\nkind = \"b\"\n[KEY.sub]\n|\n[broken\n",
    ),
    (
        Language::TOML,
        "KEY = { kind = \"b\", sub = { | } }\nbroken = \n",
    ),
    (
        Language::HCL,
        "KEY {\n  kind = \"b\"\n  sub {\n    |\n  }\n}\nbroken = \n",
    ),
    (
        Language::HCL,
        "KEY = { kind = \"b\", sub = { | } }\nbroken = \n",
    ),
    (
        Language::XML,
        "<KEY><kind>b</kind><sub><|</sub></KEY><broken></brokn>\n",
    ),
    (
        Language::XML,
        "<KEY>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</KEY>\n<broken>\n",
    ),
];

/// The caret on a new entry of `<key>.sub`, over lines and on one line, in
/// every language that suggests keys, with a syntax error after it: a
/// `broken` object written after it, cut in the middle of its list.
fn written_cases(key: &str, failures: &mut Vec<String>) -> Vec<(Language, String)> {
    let document = json!({ key: {"kind": "b", "sub": {}}, "broken": {"list": [1, 22222]} });
    let object = format!("/{key}/sub");
    let at = At::NewKey {
        object: &object,
        typed: "",
    };
    let mut cases = Vec::new();
    for language in completing() {
        for one_line in [
            &["/broken/list"][..],
            &[object.as_str(), "/broken/list"][..],
        ] {
            match write(language, &document, one_line, at) {
                // Cut where the list's last item starts: a list left open.
                Ok(written) => match written.marked().rsplit_once("22222") {
                    Some((cut, _)) if cut.contains('|') => cases.push((language, cut.to_string())),
                    _ => failures.push(format!(
                        "{language}: the list is not after the caret\n{}",
                        written.text
                    )),
                },
                Err(error) => failures.push(error),
            }
        }
    }
    cases
}

#[test]
fn nested_keys_follow_the_matched_option_with_an_error_elsewhere() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for key in ["any", "cond"] {
        let mut cases: Vec<(Language, String)> = CASES
            .iter()
            .map(|(language, marked)| (*language, marked.replace("KEY", key)))
            .collect();
        cases.extend(written_cases(key, &mut failures));
        for (language, marked) in &cases {
            let cursor = marked.find('|').unwrap();
            let text = marked.replacen('|', "", 1);
            if language.behavior().parse(&text).is_ok() {
                failures.push(format!("{language}: the case has no syntax error\n{text}"));
                continue;
            }
            let cursor = text[..cursor].encode_utf16().count();
            let labels: Vec<String> = schema
                .suggestions(*language, &text, cursor)
                .iter()
                .map(|suggestion| suggestion.label().to_string())
                .collect();
            if labels != ["sb"] {
                failures.push(format!("{language} {marked}\n{labels:?}"));
            }
        }
    }
    report(&failures);
}
