//! Schema problems name the field they are about by its JSON pointer: no
//! prefix at the root (`"/"` is the key `""`, RFC 6901), and in plain words
//! (no "False schema"), in every language.

use super::Language;
use super::diagnostics::Schema;

fn messages(language: Language, yaml: &str, schema: &str) -> Vec<String> {
    let text = Language::YAML.convert(yaml, language).unwrap().text;
    let mut found: Vec<String> = Schema::compile(schema)
        .unwrap()
        .validate(language, &text)
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    found.sort();
    found
}

#[test]
fn problems_at_the_root_have_no_pointer_and_false_schemas_read_plainly() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "a: 1\nb: 2\n",
            r#"{"properties":{"a":{"type":"integer"}},"additionalProperties":false,"required":["q"]}"#,
            &[
                r#""b" is not allowed here"#,
                r#""q" is a required property"#,
            ],
        ),
        (
            "x:\n  b: 1\n",
            r#"{"properties":{"x":{"additionalProperties":false}}}"#,
            &[r#"/x: "b" is not allowed here"#],
        ),
        (
            "a: 1\nb: 2\n",
            r#"{"properties":{"b":false}}"#,
            &["/b: 2 is not allowed"],
        ),
        (
            "a: 1\nb: 2\n",
            r#"{"additionalProperties":false,"properties":{"a":true}}"#,
            &[r#""b" is not allowed here"#],
        ),
    ];
    let mut failures = Vec::new();
    for (yaml, schema, expected) in cases {
        for language in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            let found = messages(language, yaml, schema);
            if found != *expected {
                failures.push(format!("{language} {yaml:?} {schema}: {found:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_empty_key_is_named_by_its_pointer() {
    let schema = r#"{"properties":{"":{"additionalProperties":false}}}"#;
    assert_eq!(
        messages(Language::YAML, "\"\": {b: 1}\n", schema),
        [r#"/: "b" is not allowed here"#]
    );
}
