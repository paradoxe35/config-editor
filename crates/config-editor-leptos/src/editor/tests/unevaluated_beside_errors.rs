//! A value that fails inside an applicator (`allOf`, `$ref`, …) makes its
//! whole subschema fail, which then evaluates none of its keys: the
//! validator's `unevaluatedProperties` (or `unevaluatedItems`) would refuse
//! keys that subschema declares. Only the real error is shown; a key no
//! applied subschema could evaluate is still refused. In every language,
//! and in an Edit-as view.

use leptos::prelude::*;

use crate::editor::Language;
use crate::editor::diagnostics::Schema;
use crate::editor::test_state::TestEditor;

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

const DRAFT: &str = r#""$schema":"https://json-schema.org/draft/2020-12/schema""#;

/// The problems' messages `yaml` written in `language` gets, sorted.
fn messages(language: Language, yaml: &str, schema: &str) -> (String, Vec<String>) {
    let text = Language::YAML.convert(yaml, language).unwrap().text;
    let mut found: Vec<String> = Schema::compile(schema)
        .unwrap()
        .validate(language, &text)
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    found.sort();
    (text, found)
}

fn check(yaml: &str, schema: &str, expected: &[&str], failures: &mut Vec<String>) {
    let mut wanted: Vec<String> = expected.iter().map(ToString::to_string).collect();
    wanted.sort();
    for language in LANGUAGES {
        let (text, found) = messages(language, yaml, schema);
        if found != wanted {
            failures.push(format!(
                "{language}\n{text}\nfound  {found:?}\nwanted {wanted:?}"
            ));
        }
    }
}

/// `top` declared through an `allOf`, refusing what it does not evaluate.
fn all_of(properties: &str) -> String {
    format!(
        r#"{{{DRAFT},"type":"object","properties":{{"top":{{"type":"object","allOf":[{{"properties":{{{properties}}}}}],"unevaluatedProperties":false}}}}}}"#
    )
}

#[test]
fn a_declared_key_beside_a_failing_value_is_not_refused() {
    let mut failures = Vec::new();
    let schema = all_of(r#""a":{"type":"integer"},"b":{"type":"integer"}"#);
    check(
        "top:\n  a: 1\n  b: x\n",
        &schema,
        &[r#"/top/b: "x" is not of type "integer""#],
        &mut failures,
    );
    // A key no subschema declares is still refused.
    check(
        "top:\n  a: 1\n  b: x\n  c: 2\n",
        &schema,
        &[
            r#"/top/b: "x" is not of type "integer""#,
            r#"/top: "c" is not allowed here"#,
        ],
        &mut failures,
    );
    // A key a pattern declares, or every key another subschema's
    // `additionalProperties` takes, is declared.
    check(
        "top:\n  a: x\n  x-note: 1\n",
        &all_of(r#""a":{"type":"integer"}},"patternProperties":{"^x-":{}"#),
        &[r#"/top/a: "x" is not of type "integer""#],
        &mut failures,
    );
    // The `else` the value takes declares; the `then` it does not take
    // does not.
    let conditional = format!(
        r#"{{{DRAFT},"type":"object","properties":{{"top":{{"type":"object","properties":{{"kind":{{}}}},"if":{{"properties":{{"kind":{{"const":"a"}}}}}},"then":{{"properties":{{"one":{{"type":"integer"}}}}}},"else":{{"properties":{{"two":{{"type":"integer"}}}}}},"unevaluatedProperties":false}}}}}}"#
    );
    check(
        "top:\n  kind: b\n  two: x\n  one: 1\n",
        &conditional,
        &[
            r#"/top/two: "x" is not of type "integer""#,
            r#"/top: "one" is not allowed here"#,
        ],
        &mut failures,
    );
    // Without another error the validator's answer stands: the `oneOf`
    // option `kind` picks evaluates `one`, not the other option's `two`.
    let one_of = format!(
        r#"{{{DRAFT},"type":"object","properties":{{"top":{{"type":"object","oneOf":[{{"properties":{{"kind":{{"const":"a"}},"one":{{}}}}}},{{"properties":{{"kind":{{"const":"b"}},"two":{{}}}}}}],"unevaluatedProperties":false}}}}}}"#
    );
    check(
        "top:\n  kind: a\n  two: 1\n",
        &one_of,
        &[r#"/top: "two" is not allowed here"#],
        &mut failures,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The official Compose specification's service: an everyday mistake
/// under `healthcheck` (a number where it takes a duration's text; XML
/// reads it as text) and a value no language reads as allowed (`cgroup`)
/// are the only problems.
#[test]
fn a_compose_service_with_a_type_error_reports_only_it() {
    let schema = include_str!("../../../tests/fixtures/real/compose-spec.json");
    let yaml = "services:\n  web:\n    image: nginx:1.27\n    ports:\n      - \"8080:80\"\n    healthcheck:\n      test: CMD true\n      interval: 10\n    cgroup: shared\n";
    let interval = r#"/services/web/healthcheck/interval: 10 is not of type "string""#;
    let cgroup = r#"/services/web/cgroup: "shared" is not one of "host", "private""#;
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (extra, refused) in [
            ("", None),
            // A key no part of a service declares is still refused.
            (
                "    imagee: x\n",
                Some(r#"/services/web: "imagee" is not allowed here"#),
            ),
        ] {
            let mut wanted: Vec<String> = [Some(cgroup), refused]
                .into_iter()
                .flatten()
                .chain((language != Language::XML).then_some(interval))
                .map(ToString::to_string)
                .collect();
            wanted.sort();
            let (text, found) = messages(language, &format!("{yaml}{extra}"), schema);
            if found != wanted {
                failures.push(format!(
                    "{language}\n{text}\nfound  {found:?}\nwanted {wanted:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Items alike: an item a tuple position declares is not refused beside
/// a failing one; an item past every position still is.
#[test]
fn a_declared_item_beside_a_failing_one_is_not_refused() {
    let schema = format!(
        r#"{{{DRAFT},"type":"object","properties":{{"top":{{"type":"array","allOf":[{{"prefixItems":[{{"type":"integer"}},{{"type":"integer"}}]}}],"unevaluatedItems":false}}}}}}"#
    );
    let mut failures = Vec::new();
    check(
        "top:\n  - 1\n  - x\n",
        &schema,
        &[r#"/top/1: "x" is not of type "integer""#],
        &mut failures,
    );
    let (_, found) = messages(Language::YAML, "top:\n  - 1\n  - x\n  - 3\n", &schema);
    assert!(
        found.len() == 2 && found.iter().any(|message| message.contains('3')),
        "the third item is still refused: {found:?}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// An Edit-as view validates what it shows the same way.
#[test]
fn an_edit_as_view_reports_only_the_real_error() {
    let schema = std::sync::Arc::new(
        crate::editor::diagnostics::build_schema(&all_of(
            r#""a":{"type":"integer"},"b":{"type":"integer"}"#,
        ))
        .unwrap(),
    );
    for view in [Language::JSON, Language::TOML, Language::HCL, Language::XML] {
        let host = "top:\n  a: 1\n  b: x\n";
        let (_owner, state) = TestEditor {
            edit_as: Some(view),
            schema: Some(schema.clone()),
            ..TestEditor::new(Language::YAML, host)
        }
        .mount();
        let text = Language::YAML.convert(host, view).unwrap().text;
        state.doc().text.set(text.clone());
        state.problems().diagnosed_text.set(text);
        let found: Vec<String> = state
            .problems()
            .diagnostics
            .get_untracked()
            .into_iter()
            .map(|problem| problem.message)
            .collect();
        assert_eq!(found, [r#"/top/b: "x" is not of type "integer""#], "{view}");
    }
}
