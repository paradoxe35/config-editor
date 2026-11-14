//! A list item a suggestion writes in front of items already written gets
//! the separator between it and the next one, on the item's line or the
//! next, in every language with inline lists: the list still reads, one
//! item longer.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::Value;

const SCHEMA: &str = r#"{"type":"object","properties":{"spec":{"type":"object","properties":{
    "containers":{"type":"array","items":{"type":"object","required":["name"],
        "properties":{"name":{"type":"string"}}}},
    "tags":{"type":"array","items":{"enum":["x","y"]}}}}}}"#;

const CASES: &[(Language, &str)] = &[
    (
        Language::JSON,
        "{\"spec\": {\"containers\": [\n  |\n  {\"name\": \"a\"}\n]}}\n",
    ),
    (
        Language::JSON,
        "{\"spec\": {\"containers\": [\n  |\n  {\"name\": \"a\"},\n  {\"name\": \"b\"}\n]}}\n",
    ),
    (
        Language::JSON,
        "{\"spec\": {\"containers\": [| {\"name\": \"a\"}]}}\n",
    ),
    (
        Language::HCL,
        "spec {\n  containers = [\n    |\n    { name = \"a\" }\n  ]\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  containers = [\n    |\n    { name = \"a\" },\n  ]\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  containers = [| { name = \"a\" }]\n}\n",
    ),
    (
        Language::TOML,
        "[spec]\ncontainers = [\n  |\n  { name = \"a\" },\n]\n",
    ),
    (
        Language::TOML,
        "[spec]\ncontainers = [| { name = \"a\" }]\n",
    ),
    (
        Language::YAML,
        "spec:\n  containers: [\n    |\n    {name: a}\n  ]\n",
    ),
    (Language::YAML, "spec:\n  containers: [| {name: a}]\n"),
];

/// Values of a list of scalars, in front of one written: `y` before `x`.
const VALUES: &[(Language, &str)] = &[
    (
        Language::JSON,
        "{\"spec\": {\"tags\": [\n  |\n  \"x\"\n]}}\n",
    ),
    (Language::JSON, "{\"spec\": {\"tags\": [| \"x\"]}}\n"),
    (
        Language::HCL,
        "spec {\n  tags = [\n    |\n    \"x\"\n  ]\n}\n",
    ),
    (Language::HCL, "spec {\n  tags = [| \"x\"]\n}\n"),
    (Language::TOML, "[spec]\ntags = [\n  |\n  \"x\",\n]\n"),
    (Language::TOML, "[spec]\ntags = [| \"x\"]\n"),
    (Language::YAML, "spec:\n  tags: [\n    |\n    x\n  ]\n"),
    (Language::YAML, "spec:\n  tags: [| x]\n"),
    // A block list takes no separator.
    (Language::YAML, "spec:\n  tags:\n    - |\n    - x\n"),
];

#[test]
fn a_value_written_in_front_of_values_is_separated_from_them() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, marked) in VALUES {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let options = completions(&source, at, true, Some(&bundle), true, language.behavior());
        let Some(option) = options.iter().find(|option| option.label == "y") else {
            let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
            failures.push(format!("{language} {marked:?}: offered {labels:?}"));
            continue;
        };
        let (text, _, _) = apply_completion(&source, option);
        let read = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema))
            .ok()
            .and_then(|read| read.pointer("/spec/tags").cloned());
        if read != Some(serde_json::json!(["y", "x"])) {
            failures.push(format!("{language} {marked:?}: {text:?} reads {read:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn an_item_written_in_front_of_items_is_separated_from_them() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, marked) in CASES {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let options = completions(&source, at, true, Some(&bundle), true, language.behavior());
        let Some(option) = options.iter().find(|option| option.label == "name") else {
            let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
            failures.push(format!("{language} {marked:?}: offered {labels:?}"));
            continue;
        };
        let (text, _, _) = apply_completion(&source, option);
        let items = super::super::codec::read(language.behavior(), &text, Some(&bundle.schema))
            .ok()
            .and_then(|read| {
                read.pointer("/spec/containers")
                    .and_then(Value::as_array)
                    .map(Vec::len)
            });
        let wanted = source.matches("name").count() + 1;
        if items != Some(wanted) {
            failures.push(format!(
                "{language} {marked:?}: {text:?} reads {items:?} items"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
