//! A key already written is not offered again, whatever its kind (a
//! scalar, an object, a list of scalars, a list of objects), in every
//! language; except a list the language writes by repeating its items
//! (XML elements, HCL blocks), where accepting adds one more item written
//! the same way. A list of objects written whole in HCL (`containers =
//! [ … ]`) is no such list.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::diagnostics::build_schema;
use super::Language;
use serde_json::Value;

const SCHEMA: &str = r#"{"type":"object","properties":{"spec":{"type":"object","properties":{
    "containers":{"type":"array","items":{"type":"object","required":["name"],
        "properties":{"name":{"type":"string"},"image":{"type":"string"}}}},
    "tags":{"type":"array","items":{"type":"string"}},
    "meta":{"type":"object","properties":{"a":{"type":"string"}}},
    "other":{"type":"integer"}}}}}"#;

/// Each case: the document with the caret on a new line in `spec`, and
/// how many `containers` items accepting the offered `containers` leaves
/// (`None`: not offered).
const CASES: &[(Language, &str, Option<usize>)] = &[
    // HCL: written whole, on one line and over lines: not offered.
    (
        Language::HCL,
        "spec {\n  containers = [{ name = \"web\" }]\n  |\n}\n",
        None,
    ),
    (
        Language::HCL,
        "spec {\n  containers = [\n    {\n      name = \"web\"\n    }\n  ]\n  |\n}\n",
        None,
    ),
    // HCL: as blocks, once and twice: one more block.
    (
        Language::HCL,
        "spec {\n  containers {\n    name = \"web\"\n  }\n  |\n}\n",
        Some(2),
    ),
    (
        Language::HCL,
        "spec {\n  containers {\n    name = \"a\"\n  }\n  containers {\n    name = \"b\"\n  }\n  |\n}\n",
        Some(3),
    ),
    // HCL: the caret before the blocks.
    (
        Language::HCL,
        "spec {\n  |\n  containers {\n    name = \"web\"\n  }\n}\n",
        Some(2),
    ),
    // XML: one more element.
    (
        Language::XML,
        "<spec>\n  <containers>\n    <name>web</name>\n  </containers>\n  <|\n</spec>\n",
        Some(2),
    ),
    // The others write a list whole: not offered.
    (
        Language::YAML,
        "spec:\n  containers:\n    - name: web\n  |\n",
        None,
    ),
    (
        Language::JSON,
        "{\n  \"spec\": {\n    \"containers\": [{\"name\": \"web\"}],\n    |\n  }\n}\n",
        None,
    ),
    (
        Language::TOML,
        "[spec]\ncontainers = [{ name = \"web\" }]\n|\n",
        None,
    ),
];

/// Other kinds of key, written: never offered again.
const WRITTEN: &[(Language, &str)] = &[
    (
        Language::HCL,
        "spec {\n  other = 1\n  tags = [\"a\"]\n  meta = { a = \"x\" }\n  |\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  other = 1\n  tags = [\"a\"]\n  meta {\n    a = \"x\"\n  }\n  |\n}\n",
    ),
    (
        Language::YAML,
        "spec:\n  other: 1\n  tags: [a]\n  meta: {a: x}\n  |\n",
    ),
    (
        Language::JSON,
        "{\"spec\": {\n  \"other\": 1, \"tags\": [\"a\"], \"meta\": {\"a\": \"x\"},\n  |\n}}\n",
    ),
    (
        Language::TOML,
        "[spec]\nother = 1\ntags = [\"a\"]\nmeta = { a = \"x\" }\n|\n",
    ),
    (
        Language::XML,
        "<spec>\n  <other>1</other>\n  <meta><a>x</a></meta>\n  <|\n</spec>\n",
    ),
];

fn labels_and_accept(
    language: Language,
    marked: &str,
    label: &str,
) -> (Vec<String>, Option<String>) {
    let bundle = build_schema(SCHEMA).unwrap();
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(&source, at, true, Some(&bundle), true, language.behavior());
    let labels = options.iter().map(|option| option.label.clone()).collect();
    let accepted = options
        .iter()
        .find(|option| option.label == label)
        .map(|option| apply_completion(&source, option).0);
    (labels, accepted)
}

#[test]
fn a_written_list_of_objects_is_offered_again_only_where_items_repeat() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, marked, wanted) in CASES {
        let (labels, accepted) = labels_and_accept(*language, marked, "containers");
        let items = accepted.as_ref().map(|text| {
            super::super::codec::read(language.behavior(), text, Some(&bundle.schema))
                .ok()
                .and_then(|read| {
                    read.pointer("/spec/containers")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                })
        });
        if items != wanted.map(Some) {
            failures.push(format!(
                "{language} {marked:?}: offered {labels:?}, accepted {accepted:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_written_key_of_any_other_kind_is_not_offered_again() {
    let mut failures = Vec::new();
    for (language, marked) in WRITTEN {
        let (labels, _) = labels_and_accept(*language, marked, "");
        let repeated: Vec<&String> = labels
            .iter()
            .filter(|label| {
                ["other", "tags", "meta"].contains(&label.as_str())
                    && marked.contains(label.as_str())
            })
            .collect();
        if !repeated.is_empty() || !labels.iter().any(|label| label == "containers") {
            failures.push(format!("{language} {marked:?}: offered {labels:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A key a `not` keeps apart from the keys written (`not: {required: [git,
/// path]}`, `path` written) is not offered; the others still are.
#[test]
fn a_key_a_not_keeps_apart_from_written_keys_is_not_offered() {
    let schema = r#"{"type":"object","properties":{"dep":{"type":"object",
        "properties":{"version":{"type":"string"},"git":{"type":"string"},"path":{"type":"string"}},
        "allOf":[{"not":{"required":["git","path"]}}]}}}"#;
    let bundle = build_schema(schema).unwrap();
    let documents: [(Language, &str); 5] = [
        (Language::YAML, "dep:\n  path: p\n  |\n"),
        (Language::JSON, "{\"dep\": {\n  \"path\": \"p\",\n  |\n}}\n"),
        (Language::TOML, "[dep]\npath = \"p\"\n|\n"),
        (Language::HCL, "dep {\n  path = \"p\"\n  |\n}\n"),
        (Language::XML, "<dep>\n  <path>p</path>\n  <|\n</dep>\n"),
    ];
    let mut failures = Vec::new();
    for (language, marked) in documents {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let labels: Vec<String> =
            completions(&source, at, true, Some(&bundle), true, language.behavior())
                .into_iter()
                .map(|option| option.label)
                .collect();
        if labels != ["version"] {
            failures.push(format!("{language}: offered {labels:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
