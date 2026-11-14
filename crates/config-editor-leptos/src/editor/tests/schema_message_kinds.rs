//! Schema problems of every kind read in plain words, in every language:
//! the options of a `oneOf` that share a description are told apart by
//! number, a value of a type no option takes names every option's type, a
//! list `contains` refuses says how many of its items count, a `not`
//! schema says what it must not match, and an item a list refuses
//! (`additionalItems`, `unevaluatedItems`) is marked where it is written.
//! Problems come in the order they are written, and a schema text may
//! start with a byte order mark.

use crate::editor::Language;
use crate::editor::diagnostics::{Diagnostic, Schema, build_schema};

const LANGUAGES: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// The problems of `yaml` written in `language` (`None` where the
/// language cannot hold it, a TOML null).
fn problems(language: Language, yaml: &str, schema: &str) -> Option<(String, Vec<Diagnostic>)> {
    let text = Language::YAML.convert(yaml, language).ok()?.text;
    let found = Schema::compile(schema).unwrap().validate(language, &text);
    Some((text, found))
}

#[test]
fn problems_of_every_kind_read_plainly_in_every_language() {
    let any_of = r#"{"properties":{"t":{"anyOf":[{"type":"integer","minimum":1},{"type":"string","enum":["auto"]}]}}}"#;
    let one_of =
        r#"{"properties":{"t":{"oneOf":[{"type":"integer"},{"type":"string","enum":["auto"]}]}}}"#;
    let shared = r#"{"properties":{"s":{"oneOf":[
        {"description":"Each job must have an id","required":["x"]},
        {"description":"Each job must have an id","required":["y"]},
        {"description":"Another","required":["z"]}]}}}"#;
    let cases: &[(&str, &str, &str)] = &[
        (
            "t: 1.5\n",
            any_of,
            r#"/t: 1.5 is not of type "integer", "string""#,
        ),
        (
            "t: true\n",
            any_of,
            r#"/t: true is not of type "integer", "string""#,
        ),
        (
            "t: null\n",
            one_of,
            r#"/t: null is not of type "integer", "string""#,
        ),
        (
            "s:\n  a: 1\n",
            shared,
            r#"/s: matches none of the oneOf options: option 1 needs "x"; option 2 needs "y"; Another needs "z""#,
        ),
        (
            "l: [1, 2]\n",
            r#"{"properties":{"l":{"contains":{"type":"string"}}}}"#,
            r#"/l: [1,2] has no item its "contains" schema allows"#,
        ),
        (
            "l: [a, 2]\n",
            r#"{"properties":{"l":{"contains":{"type":"string"},"minContains":2}}}"#,
            r#"/l: ["a",2] has fewer than 2 items its "contains" schema allows"#,
        ),
        (
            "l: [a, b, c]\n",
            r#"{"properties":{"l":{"contains":{"type":"string"},"maxContains":2}}}"#,
            r#"/l: ["a","b","c"] has more than 2 items its "contains" schema allows"#,
        ),
        (
            "s: x\n",
            r#"{"properties":{"s":{"not":{"type":"string"}}}}"#,
            r#"/s: "x" must not match {"type":"string"}"#,
        ),
        // A `not` of required keys: the keys can't be used together.
        (
            "d:\n  git: g\n  path: p\n",
            r#"{"properties":{"d":{"not":{"required":["git","path"]}}}}"#,
            r#"/d: "git" can't be used together with "path""#,
        ),
        (
            "d:\n  a: 1\n  b: 2\n  c: 3\n",
            r##"{"definitions":{"abc":{"required":["a","b","c"]}},"properties":{"d":{"allOf":[{"not":{"$ref":"#/definitions/abc"}}]}}}"##,
            r#"/d: "a" can't be used together with "b", "c""#,
        ),
    ];
    let mut failures = Vec::new();
    for (yaml, schema, wanted) in cases {
        for language in LANGUAGES {
            let Some((text, found)) = problems(language, yaml, schema) else {
                continue;
            };
            let messages: Vec<&str> = found
                .iter()
                .map(|problem| problem.message.as_str())
                .collect();
            if messages != [*wanted] {
                failures.push(format!("{language} {text:?}: {messages:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn each_item_a_list_refuses_is_marked_where_it_is_written() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "args:\n  - run\n  - 3\n  - x\n",
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","properties":{"args":{"items":[{"type":"string"}],"additionalItems":false}}}"#,
            &["/args/1: 3 is not allowed", "/args/2: \"x\" is not allowed"],
        ),
        (
            "args:\n  - run\n  - 3\n  - x\n",
            r#"{"properties":{"args":{"prefixItems":[{"type":"string"},{"type":"integer"}],"unevaluatedItems":false}}}"#,
            &["/args/2: \"x\" is not allowed"],
        ),
    ];
    let mut failures = Vec::new();
    for (yaml, schema, wanted) in cases {
        for language in [Language::YAML, Language::JSON] {
            let (text, found) = problems(language, yaml, schema).unwrap();
            let messages: Vec<&str> = found
                .iter()
                .map(|problem| problem.message.as_str())
                .collect();
            // Each on its item's line: `3`, `"x"`.
            let lines: Vec<Option<usize>> = found.iter().map(|problem| problem.line).collect();
            let written: Vec<Option<usize>> = wanted
                .iter()
                .map(|message| {
                    let item = message
                        .rsplit_once(": ")
                        .unwrap()
                        .1
                        .trim_end_matches(" is not allowed");
                    let item = item.trim_matches('"');
                    text.lines()
                        .position(|line| line.trim().trim_end_matches([',', '"']).ends_with(item))
                        .map(|at| at + 1)
                })
                .collect();
            if messages != *wanted || lines != written {
                failures.push(format!("{language} {text:?}: {found:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn problems_come_in_the_order_they_are_written() {
    let cases: &[(&str, &str)] = &[
        (
            "a: x\nb: y\n",
            r#"{"properties":{"b":{"type":"integer"},"a":{"type":"integer"}}}"#,
        ),
        (
            "b: y\na: x\n",
            r#"{"properties":{"a":{"type":"integer"}},"required":["zz"],"additionalProperties":false}"#,
        ),
        (
            "x: 1\na: x\n",
            r#"{"properties":{"a":{"type":"integer"}},"additionalProperties":false}"#,
        ),
    ];
    let mut failures = Vec::new();
    for (yaml, schema) in cases {
        for language in LANGUAGES {
            let (text, found) = problems(language, yaml, schema).unwrap();
            let places: Vec<(Option<usize>, Option<usize>)> = found
                .iter()
                .map(|problem| (problem.line, problem.column))
                .collect();
            let mut sorted = places.clone();
            sorted.sort();
            if places != sorted {
                failures.push(format!("{language} {text:?}: {places:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_schema_text_may_start_with_a_byte_order_mark() {
    let schema = "\u{feff}{\"properties\":{\"a\":{\"type\":\"string\"}}}";
    let found = Schema::compile(schema).map(|schema| schema.validate(Language::YAML, "a: 1\n"));
    let messages: Option<Vec<String>> = found
        .ok()
        .map(|found| found.into_iter().map(|problem| problem.message).collect());
    assert_eq!(
        messages,
        Some(vec!["/a: 1 is not of type \"string\"".to_string()])
    );
    assert!(build_schema(schema).is_ok());
}

/// An object where no `oneOf`/`anyOf` option takes one is a wrong type,
/// underlined as one: on the value where it starts on the key's line, on
/// the key where it is a block below; an object an option takes but whose
/// keys match none stays on the key, as a missing key does.
#[test]
fn an_object_no_option_takes_is_underlined_as_a_wrong_type() {
    let types = r#"{"properties":{"o":{"anyOf":[{"type":"string"},{"type":"integer"}]},
        "p":{"oneOf":[{"type":"string"},{"type":"integer"}]}}}"#;
    let keys = r#"{"properties":{"o":{"oneOf":[{"required":["x"]},{"required":["y"]}]}}}"#;
    type Case<'a> = (Language, &'a str, &'a str, &'a [(usize, usize)]);
    let cases: &[Case<'_>] = &[
        (
            Language::JSON,
            "{\n  \"o\": {\"a\": 1},\n  \"p\": {\"b\": 2}\n}\n",
            types,
            &[(2, 8), (3, 8)],
        ),
        (
            Language::YAML,
            "o: {a: 1}\np:\n  b: 2\n",
            types,
            &[(1, 4), (2, 1)],
        ),
        (
            Language::HCL,
            "o = { a = 1 }\np {\n  b = 2\n}\n",
            types,
            &[(1, 5), (2, 1)],
        ),
        (
            Language::JSON,
            "{\n  \"o\": {\"a\": 1}\n}\n",
            keys,
            &[(2, 3)],
        ),
    ];
    let mut failures = Vec::new();
    for (language, text, schema, wanted) in cases {
        let found = Schema::compile(schema).unwrap().validate(*language, text);
        let places: Vec<(usize, usize)> = found
            .iter()
            .filter_map(|problem| Some((problem.line?, problem.column?)))
            .collect();
        if places != *wanted {
            failures.push(format!("{language} {text:?}: {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
