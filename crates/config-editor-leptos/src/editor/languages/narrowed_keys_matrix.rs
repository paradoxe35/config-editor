//! The keys offered in an object are only those a value can be valid
//! for, in every language alike: a key whose schema admits nothing
//! (`false`, `{not: {}}`, `{not: true}`) is not offered, and the keys of
//! an object nested in a `oneOf`/`anyOf` option are the matched option's
//! only, as the item's own keys already are.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::super::schema::build_schema;
use super::Language;
use super::harness::{At, completing, report, write};
use serde_json::json;

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

/// The caret on a new entry of the object `off`, over lines and on one line.
fn inside_off(language: Language) -> Vec<Result<String, String>> {
    let at = At::NewKey {
        object: "/off",
        typed: "",
    };
    [&[][..], &["/off"][..]]
        .into_iter()
        .map(|one_line| {
            write(language, &json!({"off": {}}), one_line, at).map(|written| written.marked())
        })
        .collect()
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
        for language in completing() {
            for marked in inside_off(language) {
                let labels = match marked {
                    Ok(marked) => offered(&schema, &marked, language),
                    Err(error) => vec![error],
                };
                if labels != ["a"] {
                    failures.push(format!("{language} b: {never}: offered {labels:?}"));
                }
            }
        }
    }
    report(&failures);
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

/// The caret on a new entry of `sub` in an item whose `kind` is `b`: `sub`
/// over lines and on one line, alone and with an item of the other kind
/// after it; then the forms written by hand.
fn inside_sub(language: Language) -> Vec<Result<String, String>> {
    let item = json!({"kind": "b", "sub": {}});
    let at = At::NewKey {
        object: "/items/0/sub",
        typed: "",
    };
    let mut found = Vec::new();
    for document in [
        json!({ "items": [item] }),
        json!({"items": [item, {"kind": "a"}]}),
    ] {
        for one_line in [&[][..], &["/items/0/sub"][..]] {
            found.push(write(language, &document, one_line, at).map(|written| written.marked()));
        }
    }
    found.extend(
        written_by_hand(language)
            .iter()
            .map(|marked| Ok((*marked).to_string())),
    );
    found
}

/// Forms of each language's own syntax, beside those it writes itself: an
/// HCL list of objects as a value, a YAML flow map, a TOML sub-table header,
/// an XML element on one line.
fn written_by_hand(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &[
            "items:\n  - kind: b\n    sub:\n      |\n",
            "items:\n  - kind: b\n    sub:\n      |\n  - kind: a\n",
            "items:\n  - kind: b\n    sub: {|}\n",
        ],
        "JSON" => &[
            "{\"items\": [\n  {\n    \"kind\": \"b\",\n    \"sub\": {\n      |\n    }\n  }\n]}\n",
            "{\"items\": [{\"kind\": \"b\", \"sub\": {|}}, {\"kind\": \"a\"}]}\n",
        ],
        "TOML" => &[
            "[[items]]\nkind = \"b\"\n[items.sub]\n|\n",
            "[[items]]\nkind = \"b\"\nsub = { | }\n",
        ],
        "HCL" => &[
            "items = [\n  {\n    kind = \"b\"\n    sub = {\n      |\n    }\n  },\n]\n",
            "items = [{ kind = \"b\", sub = { | } }]\n",
        ],
        "XML" => &[
            "<items><kind>b</kind><sub><|</sub></items>\n",
            "<items>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</items>\n<items><kind>a</kind></items>\n",
        ],
        _ => &[],
    }
}

#[test]
fn keys_of_an_object_in_an_option_follow_the_matched_option() {
    let mut failures = Vec::new();
    for combinator in ["oneOf", "anyOf"] {
        let schema = option_schema(combinator);
        for language in completing() {
            for marked in inside_sub(language) {
                let labels = match &marked {
                    Ok(marked) => offered(&schema, marked, language),
                    Err(error) => vec![error.clone()],
                };
                if labels != ["sb"] {
                    failures.push(format!(
                        "{language} {combinator} {marked:?}: offered {labels:?}"
                    ));
                }
            }
        }
    }
    report(&failures);
}
