//! The options of an object's `oneOf`/`anyOf`, named in every language
//! against the schema-help fixture's `dns` (a Cloudflare option through a
//! `$ref` that requires `cloudflare`, a Route 53 one that requires
//! `route53`, and two that require `provider` with a `const` each): hover
//! on the field names the option the value is in (or lists them all), and
//! key suggestions carry their option's title, the matched option's keys
//! first. Each case is a document, written by every language itself; each
//! test lists every failing case at once.

use super::super::completion::{ChoiceHover, ValueDoc, completions, key_hover};
use super::super::schema::{SchemaBundle, build_schema};
use super::Language;
use super::harness::{At, completing, hovering, key_place, report, text, write};
use serde_json::{Value, json};

fn help() -> SchemaBundle {
    build_schema(include_str!(
        "../../../tests/fixtures/schema-help.schema.json"
    ))
    .unwrap()
}

/// The hover of the key at `pointer` of `document` as `language` writes
/// it (the values at `one_line` on one line).
fn hover_at(
    language: Language,
    document: &Value,
    one_line: &[&str],
    pointer: &str,
) -> Result<(String, Option<super::super::completion::KeyHover>), String> {
    let text = text(language, document, one_line)?;
    let (line, column) = key_place(language, &text, pointer, 1)?;
    let hover = key_hover(&text, line, column, Some(&help()), language.behavior());
    Ok((text, hover))
}

fn matched(title: &str, description: &str) -> Option<ChoiceHover> {
    Some(ChoiceHover::Matched(ValueDoc {
        title: Some(title.into()),
        description: Some(description.into()),
    }))
}

#[test]
fn hover_names_the_option_the_value_is_in() {
    let cloudflare = matched("Cloudflare", "Records in a Cloudflare zone.");
    let file = matched("Static file", "Records read from a file.");
    let none = Some(ChoiceHover::OneOf(vec![
        "Cloudflare".into(),
        "Route 53".into(),
        "Manual".into(),
        "Static file".into(),
    ]));
    let cases: [(Value, &[&str], &Option<ChoiceHover>); 5] = [
        // By its required key (through a `$ref`), over lines and on one.
        (
            json!({"dns": {"cloudflare": {"zone": "z"}}}),
            &[],
            &cloudflare,
        ),
        (
            json!({"dns": {"cloudflare": {"zone": "z"}}}),
            &["/dns/cloudflare"],
            &cloudflare,
        ),
        // Two options require `provider`: its const decides.
        (json!({"dns": {"provider": "static"}}), &[], &file),
        // Nothing decides yet: every option, by title.
        (json!({"dns": {"ttl": 60}}), &[], &none),
        (json!({"dns": {"provider": "other"}}), &[], &none),
    ];
    let mut failures = Vec::new();
    for language in hovering() {
        for (document, one_line, expected) in &cases {
            let found = hover_at(language, document, one_line, "/dns")
                .map(|(text, hover)| (text, hover.and_then(|hover| hover.choice)));
            if !matches!(&found, Ok((_, choice)) if choice == *expected) {
                failures.push(format!("{language} {document}: {found:?}"));
            }
        }
    }
    report(&failures);
}

/// The keys offered on a new entry of `dns` in `document` (as after
/// Ctrl+Space), with their details.
fn offered(language: Language, document: &Value) -> Result<Vec<(String, String)>, String> {
    let at = At::NewKey {
        object: "/dns",
        typed: "",
    };
    let written = write(language, document, &[], at)?;
    Ok(completions(
        &written.text,
        written.cursor(),
        true,
        Some(&help()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| (option.label, option.detail))
    .collect())
}

/// A key offered, its detail, and the keys the list starts with.
type Expected = (&'static str, &'static str, &'static [&'static str]);

#[test]
fn key_suggestions_carry_their_option_and_list_the_matched_one_first() {
    let cloudflare: Expected = (
        "ttl",
        "Cloudflare — Seconds records are cached.",
        &["ttl"][..],
    );
    let file: Expected = ("path", "Static file", &["path"][..]);
    let cases = [
        (json!({"dns": {"cloudflare": {"zone": "z"}}}), cloudflare),
        (json!({"dns": {"provider": "static"}}), file),
    ];
    let mut failures = Vec::new();
    for language in completing() {
        for (document, (key, detail, first)) in &cases {
            let options = match offered(language, document) {
                Ok(options) => options,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
            let found = options.iter().find(|(label, _)| label == key);
            let leading: Vec<&str> = options
                .iter()
                .take(first.len())
                .map(|(label, _)| label.as_str())
                .collect();
            if found.map(|(_, found)| found.as_str()) != Some(*detail) || leading != *first {
                failures.push(format!("{language} {document}: {options:?}"));
            }
        }
        // Before anything decides, each key names its option.
        let options = offered(language, &json!({"dns": {}})).unwrap_or_default();
        for (key, detail) in [("route53", "Route 53"), ("records", "Manual")] {
            if !options
                .iter()
                .any(|(label, found)| label == key && found == detail)
            {
                failures.push(format!("{language} undecided {key}: {options:?}"));
            }
        }
    }
    report(&failures);
}

#[test]
fn a_key_of_several_options_names_each_and_a_const_key_shows_its_values() {
    let mut failures = Vec::new();
    let at = At::NewKey {
        object: "/dns",
        typed: "",
    };
    for language in completing() {
        // The empty `dns` object with the caret in it.
        let written = match write(language, &json!({"dns": {}}), &[], at) {
            Ok(written) => written,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let options = completions(
            &written.text,
            written.cursor(),
            true,
            Some(&help()),
            true,
            language.behavior(),
        );
        for (key, detail, type_label) in [
            ("provider", "Manual, Static file", "\"manual\" | \"static\""),
            (
                "ttl",
                "Cloudflare, Route 53 — Seconds records are cached.",
                "integer",
            ),
            ("path", "Static file", "string"),
        ] {
            let found = options.iter().find(|option| option.label == key);
            let seen = found.map(|option| (option.detail.as_str(), option.type_label.as_str()));
            if seen != Some((detail, type_label)) {
                failures.push(format!("{language} {key}: {seen:?}"));
            }
        }
    }
    report(&failures);
}

#[test]
fn hover_on_a_key_of_an_option_names_the_option_and_a_const_its_value() {
    let file = matched("Static file", "Records read from a file.");
    let both = Some(ChoiceHover::OneOf(vec![
        "Cloudflare".into(),
        "Route 53".into(),
    ]));
    let decided = json!({"dns": {"provider": "static", "path": "/x"}});
    let undecided = json!({"dns": {"ttl": 60}});
    let cases = [
        (&decided, "provider", ("\"static\"", &file)),
        (&decided, "path", ("string", &file)),
        (&undecided, "ttl", ("integer", &both)),
    ];
    let mut failures = Vec::new();
    for language in hovering() {
        for (document, key, want) in cases {
            let seen = hover_at(language, document, &[], &format!("/dns/{key}"))
                .map(|(text, hover)| (text, hover.map(|hover| (hover.type_label, hover.choice))));
            let shown = seen.as_ref().ok().and_then(|(_, hover)| hover.as_ref());
            if shown.map(|(label, choice)| (label.as_str(), choice)) != Some(want) {
                failures.push(format!("{language} {key} in {document}: {seen:?}"));
            }
        }
    }
    report(&failures);
}
