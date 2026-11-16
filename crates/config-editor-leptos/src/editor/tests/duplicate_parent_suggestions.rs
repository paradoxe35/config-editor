//! Keys typed into an object written a second time (a key the text holds
//! twice, so it does not parse) are those the second one lacks: the read
//! around the caret may leave a broken line blank, but never the line that
//! opens the caret's object, which would read the first one as the caret's
//! and leave out the keys only it has.

use crate::editor::languages::Language;
use crate::editor::schema::Schema;

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "pool": {"type": "object", "properties": {
        "min_idle": {"type": "integer"},
        "max_idle": {"type": "integer"}}},
    "other": {"type": "string"}
}}"#;

/// Per language: `pool` with `max_idle`, another key, then `pool` again
/// with the caret (`|`) in it, `max_` typed. Languages that read a block
/// written twice as a list of them (HCL, XML) are not among them.
const CASES: [(Language, &str); 3] = [
    (
        Language::YAML,
        "pool:\n  max_idle: 64\nother: x\npool:\n  max_|\n",
    ),
    (
        Language::JSON,
        "{\n  \"pool\": {\n    \"max_idle\": 64\n  },\n  \"other\": \"x\",\n  \"pool\": {\n    \"max_|\"\n  }\n}\n",
    ),
    (
        Language::TOML,
        "[pool]\nmax_idle = 64\n\n[other]\n\n[pool]\nmax_|\n",
    ),
];

#[test]
fn an_object_written_twice_offers_the_keys_its_second_copy_lacks() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, marked) in CASES {
        let cursor = marked.find('|').unwrap();
        let text = marked.replace('|', "");
        let cursor = text[..cursor].encode_utf16().count();
        let labels: Vec<String> = schema
            .suggestions(language, &text, cursor)
            .iter()
            .map(|suggestion| suggestion.label().to_string())
            .collect();
        if !labels.iter().any(|label| label.contains("max_idle")) {
            failures.push(format!("{language} offers {labels:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
