//! A key already written is not offered again, whatever its kind (a
//! scalar, an object, a list of scalars, a list of objects), in every
//! language; except a list the language writes by repeating its items
//! (XML elements, HCL blocks), where accepting adds one more item written
//! the same way. A list of objects written whole in HCL (`containers =
//! [ … ]`) is no such list.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::schema::build_schema;
use super::Language;
use super::harness::{At, completing, report, text, write};
use serde_json::{Value, json};

const SCHEMA: &str = r#"{"type":"object","properties":{"spec":{"type":"object","properties":{
    "containers":{"type":"array","items":{"type":"object","required":["name"],
        "properties":{"name":{"type":"string"},"image":{"type":"string"}}}},
    "tags":{"type":"array","items":{"type":"string"}},
    "meta":{"type":"object","properties":{"a":{"type":"string"}}},
    "other":{"type":"integer"}}}}}"#;

/// Forms written by hand, beside those each language writes itself (below):
/// each case the document with the caret on a new line in `spec`, and
/// how many `containers` items accepting the offered `containers` leaves
/// (`None`: not offered).
const CASES: &[(Language, &str, Option<usize>)] = &[
    // HCL: written whole, on one line and over lines: not offered.
    (
        Language::HCL,
        "spec {\n  containers = [{ name = \"web\" }]\n  |\n}\n",
        None,
    ),
    (
        Language::HCL,
        "spec {\n  containers = [\n    {\n      name = \"web\"\n    }\n  ]\n  |\n}\n",
        None,
    ),
    // HCL: as blocks, once and twice: one more block.
    (
        Language::HCL,
        "spec {\n  containers {\n    name = \"web\"\n  }\n  |\n}\n",
        Some(2),
    ),
    (
        Language::HCL,
        "spec {\n  containers {\n    name = \"a\"\n  }\n  containers {\n    name = \"b\"\n  }\n  |\n}\n",
        Some(3),
    ),
    // HCL: the caret before the blocks.
    (
        Language::HCL,
        "spec {\n  |\n  containers {\n    name = \"web\"\n  }\n}\n",
        Some(2),
    ),
    // XML: one more element.
    (
        Language::XML,
        "<spec>\n  <containers>\n    <name>web</name>\n  </containers>\n  <|\n</spec>\n",
        Some(2),
    ),
    // The others write a list whole: not offered.
    (
        Language::YAML,
        "spec:\n  containers:\n    - name: web\n  |\n",
        None,
    ),
    (
        Language::JSON,
        "{\n  \"spec\": {\n    \"containers\": [{\"name\": \"web\"}],\n    |\n  }\n}\n",
        None,
    ),
    (
        Language::TOML,
        "[spec]\ncontainers = [{ name = \"web\" }]\n|\n",
        None,
    ),
];

/// Other kinds of key, written: never offered again. Forms written by hand,
/// beside those each language writes itself.
const WRITTEN: &[(Language, &str)] = &[
    (
        Language::HCL,
        "spec {\n  other = 1\n  tags = [\"a\"]\n  meta = { a = \"x\" }\n  |\n}\n",
    ),
    (
        Language::HCL,
        "spec {\n  other = 1\n  tags = [\"a\"]\n  meta {\n    a = \"x\"\n  }\n  |\n}\n",
    ),
    (
        Language::YAML,
        "spec:\n  other: 1\n  tags: [a]\n  meta: {a: x}\n  |\n",
    ),
    (
        Language::JSON,
        "{\"spec\": {\n  \"other\": 1, \"tags\": [\"a\"], \"meta\": {\"a\": \"x\"},\n  |\n}}\n",
    ),
    (
        Language::TOML,
        "[spec]\nother = 1\ntags = [\"a\"]\nmeta = { a = \"x\" }\n|\n",
    ),
    (
        Language::XML,
        "<spec>\n  <other>1</other>\n  <meta><a>x</a></meta>\n  <|\n</spec>\n",
    ),
];

fn labels_and_accept(
    language: Language,
    marked: &str,
    label: &str,
) -> (Vec<String>, Option<String>) {
    let bundle = build_schema(SCHEMA).unwrap();
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(&source, at, true, Some(&bundle), true, language.behavior());
    let labels = options.iter().map(|option| option.label.clone()).collect();
    let accepted = options
        .iter()
        .find(|option| option.label == label)
        .map(|option| apply_completion(&source, option).0);
    (labels, accepted)
}

/// Whether `language` writes a list of objects by repeating its key, once
/// per item (XML elements, HCL blocks).
fn repeats_items(language: Language) -> bool {
    repeats(
        language,
        "containers",
        &json!([{"name": "a"}, {"name": "b"}]),
    )
}

/// Whether `language` writes the list `list` of `key` by repeating the key,
/// once per item (or its tags around each).
fn repeats(language: Language, key: &str, list: &Value) -> bool {
    text(language, &json!({"spec": { key: list }}), &[])
        .is_ok_and(|text| text.matches(key).count() >= 2)
}

/// Languages that write a list of objects by repeating its key, but do not
/// offer the key again to add an item: open findings, checked by the
/// ignored test below until fixed.
const NOT_OFFERED_AGAIN: &[&str] = &["TOML"];

/// The caret on a new entry of `spec` holding `spec`, in every language.
fn written_cases(spec: &Value, failures: &mut Vec<String>) -> Vec<(Language, String)> {
    let at = At::NewKey {
        object: "/spec",
        typed: "",
    };
    let mut cases = Vec::new();
    for language in completing() {
        match write(language, &json!({ "spec": spec }), &[], at) {
            Ok(written) => cases.push((language, written.marked())),
            Err(error) => failures.push(error),
        }
    }
    cases
}

#[test]
fn a_written_list_of_objects_is_offered_again_only_where_items_repeat() {
    report(&lists_offered_again(false));
}

#[test]
#[ignore = "bug: TOML does not offer a list of objects written as an array of tables (`[[spec.containers]]`) again, to add one more table, as XML and HCL do for theirs"]
fn a_toml_array_of_tables_is_offered_again() {
    report(&lists_offered_again(true));
}

/// The cases of a written list of objects, by hand and as each language
/// writes it: those of [`NOT_OFFERED_AGAIN`] only (`known`), or all the
/// others. Why each failing one fails.
fn lists_offered_again(known: bool) -> Vec<String> {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    let mut cases: Vec<(Language, String, Option<usize>)> = CASES
        .iter()
        .filter(|_| !known)
        .map(|(language, marked, wanted)| (*language, (*marked).to_string(), *wanted))
        .collect();
    for (items, spec) in [
        (1, json!({"containers": [{"name": "web"}]})),
        (2, json!({"containers": [{"name": "a"}, {"name": "b"}]})),
    ] {
        for (language, marked) in written_cases(&spec, &mut failures) {
            if NOT_OFFERED_AGAIN.contains(&language.label()) != known {
                continue;
            }
            let wanted = repeats_items(language).then_some(items + 1);
            cases.push((language, marked, wanted));
        }
    }
    for (language, marked, wanted) in &cases {
        let (labels, accepted) = labels_and_accept(*language, marked, "containers");
        let items = accepted.as_ref().map(|text| {
            super::super::codec::read(language.behavior(), text, Some(&bundle.schema))
                .ok()
                .and_then(|read| {
                    read.pointer("/spec/containers")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                })
        });
        if items != wanted.map(Some) {
            failures.push(format!(
                "{language} {marked:?}: offered {labels:?}, accepted {accepted:?}"
            ));
        }
    }
    failures
}

#[test]
fn a_written_key_of_any_other_kind_is_not_offered_again() {
    let mut failures = Vec::new();
    let mut cases: Vec<(Language, String)> = WRITTEN
        .iter()
        .map(|(language, marked)| (*language, (*marked).to_string()))
        .collect();
    let spec = json!({"other": 1, "tags": ["a", "b"], "meta": {"a": "x"}});
    cases.extend(written_cases(&spec, &mut failures));
    for (language, marked) in &cases {
        let (labels, _) = labels_and_accept(*language, marked, "");
        // A list the language writes by repeating its key may be offered
        // again, to add an item (as `containers` above).
        let repeated: Vec<&String> = labels
            .iter()
            .filter(|label| {
                ["other", "tags", "meta"].contains(&label.as_str())
                    && marked.contains(label.as_str())
                    && !(label.as_str() == "tags" && repeats(*language, "tags", &json!(["a", "b"])))
            })
            .collect();
        if !repeated.is_empty() || !labels.iter().any(|label| label == "containers") {
            failures.push(format!("{language} {marked:?}: offered {labels:?}"));
        }
    }
    report(&failures);
}

/// A key a `not` keeps apart from the keys written (`path` written) is
/// not offered; the others still are. Each shape of such a `not`: a
/// `required` pair, with properties only naming the keys (as Cargo's
/// schema writes it), a key's dependent schema (`dependentSchemas`, draft
/// 7's `dependencies`) and an `anyOf` of pairs.
#[test]
fn a_key_a_not_keeps_apart_from_written_keys_is_not_offered() {
    let object = |kept_apart: &str| {
        format!(
            r#"{{"type":"object","properties":{{"dep":{{"type":"object",
            "properties":{{"version":{{"type":"string"}},"git":{{"type":"string"}},"path":{{"type":"string"}}}},
            {kept_apart}}}}}}}"#
        )
    };
    let schemas = [
        object(r#""allOf":[{"not":{"required":["git","path"]}}]"#),
        object(
            r#""allOf":[{"not":{"properties":{"git":true,"path":true},"required":["git","path"]}}]"#,
        ),
        object(r#""dependentSchemas":{"git":{"not":{"required":["path"]}}}"#),
        object(r#""dependencies":{"git":{"not":{"required":["path"]}}}"#).replacen(
            '{',
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","#,
            1,
        ),
        object(r#""not":{"anyOf":[{"required":["version","path"]},{"required":["git","path"]}]}"#),
    ];
    let mut failures = Vec::new();
    let at = At::NewKey {
        object: "/dep",
        typed: "",
    };
    let documents: Vec<(Language, String)> = completing()
        .into_iter()
        .filter_map(
            |language| match write(language, &json!({"dep": {"path": "p"}}), &[], at) {
                Ok(written) => Some((language, written.marked())),
                Err(error) => {
                    failures.push(error);
                    None
                }
            },
        )
        .collect();
    for (shape, schema) in schemas.iter().enumerate() {
        let bundle = build_schema(schema).unwrap();
        // The `anyOf` keeps `version` from `path` too.
        let wanted: &[&str] = if shape == 4 { &[] } else { &["version"] };
        for (language, marked) in &documents {
            let language = *language;
            let at = marked.find('|').unwrap();
            let source = marked.replacen('|', "", 1);
            let labels: Vec<String> =
                completions(&source, at, true, Some(&bundle), true, language.behavior())
                    .into_iter()
                    .map(|option| option.label)
                    .collect();
            if labels != wanted {
                failures.push(format!("shape {shape}, {language}: offered {labels:?}"));
            }
        }
    }
    report(&failures);
}
