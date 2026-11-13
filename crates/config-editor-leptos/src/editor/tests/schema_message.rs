//! Schema problems name the field they are about by its JSON pointer: no
//! prefix at the root (`"/"` is the key `""`, RFC 6901), and in plain words
//! (no "False schema"), in every language.

use crate::editor::Language;
use crate::editor::diagnostics::Schema;

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

/// Schema L.3 and L.12: an invalid schema says where in it the value is,
/// and one nested too deep to read says so.
#[test]
fn an_invalid_schema_names_its_place_and_a_too_deep_one_says_so() {
    let message = |text: &str| Schema::compile(text).expect_err("refused").0;
    let typed = message(r#"{"properties":{"a":{"type":5}}}"#);
    assert!(typed.contains("(at /properties/a/type)"), "{typed}");
    let lists = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let deep = message(&format!(r#"{{"enum":[{lists}]}}"#));
    assert!(deep.contains("more than 128 levels deep"), "{deep}");
}

/// Up to draft-07 `format` is checked; the problem reads "is not a valid",
/// not "is not a "email"". From 2019-09 on it is an annotation only.
#[test]
fn a_value_out_of_its_format_reads_plainly_up_to_draft_07() {
    let schema = |draft: &str| {
        format!(
            r#"{{"$schema":"{draft}","properties":{{"e":{{"type":"string","format":"email"}},"d":{{"type":"string","format":"date"}}}}}}"#
        )
    };
    let yaml = "e: nope\nd: \"2020-13-45\"\n";
    for draft in [
        "http://json-schema.org/draft-04/schema#",
        "http://json-schema.org/draft-06/schema#",
        "http://json-schema.org/draft-07/schema#",
    ] {
        for language in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            assert_eq!(
                messages(language, yaml, &schema(draft)),
                [
                    r#"/d: "2020-13-45" is not a valid "date""#,
                    r#"/e: "nope" is not a valid "email""#,
                ],
                "{draft} {language}"
            );
        }
    }
    for draft in [
        "https://json-schema.org/draft/2019-09/schema",
        "https://json-schema.org/draft/2020-12/schema",
    ] {
        assert!(
            messages(Language::YAML, yaml, &schema(draft)).is_empty(),
            "{draft}"
        );
    }
}

/// A value for options chosen by type (a number, or an expression string)
/// that is text matching neither: the message names both options, the
/// number one too, and a pattern's line breaks are written escaped, never
/// raw.
#[test]
fn a_value_for_options_chosen_by_type_names_each_and_escapes_its_pattern() {
    let schema = r#"{"properties":{"n":{"oneOf":[{"type":"number"},{"type":"string","pattern":"^\\$\\{[^\r\n]*\\}$"}]}}}"#;
    let expected = r#"/n: matches none of the oneOf options: option 1: "abc" is not of type "number"; option 2: "abc" does not match "^\\$\\{[^\r\n]*\\}$""#;
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let found = messages(language, "n: abc\n", schema);
        assert_eq!(found, [expected], "{language}");
        assert!(!found[0].contains(['\r', '\n']), "{language}");
    }
    // A number written as one is fine; an object's typo is still the typo.
    assert!(messages(Language::YAML, "n: 5\n", schema).is_empty());
}
