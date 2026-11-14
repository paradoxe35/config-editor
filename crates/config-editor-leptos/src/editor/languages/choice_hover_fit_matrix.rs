//! Hover on a key that several `oneOf`/`anyOf` options name documents
//! it by the option the hovered object fits: each list item by its own
//! values (block and one-line items), and options told apart by more than
//! a `const` (the keys only one allows, a one-value `enum`, a `minimum`
//! or `maximum`), in every language.

use super::super::completion::key_hover;
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::{Value, json};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// The description hover gives each `shared` key of `text`, in order.
fn said_at_each(
    text: &str,
    key: &str,
    bundle_schema: &str,
    language: Language,
) -> Vec<Option<String>> {
    let bundle = build_schema(bundle_schema).unwrap();
    text.lines()
        .enumerate()
        .flat_map(|(line, content)| {
            content
                .match_indices(key)
                .filter(|(at, _)| {
                    // The key itself, not its end tag or part of a word.
                    let before = content[..*at].chars().next_back();
                    !before.is_some_and(|c| c == '/' || c.is_alphanumeric())
                })
                .map(move |(at, _)| (line, at + 1))
                .collect::<Vec<_>>()
        })
        .map(|(line, column)| {
            key_hover(text, line, column, Some(&bundle), language.behavior())
                .and_then(|hover| hover.description)
        })
        .collect()
}

fn item_option(kind: &str, own: &str) -> Value {
    json!({
        "type": "object", "required": ["kind"], "additionalProperties": false,
        "properties": {
            "kind": {"const": kind},
            own: {"type": "string"},
            "shared": {"type": "string", "description": format!("{kind} shared")}
        }
    })
}

/// Each item of a list documented by its own option, the items written
/// in blocks or on one line each.
#[test]
fn each_list_item_is_documented_by_its_own_option() {
    let schema = json!({"type": "object", "properties": {"outs": {"type": "array",
        "items": {"oneOf": [item_option("file", "path"), item_option("http", "url")]}}}})
    .to_string();
    let value = json!({"outs": [{"kind": "file", "shared": "x"}, {"kind": "http", "shared": "y"}]});
    let mut texts: Vec<(Language, String)> = LANGUAGES
        .iter()
        .map(|language| (*language, language.serialize(&value).unwrap()))
        .collect();
    texts.extend([
        (
            Language::YAML,
            "outs:\n  - {kind: file, shared: x}\n  - {kind: http, shared: y}\n".to_string(),
        ),
        (
            Language::YAML,
            "outs: [{kind: file, shared: x}, {kind: http, shared: y}]\n".to_string(),
        ),
        (
            Language::JSON,
            "{\"outs\": [\n  {\"kind\": \"file\", \"shared\": \"x\"},\n  {\"kind\": \"http\", \"shared\": \"y\"}\n]}\n"
                .to_string(),
        ),
        (
            Language::TOML,
            "outs = [\n  { kind = \"file\", shared = \"x\" },\n  { kind = \"http\", shared = \"y\" },\n]\n"
                .to_string(),
        ),
        (
            Language::HCL,
            "outs = [\n  { kind = \"file\", shared = \"x\" },\n  { kind = \"http\", shared = \"y\" },\n]\n"
                .to_string(),
        ),
        (
            Language::XML,
            "<outs><kind>file</kind><shared>x</shared></outs>\n<outs><kind>http</kind><shared>y</shared></outs>\n"
                .to_string(),
        ),
    ]);
    let wanted = vec![
        Some("file shared".to_string()),
        Some("http shared".to_string()),
    ];
    let mut failures = Vec::new();
    for (language, text) in texts {
        let said = said_at_each(&text, "shared", &schema, language);
        if said != wanted {
            failures.push(format!("{language} {text:?}: said {said:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

fn host_option(name: &str, own: (&str, Value), extra: Value) -> Value {
    let mut option = json!({"type": "object", "properties": {
        "host": {"type": "string", "description": format!("{name} host")},
        own.0: own.1,
    }});
    for (key, value) in extra.as_object().unwrap() {
        option[key] = value.clone();
    }
    option
}

/// Options told apart by what only one of them allows.
#[test]
fn a_shared_key_is_documented_by_the_option_its_object_fits() {
    let string = || json!({"type": "string"});
    let cases = [
        (
            "required keys, no other keys",
            host_option(
                "A",
                ("port", string()),
                json!({"required": ["host"], "additionalProperties": false}),
            ),
            host_option(
                "B",
                ("path", string()),
                json!({"required": ["path"], "additionalProperties": false}),
            ),
            json!({"host": "h", "path": "/p"}),
        ),
        (
            "required keys",
            host_option("A", ("port", string()), json!({"required": ["host"]})),
            host_option("B", ("path", string()), json!({"required": ["path"]})),
            json!({"host": "h", "path": "/p"}),
        ),
        (
            "no other keys",
            host_option(
                "A",
                ("port", string()),
                json!({"additionalProperties": false}),
            ),
            host_option(
                "B",
                ("path", string()),
                json!({"additionalProperties": false}),
            ),
            json!({"host": "h", "path": "/p"}),
        ),
        (
            "one-value enum",
            host_option(
                "A",
                ("t", json!({"type": "string", "enum": ["a"]})),
                json!({}),
            ),
            host_option(
                "B",
                ("t", json!({"type": "string", "enum": ["b"]})),
                json!({}),
            ),
            json!({"t": "b", "host": "h"}),
        ),
        (
            "minimum and maximum",
            host_option(
                "A",
                ("n", json!({"type": "integer", "maximum": 5})),
                json!({}),
            ),
            host_option(
                "B",
                ("n", json!({"type": "integer", "minimum": 6})),
                json!({}),
            ),
            json!({"n": 9, "host": "h"}),
        ),
    ];
    let mut failures = Vec::new();
    for (name, a, b, value) in &cases {
        for combinator in ["oneOf", "anyOf"] {
            let schema =
                json!({"type": "object", "properties": {"conn": {combinator: [a, b]}}}).to_string();
            let document = json!({ "conn": value });
            for language in LANGUAGES {
                let text = language.serialize(&document).unwrap();
                let said = said_at_each(&text, "host", &schema, language);
                if said != [Some("B host".to_string())] {
                    failures.push(format!(
                        "{name} {combinator} {language} {text:?}: said {said:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
