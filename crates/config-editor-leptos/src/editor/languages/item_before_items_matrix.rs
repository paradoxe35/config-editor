//! A list item a suggestion writes in front of items already written gets
//! the separator between it and the next one, on the item's line or the
//! next, in every language with inline lists: the list still reads, one
//! item longer.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::schema::build_schema;
use super::Language;
use super::harness::{completing, name_range, report, text};
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"type":"object","properties":{"spec":{"type":"object","properties":{
    "containers":{"type":"array","items":{"type":"object","required":["name"],
        "properties":{"name":{"type":"string"}}}},
    "tags":{"type":"array","items":{"enum":["x","y"]}}}}}}"#;

/// The caret in front of the first item of the list at `list` in
/// `document`, the list written on one line, in each language that writes
/// lists in brackets: on the item's line, on a line of its own, and with a
/// comment line above it or below it. A form the language cannot read
/// (a list over lines in a language whose lists are on one line) is left
/// out; a language that cannot write the list is a failure.
fn written_cases(
    document: &Value,
    list: &str,
    failures: &mut Vec<String>,
) -> Vec<(Language, String)> {
    let mut cases = Vec::new();
    for language in completing() {
        let config = language.behavior().config();
        let Some((open, _)) = config.list_brackets else {
            continue;
        };
        let opened = text(language, document, &[list]).and_then(|text| {
            let (_, name_end) = name_range(language, &text, list)?;
            let at = text[name_end..]
                .find(open)
                .ok_or_else(|| format!("{language}: no `{open}` after {list}\n{text}"))?;
            Ok((name_end + at + open.len_utf8(), text))
        });
        let (start, text) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
        let indent: String = text[line_start..]
            .chars()
            .take_while(|c| *c == ' ')
            .chain("    ".chars())
            .collect();
        let mut inserts = vec!["| ".to_string(), format!("\n{indent}|\n{indent}")];
        if let Some(comment) = config.line_comments.first() {
            inserts.push(format!("\n{indent}{comment} first\n{indent}|\n{indent}"));
            inserts.push(format!("\n{indent}|\n{indent}{comment} c\n{indent}"));
        }
        let before = cases.len();
        for insert in inserts {
            let marked = format!("{}{insert}{}", &text[..start], &text[start..]);
            if language
                .parse(&marked.replacen('|', "", 1))
                .is_ok_and(|read| read == *document)
            {
                cases.push((language, marked));
            }
        }
        if cases.len() == before {
            failures.push(format!(
                "{language}: no form of the caret before the first item reads\n{text}"
            ));
        }
    }
    cases
}

/// Forms written by hand, beside those each language writes itself.
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
/// Forms written by hand, beside those each language writes itself.
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
    // A comment line above the caret, or between it and the next value.
    (
        Language::HCL,
        "spec {\n  tags = [\n    # first\n    |\n    \"x\"\n  ]\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  tags = [\n    // first\n    |\n    \"x\"\n  ]\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  tags = [\n    |\n    # c\n    \"x\"\n  ]\n}\n",
    ),
    (
        Language::TOML,
        "[spec]\ntags = [\n  # first\n  |\n  \"x\",\n]\n",
    ),
    (
        Language::YAML,
        "spec:\n  tags: [\n    # first\n    |\n    x\n  ]\n",
    ),
];

#[test]
fn a_value_written_in_front_of_values_is_separated_from_them() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    let mut cases: Vec<(Language, String)> = VALUES
        .iter()
        .map(|(language, marked)| (*language, (*marked).to_string()))
        .collect();
    cases.extend(written_cases(
        &json!({"spec": {"tags": ["x"]}}),
        "/spec/tags",
        &mut failures,
    ));
    for (language, marked) in &cases {
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
    report(&failures);
}

#[test]
fn an_item_written_in_front_of_items_is_separated_from_them() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    let mut cases: Vec<(Language, String)> = CASES
        .iter()
        .map(|(language, marked)| (*language, (*marked).to_string()))
        .collect();
    for items in [
        json!([{"name": "a"}]),
        json!([{"name": "a"}, {"name": "b"}]),
    ] {
        let document = json!({"spec": {"containers": items}});
        cases.extend(written_cases(&document, "/spec/containers", &mut failures));
    }
    for (language, marked) in &cases {
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
    report(&failures);
}
