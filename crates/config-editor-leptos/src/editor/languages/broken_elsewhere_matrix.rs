//! The keys offered inside an object follow the `oneOf`/`anyOf` option
//! (or `if`/`then`) its values match while the document has a syntax
//! error elsewhere, in every language: what still parses around the caret
//! decides it. `|` marks the caret.

use super::super::schema::Schema;
use super::Language;

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

#[test]
fn nested_keys_follow_the_matched_option_with_an_error_elsewhere() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for key in ["any", "cond"] {
        for (language, marked) in CASES {
            let marked = marked.replace("KEY", key);
            let cursor = marked.find('|').unwrap();
            let text = marked.replace('|', "");
            assert!(
                language.behavior().parse(&text).is_err(),
                "{language}: the case has a syntax error\n{text}"
            );
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
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
