//! The keys offered in an object are only those a value can be valid
//! for, in every language alike: a key whose schema admits nothing
//! (`false`, `{not: {}}`, `{not: true}`) is not offered, and the keys of
//! an object nested in a `oneOf`/`anyOf` option are the matched option's
//! only, as the item's own keys already are.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::super::diagnostics::build_schema;
use super::Language;

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

fn offered(schema: &str, marked: &str, language: Language) -> Vec<String> {
    let bundle = build_schema(schema).unwrap();
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let mut labels: Vec<String> =
        completions(&source, at, true, Some(&bundle), true, language.behavior())
            .into_iter()
            .map(|option| option.label)
            .collect();
    labels.sort();
    labels
}

/// The caret inside the object `off`, per language.
fn inside_off(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "off:\n  |\n",
        "JSON" => "{\"off\": {|}}\n",
        "TOML" => "[off]\n|\n",
        "HCL" => "off {\n  |\n}\n",
        _ => "<off><|</off>\n",
    }
}

#[test]
fn keys_whose_schema_admits_nothing_are_not_offered() {
    let schemas = [
        r#"false"#,
        r#"{"not":{}}"#,
        r#"{"not":true}"#,
        r#"{"type":"string","not":{}}"#,
    ];
    let mut failures = Vec::new();
    for never in schemas {
        let schema = format!(
            r#"{{"type":"object","properties":{{"off":{{"type":"object","properties":{{
                "a":{{"type":"string"}},"b":{never}}}}}}}}}"#
        );
        for language in LANGUAGES {
            let labels = offered(&schema, inside_off(language), language);
            if labels != ["a"] {
                failures.push(format!("{language} b: {never}: offered {labels:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Items told apart by `kind`, each option with its own `sub` object.
fn option_schema(combinator: &str) -> String {
    format!(
        r#"{{"type":"object","properties":{{"items":{{"type":"array","items":{{"{combinator}":[
        {{"type":"object","required":["kind"],"properties":{{"kind":{{"const":"a"}},
            "sub":{{"type":"object","additionalProperties":false,"properties":{{"sa":{{"type":"string"}}}}}}}}}},
        {{"type":"object","required":["kind"],"properties":{{"kind":{{"const":"b"}},
            "sub":{{"type":"object","additionalProperties":false,"properties":{{"sb":{{"type":"string"}}}}}}}}}}
    ]}}}}}}}}"#
    )
}

/// The caret inside `sub` of an item whose `kind` is `b`, per language,
/// with an item of the other kind after it.
fn inside_sub(language: Language) -> Vec<&'static str> {
    match language.label() {
        "YAML" => vec![
            "items:\n  - kind: b\n    sub:\n      |\n",
            "items:\n  - kind: b\n    sub:\n      |\n  - kind: a\n",
            "items:\n  - kind: b\n    sub: {|}\n",
        ],
        "JSON" => vec![
            "{\"items\": [\n  {\n    \"kind\": \"b\",\n    \"sub\": {\n      |\n    }\n  }\n]}\n",
            "{\"items\": [{\"kind\": \"b\", \"sub\": {|}}, {\"kind\": \"a\"}]}\n",
        ],
        "TOML" => vec![
            "[[items]]\nkind = \"b\"\n[items.sub]\n|\n",
            "[[items]]\nkind = \"b\"\nsub = { | }\n",
        ],
        "HCL" => vec![
            "items = [\n  {\n    kind = \"b\"\n    sub = {\n      |\n    }\n  },\n]\n",
            "items = [{ kind = \"b\", sub = { | } }]\n",
        ],
        _ => vec![
            "<items><kind>b</kind><sub><|</sub></items>\n",
            "<items>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</items>\n<items><kind>a</kind></items>\n",
        ],
    }
}

#[test]
fn keys_of_an_object_in_an_option_follow_the_matched_option() {
    let mut failures = Vec::new();
    for combinator in ["oneOf", "anyOf"] {
        let schema = option_schema(combinator);
        for language in LANGUAGES {
            for marked in inside_sub(language) {
                let labels = offered(&schema, marked, language);
                if labels != ["sb"] {
                    failures.push(format!(
                        "{language} {combinator} {marked:?}: offered {labels:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
