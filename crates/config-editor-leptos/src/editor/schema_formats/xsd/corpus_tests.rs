//! Real schemas (Maven POM 4.0.0, Log4j 2, Spring beans, JUnit XML, a
//! .NET-style config; the four upstream ones under the Apache License 2.0)
//! in `tests/fixtures/xsd`:
//!
//! - each instance gets the verdict libxml2 (`xmllint --schema`) gave it,
//!   run once offline and checked in as `verdicts.json`; where ours differs,
//!   the difference is a loss the compiler reports;
//! - the compiler and the XML reader agree (design section 6): for every
//!   global element, a minimal and a maximal instance generated from the
//!   compiled schema, written as XML, read back and validated, pass.

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::super::{SchemaFormat, SchemaInput};
use crate::editor::Language;
use crate::editor::completion::resolve_branches;
use crate::editor::diagnostics::Schema;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xsd")
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn compile(name: &str) -> Schema {
    Schema::compile_as(
        SchemaFormat::XSD,
        &SchemaInput::new(&fixture(name)).at(name),
    )
    .unwrap_or_else(|error| panic!("{name}: {}", error.0))
}

const SCHEMAS: [&str; 6] = [
    "maven-4.0.0.xsd",
    "log4j-config-2.xsd",
    "spring-beans.xsd",
    "junit.xsd",
    "dotnet-config.xsd",
    "inventory.xsd",
];

/// Invalid instances only the checks beside the JSON Schema catch (its
/// JSON Schema alone takes them).
const ONLY_THE_CHECKS_CATCH: [&str; 8] = [
    "junit-out-of-order.xml",
    "beans-unknown-attribute.xml",
    "beans-two-values.xml",
    "inventory-dangling-ref.xml",
    "inventory-duplicate-code.xml",
    "inventory-duplicate-sku.xml",
    "inventory-out-of-order.xml",
    "inventory-stock-out-of-order.xml",
];

#[test]
fn some_invalid_instances_are_caught_by_the_checks_alone() {
    let verdicts: Value = serde_json::from_str(&fixture("verdicts.json")).unwrap();
    for instance in ONLY_THE_CHECKS_CATCH {
        let verdict = &verdicts["verdicts"][instance];
        assert_eq!(verdict["valid"], json!(false), "{instance}");
        let schema = compile(verdict["schema"].as_str().unwrap());
        let document = fixture(instance);
        assert!(
            !schema.validate(Language::XML, &document).is_empty(),
            "{instance}: the checks find nothing"
        );
        let plain = Schema::compile(&schema.json().to_string()).unwrap();
        let alone = plain.validate(Language::XML, &document);
        assert!(
            alone.is_empty(),
            "{instance}: the JSON Schema alone finds {alone:?}"
        );
    }
}

/// Instances whose verdict differs from libxml2's, and the construct of
/// the loss that says why: an element order, and an unknown attribute
/// where `xs:any` allows elements of other namespaces (the data model keys
/// elements by local name, so it allows any key there).
const KNOWN_DIFFERENCES: [(&str, &str); 0] = [];

#[test]
fn every_corpus_instance_gets_the_oracles_verdict_or_a_reported_loss() {
    let verdicts: Value = serde_json::from_str(&fixture("verdicts.json")).unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for (instance, verdict) in verdicts["verdicts"].as_object().unwrap() {
        let schema = compile(verdict["schema"].as_str().unwrap());
        let problems = schema.validate(Language::XML, &fixture(instance));
        let ours = problems.is_empty();
        let oracle = verdict["valid"].as_bool().unwrap();
        checked += 1;
        if ours == oracle {
            continue;
        }
        match KNOWN_DIFFERENCES.iter().find(|(name, _)| name == instance) {
            Some((_, construct))
                if schema
                    .losses()
                    .iter()
                    .any(|loss| loss.construct == *construct) => {}
            Some((_, construct)) => failures.push(format!(
                "{instance}: differs, and no {construct} loss is reported"
            )),
            None => failures.push(format!(
                "{instance}: ours {ours} ({problems:?}), libxml2 {oracle} ({})",
                verdict["libxml2"]
            )),
        }
    }
    assert!(checked >= 17, "{checked}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A value of `schema` (resolved in `root`): its const, default or first
/// enum value, else one of its type; objects with their required keys
/// (`maximal`: every key but other options of a choice), up to `depth`.
fn generate(schema: &Value, root: &Value, maximal: bool, depth: usize) -> Option<Value> {
    generate_at(schema, root, maximal, depth, 0)
}

fn generate_at(
    schema: &Value,
    root: &Value,
    maximal: bool,
    depth: usize,
    level: usize,
) -> Option<Value> {
    if level > 16 {
        return None;
    }
    let branches = resolve_branches(schema, root);
    for branch in &branches {
        if let Some(value) = branch
            .get("const")
            .or_else(|| branch.get("default"))
            .or_else(|| branch.get("enum").and_then(|values| values.get(0)))
        {
            return Some(value.clone());
        }
    }
    // A scalar branch first: what an element without attributes reads as.
    let typed = |name: &str| {
        branches
            .iter()
            .copied()
            .find(|branch| branch.get("type").and_then(Value::as_str) == Some(name))
    };
    if let Some(branch) = typed("integer").or_else(|| typed("number")) {
        let minimum = branch.get("minimum").and_then(Value::as_i64).unwrap_or(0);
        let maximum = branch
            .get("maximum")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        return Some(json!(minimum.max(1).min(maximum)));
    }
    if typed("boolean").is_some() {
        return Some(json!(true));
    }
    if let Some(branch) = typed("string") {
        if branch.get("pattern").is_some() {
            return None;
        }
        let text = match branch.get("format").and_then(Value::as_str) {
            Some("date-time") => "2026-01-01T00:00:00".to_string(),
            Some("date") => "2026-01-01".to_string(),
            Some("time") => "10:00:00".to_string(),
            Some("duration") => "PT1S".to_string(),
            Some("uri-reference") => "https://example.com".to_string(),
            _ => "x".repeat(
                branch
                    .get("minLength")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1) as usize,
            ),
        };
        return Some(Value::String(text));
    }
    if let Some(branch) = branches
        .iter()
        .copied()
        .find(|branch| branch.get("items").is_some())
    {
        let item = generate_at(&branch["items"], root, maximal, depth, level + 1)?;
        let count = branch
            .get("minItems")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1);
        return Some(Value::Array(vec![item; count as usize]));
    }
    let object = branches
        .iter()
        .copied()
        .find(|branch| branch.get("properties").is_some())?;
    let mut required: HashSet<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut excluded = HashSet::new();
    let choices: Vec<&Value> = object
        .get("oneOf")
        .into_iter()
        .chain(
            object
                .get("allOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|all| all.get("oneOf")),
        )
        .collect();
    for choice in choices {
        for (index, option) in choice.as_array().into_iter().flatten().enumerate() {
            for key in option
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if index == 0 {
                    required.insert(key);
                } else {
                    excluded.insert(key);
                }
            }
        }
    }
    // The content model's elements, the first option of each choice: a
    // maximal instance takes no more than one option.
    let mut named = HashSet::new();
    let mut chosen = HashSet::new();
    if let Some(model) = object.pointer("/x-xsd-content/model") {
        model_names(model, &mut named, &mut chosen, true);
    }
    let mut out = Map::new();
    for (key, property) in object["properties"].as_object()? {
        let in_model = !named.contains(key.as_str()) || chosen.contains(key.as_str());
        let wanted = required.contains(key.as_str())
            || maximal && depth > 0 && !excluded.contains(key.as_str()) && in_model;
        if !wanted || key == "#text" && !required.contains("#text") {
            continue;
        }
        match generate_at(property, root, maximal, depth.saturating_sub(1), level + 1) {
            Some(value) => {
                out.insert(key.clone(), value);
            }
            None if required.contains(key.as_str()) => return None,
            None => {}
        }
    }
    Some(Value::Object(out))
}

/// The element names of content model `model` (`x-xsd-content`) into
/// `named`, and those of its first path (each choice's first option) into
/// `chosen` while `taken`.
fn model_names<'m>(
    model: &'m Value,
    named: &mut HashSet<&'m str>,
    chosen: &mut HashSet<&'m str>,
    taken: bool,
) {
    if let Some(name) = model.get("element").and_then(Value::as_str) {
        named.insert(name);
        if taken {
            chosen.insert(name);
        }
    }
    for kind in ["sequence", "all", "choice"] {
        for (index, item) in model
            .get(kind)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            model_names(
                item,
                named,
                chosen,
                taken && (kind != "choice" || index == 0),
            );
        }
    }
}

#[test]
fn instances_generated_from_each_compiled_schema_read_back_valid() {
    let mut failures = Vec::new();
    let mut generated = 0;
    for name in SCHEMAS {
        let schema = compile(name);
        let root = schema.bundle.schema.clone();
        for (element, property) in root["properties"].as_object().unwrap() {
            for maximal in [false, true] {
                let Some(value) = generate(property, &root, maximal, 3) else {
                    continue;
                };
                let document = json!({ element: value });
                let text =
                    match schema.convert(Language::JSON, &document.to_string(), Language::XML) {
                        Ok(converted) => converted.text,
                        Err(error) => {
                            failures.push(format!(
                                "{name} {element} (maximal {maximal}): not written: {error}"
                            ));
                            continue;
                        }
                    };
                generated += 1;
                let problems = schema.validate(Language::XML, &text);
                if !problems.is_empty() {
                    let messages: Vec<&str> = problems
                        .iter()
                        .map(|problem| problem.message.as_str())
                        .take(3)
                        .collect();
                    failures.push(format!(
                        "{name} {element} (maximal {maximal}): {messages:?}\n{text}"
                    ));
                }
            }
        }
    }
    assert!(generated >= 40, "only {generated} generated");
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures[..failures.len().min(6)].join("\n\n")
    );
}

/// Cross-language: an XSD validates every view of a document alike. Each
/// corpus instance converted (by the schema) to every other language gives
/// the problems it gives in XML, at the same pointers.
#[cfg(all(feature = "yaml", feature = "toml", feature = "hcl"))]
#[test]
fn every_view_of_a_corpus_instance_has_the_same_problems() {
    let verdicts: Value = serde_json::from_str(&fixture("verdicts.json")).unwrap();
    let mut failures = Vec::new();
    let mut compared = 0;
    for (instance, verdict) in verdicts["verdicts"].as_object().unwrap() {
        let schema = compile(verdict["schema"].as_str().unwrap());
        let xml = fixture(instance);
        let messages = |language: Language, text: &str| {
            let mut found: Vec<String> = schema
                .validate(language, text)
                .into_iter()
                .map(|problem| problem.message)
                .collect();
            found.sort();
            found
        };
        let expected = messages(Language::XML, &xml);
        // A value named as both an attribute and a child element: another
        // language's view may read it as either (the reported loss).
        if instance == "beans-two-values.xml" {
            assert!(
                schema
                    .losses()
                    .iter()
                    .any(|loss| loss.construct == "attribute named as an element")
            );
            continue;
        }
        // Elements out of order: no other language keeps XML's order (the
        // reported "xs:sequence order" loss).
        if instance.ends_with("-out-of-order.xml") && !expected.is_empty() {
            assert!(
                schema
                    .losses()
                    .iter()
                    .any(|loss| loss.construct == "xs:sequence order")
            );
            continue;
        }
        for language in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            // A language that cannot hold the document (a key HCL cannot
            // name) is no view of it.
            let Ok(converted) = schema.convert(Language::XML, &xml, language) else {
                continue;
            };
            if !converted.losses.is_empty() {
                continue;
            }
            compared += 1;
            let found = messages(language, &converted.text);
            if found != expected {
                failures.push(format!(
                    "{instance} as {language}: {found:?}, XML {expected:?}\n{}",
                    converted.text
                ));
            }
        }
    }
    assert!(compared >= 40, "only {compared} views compared");
    assert!(
        failures.is_empty(),
        "{}",
        failures[..failures.len().min(5)].join("\n\n")
    );
}

/// The losses of each corpus schema are each placed on a line of it.
#[test]
fn every_corpus_loss_is_placed() {
    for name in SCHEMAS {
        let lines = fixture(name).lines().count();
        for loss in compile(name).losses() {
            assert!(
                loss.line.is_some_and(|line| line >= 1 && line <= lines),
                "{name}: {loss}"
            );
        }
    }
}
