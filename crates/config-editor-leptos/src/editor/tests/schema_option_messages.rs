//! A value that matches none of a `oneOf`'s (or `anyOf`'s) options, or more
//! than one of a `oneOf`'s, is one problem on the object's key naming the
//! options and what each misses (S.combinators-6). In every language.

use crate::editor::Language;
use crate::editor::diagnostics::{Diagnostic, Schema};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

fn problems(language: Language, yaml: &str, schema: &str) -> (String, Vec<Diagnostic>) {
    let text = Language::YAML.convert(yaml, language).unwrap().text;
    let found = Schema::compile(schema).unwrap().validate(language, &text);
    (text, found)
}

/// The one-based line where `key` is written in `text` as a key.
fn key_line(text: &str, key: &str) -> Option<usize> {
    let spellings = [
        format!("{key}:"),
        format!("\"{key}\""),
        format!("{key} ="),
        format!("{key} {{"),
        format!("[{key}"),
        format!("<{key}>"),
        format!("<{key}/>"),
    ];
    text.lines()
        .position(|line| {
            let line = line.trim_start();
            spellings.iter().any(|spelling| line.starts_with(spelling))
        })
        .map(|index| index + 1)
}

/// Checks that each language gives exactly `expected` (sorted), each
/// marked on the line where its key (the second item) is written.
fn check(yaml: &str, schema: &str, expected: &[(&str, &str)], failures: &mut Vec<String>) {
    for language in LANGUAGES {
        let (text, found) = problems(language, yaml, schema);
        let mut messages: Vec<(String, Option<usize>)> = found
            .iter()
            .map(|problem| (problem.message.clone(), problem.line))
            .collect();
        messages.sort();
        let wanted: Vec<(String, Option<usize>)> = expected
            .iter()
            .map(|(message, key)| (message.to_string(), key_line(&text, key)))
            .collect();
        if messages != wanted {
            failures.push(format!(
                "{language}\n{text}\nfound  {messages:?}\nwanted {wanted:?}"
            ));
        }
    }
}

fn options(keyword: &str, options: &str) -> String {
    format!(r#"{{"type":"object","properties":{{"dns":{{"{keyword}":[{options}]}}}}}}"#)
}

const TITLED: &str = r#"
    {"title":"Cloudflare","required":["cloudflare"],"properties":{"cloudflare":{"type":"object"}}},
    {"title":"Route 53","required":["route53"],"properties":{"route53":{"type":"object"}}}"#;

#[test]
fn a_value_matching_no_option_names_each_option_and_what_it_misses() {
    let mut failures = Vec::new();
    for keyword in ["oneOf", "anyOf"] {
        check(
            "dns:\n  other: 1\n",
            &options(keyword, TITLED),
            &[(
                &format!(
                    r#"/dns: matches none of the {keyword} options: Cloudflare needs "cloudflare"; Route 53 needs "route53""#
                ),
                "dns",
            )],
            &mut failures,
        );
    }
    // Untitled: the start of the description, else the option's number.
    check(
        "dns:\n  other: 1\n",
        &options(
            "oneOf",
            r#"{"description":"Plain password login, checked against the local user list","required":["password"]},
               {"required":["token"]}"#,
        ),
        &[(
            r#"/dns: matches none of the oneOf options: Plain password login, checked against th… needs "password"; option 2 needs "token""#,
            "dns",
        )],
        &mut failures,
    );
    // A key one option refuses, another misses: each option's own reason,
    // not the last option's errors alone.
    check(
        "dns:\n  a: x\n  c: 1\n",
        &options(
            "oneOf",
            r#"{"title":"A","properties":{"a":{"type":"string"}},"required":["a"],"additionalProperties":false},
               {"title":"B","properties":{"b":{"type":"string"}},"required":["b"],"additionalProperties":false}"#,
        ),
        &[(
            r#"/dns: matches none of the oneOf options: A does not allow "c"; B needs "b""#,
            "dns",
        )],
        &mut failures,
    );
    // Options nested in `allOf`, and one reached through a `$ref`.
    check(
        "dns:\n  beta: 2\n",
        r##"{"$defs":{"other":{"title":"Other","required":["gamma"]}},
            "properties":{"dns":{"oneOf":[
              {"title":"Both","allOf":[{"required":["alpha"]},{"properties":{"beta":{"type":"number"}}}]},
              {"$ref":"#/$defs/other"}]}}}"##,
        &[(
            r#"/dns: matches none of the oneOf options: Both needs "alpha"; Other needs "gamma""#,
            "dns",
        )],
        &mut failures,
    );
    // Another problem than a key: what, of the value itself.
    check(
        "dns:\n  a: x\n  b: y\n",
        &options(
            "anyOf",
            r#"{"title":"Short","maxProperties":1},{"title":"Named","required":["name"]}"#,
        ),
        &[(
            r#"/dns: matches none of the anyOf options: Short: {"a":"x","b":"y"} has more than 1 property; Named needs "name""#,
            "dns",
        )],
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn a_value_matching_several_options_names_the_ones_it_matches() {
    let mut failures = Vec::new();
    check(
        "dns:\n  cloudflare: {}\n  route53: {}\n",
        &options("oneOf", TITLED),
        &[(
            "/dns: matches more than one of the oneOf options (Cloudflare, Route 53); only one may match",
            "dns",
        )],
        &mut failures,
    );
    check(
        "dns:\n  cloudflare: {}\n  route53: {}\n",
        &options(
            "oneOf",
            &format!(r#"{{"title":"Manual","required":["manual"]}},{TITLED}"#),
        ),
        &[(
            "/dns: matches more than one of the oneOf options (Cloudflare, Route 53); only one may match",
            "dns",
        )],
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// An option the value got further into than the others is the one it
/// was meant for: its own problem is shown where it is.
#[test]
fn the_option_a_value_went_furthest_into_shows_its_own_problems() {
    let mut failures = Vec::new();
    check(
        "dns:\n  cloudflare:\n    token: abc\n",
        &options(
            "oneOf",
            r#"{"title":"Cloudflare","required":["cloudflare"],
                "properties":{"cloudflare":{"properties":{"token":{"type":"integer"}}}}},
               {"title":"Route 53","required":["route53"]}"#,
        ),
        &[(
            r#"/dns/cloudflare/token: "abc" is not of type "integer""#,
            "token",
        )],
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The reason an option gives never quotes a secret field's value; a key
/// whose schema is `false` reads as one the option does not allow.
#[cfg(feature = "protect")]
#[test]
fn an_option_s_reason_never_quotes_a_secret() {
    let schema = options(
        "oneOf",
        r#"{"title":"Login","required":["password"],
            "properties":{"password":{"type":"string","x-secret":true,"minLength":12}}},
           {"title":"Token","properties":{"password":false}}"#,
    );
    for language in LANGUAGES {
        let (text, found) = problems(language, "dns:\n  password: hunter2\n", &schema);
        let messages: Vec<&str> = found
            .iter()
            .map(|problem| problem.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                r#"/dns: matches none of the oneOf options: Login: /password: the secret value is shorter than 12 characters; Token does not allow "password""#
            ],
            "{language}\n{text}"
        );
    }
}
