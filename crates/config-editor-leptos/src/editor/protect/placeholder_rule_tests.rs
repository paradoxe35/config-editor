//! An untouched placeholder stands for a secret the editor does not know,
//! in every language: no rule judges it by its value. Two items holding
//! the same token are not repeated items (the token stands for each
//! field's own secret), a placeholder may be the item a `contains` wants,
//! and an `enum` of the item holding one is not checked. A problem the
//! placeholders cannot undo still shows: a real item written twice
//! beside them, a list too short for its `minContains`.

use super::super::Language;
use super::super::diagnostics::{build_schema, validate, validate_with};
use super::super::secret_options::SecretOptions;
use super::tokens::SecretMatcher;

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "ks": {"type": "array", "uniqueItems": true, "items": {"type": "string", "writeOnly": true}},
    "ko": {"type": "array", "uniqueItems": true,
           "items": {"type": "object", "properties": {"tok": {"type": "string", "writeOnly": true}}}},
    "kc": {"type": "array", "contains": {"type": "string", "writeOnly": true, "minLength": 30},
           "items": {"type": "string", "writeOnly": true}},
    "km": {"type": "array", "contains": {"type": "string", "writeOnly": true, "minLength": 30},
           "maxContains": 1, "items": {"type": "string", "writeOnly": true}},
    "kn": {"type": "array", "contains": {"type": "string", "writeOnly": true}, "minContains": 2,
           "items": {"type": "string", "writeOnly": true}},
    "ke": {"type": "array", "items": {"enum": ["a"], "writeOnly": true}},
    "eo": {"type": "array", "items": {"enum": [{"tok": "a"}], "properties": {"tok": {"writeOnly": true}}}},
    "ao": {"anyOf": [{"type": "object", "properties": {"tok": {"writeOnly": true, "minLength": 30}}}, {"type": "string"}]}
  }
}"#;

/// The same document in every language: placeholders only.
fn untouched() -> Vec<(Language, String)> {
    let yaml = "ks: [«secret aa», «secret aa»]
ko:
  - tok: «secret aa»
  - tok: «secret aa»
kc: [«secret bb»]
km: [«secret bb», «secret cc»]
ke: [«secret cc»]
eo:
  - tok: «secret cc»
ao: {tok: «secret dd»}
";
    every_language(yaml)
}

fn every_language(yaml: &str) -> Vec<(Language, String)> {
    [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ]
    .into_iter()
    .map(|language| {
        (
            language,
            Language::YAML.convert(yaml, language).unwrap().text,
        )
    })
    .collect()
}

/// The problems of `text`, its placeholders accepted.
fn problems(language: Language, text: &str) -> Vec<String> {
    let bundle = build_schema(SCHEMA).unwrap();
    let accepted: Vec<(String, String)> = SecretMatcher::new(&SecretOptions::default())
        .known(language.behavior(), text, None)
        .into_iter()
        .filter_map(|known| Some((known.token, known.field?)))
        .collect();
    validate_with(
        text,
        Some(&bundle),
        None,
        true,
        language.behavior(),
        &accepted,
        crate::strings::default_strings(),
    )
    .into_iter()
    .map(|problem| problem.message)
    .collect()
}

#[test]
fn no_rule_judges_an_untouched_placeholder_by_its_value() {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, text) in untouched() {
        let found = problems(language, &text);
        if !found.is_empty() {
            failures.push(format!("{language}: {found:#?}\n{text}"));
        }
        // Read as the values they are written as, the rules refuse them.
        let unaccepted = validate(&text, Some(&bundle), None, true, language.behavior());
        if unaccepted.len() < 3 {
            failures.push(format!(
                "{language}: only {unaccepted:#?} without the placeholders"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_problem_the_placeholders_cannot_undo_still_shows() {
    let yaml = "ks: [«secret aa», «secret aa», x, x]
kn: [«secret bb»]
";
    let mut failures = Vec::new();
    for (language, text) in every_language(yaml) {
        let found = problems(language, &text);
        let wanted = [
            "/ks: the list holds the secret value more than once: each item must differ",
            "/kn: fewer than 2 items of the list match its \"contains\" schema",
        ];
        if found != wanted {
            failures.push(format!("{language}: {found:#?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
