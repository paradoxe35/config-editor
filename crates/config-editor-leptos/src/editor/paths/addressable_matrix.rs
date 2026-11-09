//! Every path of a document can be found in it: each container and leaf
//! (list items written as blocks, repeated HCL blocks, TOML `[[arrays]]`
//! included) has a range, `path_at` at its start gives the path back, and
//! a schema problem with a list item is marked on that item. Five
//! languages, lines ended by LF and CRLF; hand-written documents and random
//! data each serializer writes.

use serde_json::{Value, json};

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::host_diagnostics::{HostDiagnostic, RangeText, place};
use super::super::language::Repetition;
use super::ranges_matrix::{cases as written_cases, pointers};
use super::{TextPosition, same_field};
use crate::Schema;
use crate::strings::default_strings;

/// Lists of objects and lists, the way each language writes them.
fn lists() -> Vec<(Language, &'static str)> {
    vec![
        (Language::YAML, "l:\n  - n: 1\n  - n: 2\n    m: [1, 2]\n"),
        (Language::YAML, "l:\n- n: 1\n- n: 2\n"),
        (
            Language::YAML,
            "l:\n  - - a\n    - b\n  - [c]\n  - {n: 1}\n",
        ),
        (Language::YAML, "- n: 1\n- n: 2\n"),
        (Language::YAML, "l:\n  -\n    x: 1\n  - a\n"),
        (
            Language::YAML,
            "l:\n  - ? k\n    : v\n  - ? m\n    :\n      x: 1\n? top\n: 2\n",
        ),
        (
            Language::YAML,
            "spec:\n  containers:\n    - name: web\n      ports:\n        - containerPort: 80\n        - containerPort: 443\n    - name: side\n",
        ),
        (Language::JSON, "{\"l\": [{\"n\": 1}, {\"n\": 2}]}\n"),
        (Language::JSON, "[\n  {\"n\": 1},\n  [1, 2]\n]\n"),
        (Language::TOML, "[[l]]\nn = 1\n\n[[l]]\nn = 2\n"),
        (
            Language::TOML,
            "a = 1\n\n[t]\n[[t.l]]\nn = 1\n[[t.l]]\nn = 2\n",
        ),
        (Language::TOML, "l = [{n = 1}, {n = 2}]\n"),
        // Arrays of tables in a later entry of their parent count from 0.
        (
            Language::TOML,
            "[[r]]\n[[r.t]]\nq = 1\n[[r.t]]\nq = 2\n[[r]]\n[[r.t]]\nq = 3\n",
        ),
        (
            Language::TOML,
            "[[root]]\n[[root.c]]\nx = 1\n[[root]]\n[root.c.a]\ny = 2\n",
        ),
        (Language::HCL, "b {\n  x = 1\n}\nb {\n  x = 2\n}\n"),
        (
            Language::HCL,
            "c \"l\" {\n  y = 1\n}\nc \"m\" {\n  y = 2\n}\nd \"l\" {\n  y = 1\n}\nd \"l\" {\n  y = 2\n}\n",
        ),
        (Language::HCL, "l = [{n = 1}, {n = 2}]\n"),
        (
            Language::XML,
            "<r>\n  <x>1</x>\n  <x>2</x>\n  <y><n>1</n></y>\n  <y>\n    <n>2</n>\n  </y>\n</r>\n",
        ),
    ]
}

fn cases() -> Vec<(Language, String)> {
    let mut cases = written_cases();
    cases.extend(
        lists()
            .into_iter()
            .map(|(language, text)| (language, text.to_string())),
    );
    cases.extend(generated());
    cases
}

/// Random data written by each language's serializer: nested lists of
/// objects (TOML `[[a.b]]` in later entries of `[[a]]`), implicit tables,
/// lists of lists and scalars.
fn generated() -> Vec<(Language, String)> {
    let mut random = Lcg(24);
    let mut cases = Vec::new();
    for _ in 0..120 {
        let mut document = serde_json::Map::new();
        document.insert("root".into(), random_value(&mut random, 0));
        if random.next(2) == 0 {
            document.insert("other".into(), random_value(&mut random, 1));
        }
        let document = Value::Object(document);
        for &language in Language::ALL {
            let value = if language == Language::XML {
                json!({ "doc": document })
            } else {
                document.clone()
            };
            if let Ok(text) = language.serialize(&value) {
                cases.push((language, text));
            }
        }
    }
    cases
}

fn random_value(random: &mut Lcg, depth: usize) -> Value {
    const KEYS: [&str; 4] = ["a", "b", "c", "d"];
    let roll = random.next(20);
    if depth > 3 || roll < 5 {
        return [json!(1), json!("x"), json!(true), json!(2.5)][random.next(4)].clone();
    }
    let count = 1 + random.next(3);
    if roll < 12 {
        let mut members = serde_json::Map::new();
        for _ in 0..count {
            let key = KEYS[random.next(4)];
            members.insert(key.into(), random_value(random, depth + 1));
        }
        return Value::Object(members);
    }
    let objects = random.next(10) < 7;
    Value::Array(
        (0..count)
            .map(|index| {
                if objects {
                    let mut item = serde_json::Map::new();
                    item.insert(KEYS[index % 4].into(), random_value(random, depth + 1));
                    item.insert("z".into(), random_value(random, depth + 2));
                    Value::Object(item)
                } else {
                    random_value(random, depth + 1)
                }
            })
            .collect(),
    )
}

/// A small deterministic generator, so the matrix needs no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

/// Whether `found` (from `path_at`) names the field at `pointer` of
/// `document`: the same field, or for a list written as repeated items (no
/// key of its own), its first item. Only XML and HCL write a lone repeated
/// element or block without its `/0`.
fn names(language: Language, document: &Value, pointer: &str, found: &str) -> bool {
    let same = |one: &str, other: &str| {
        one == other
            || (language.behavior().repetition() != Repetition::None && same_field(one, other))
    };
    same(found, pointer)
        || (document.pointer(pointer).is_some_and(Value::is_array)
            && same(found, &format!("{pointer}/0")))
}

#[test]
fn every_path_has_a_range_that_path_at_names() {
    let mut failures = Vec::new();
    for (language, lf) in cases() {
        let Ok(document) = language.parse(&lf) else {
            failures.push(format!("{language}: does not parse:\n{lf}"));
            continue;
        };
        let mut paths = Vec::new();
        pointers(&document, "", &mut paths);
        for ending in ["\n", "\r\n"] {
            let text = lf.replace('\n', ending);
            let many = language.ranges_of(&text, paths.iter().map(String::as_str));
            for (pointer, many) in paths.iter().zip(many).skip(1) {
                let Some(range) = language.range_of(&text, pointer.as_str()) else {
                    failures.push(format!(
                        "{language} {ending:?} {pointer}: no range in\n{lf}"
                    ));
                    continue;
                };
                if many != Some(range) {
                    failures.push(format!(
                        "{language} {ending:?} {pointer}: ranges_of {many:?}"
                    ));
                }
                let found = language
                    .path_at(&text, range.start)
                    .map(|path| path.to_pointer())
                    .unwrap_or_default();
                if !names(language, &document, pointer, &found) {
                    failures.push(format!(
                        "{language} {ending:?} {pointer}: path_at {:?} gives {found:?} in\n{lf}",
                        range.slice(&text)
                    ));
                }
            }
        }
    }
    report(failures);
}

/// A schema rejecting each item of the list at `target` (and nothing else).
fn rejecting_items(document: &Value, at: &str, target: &str) -> Value {
    if at == target {
        return json!({ "items": false });
    }
    let child = |key: &str| format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
    match document {
        Value::Object(members) => {
            let properties: serde_json::Map<String, Value> = members
                .iter()
                .filter(|(key, _)| {
                    target.starts_with(&format!("{}/", child(key))) || target == child(key)
                })
                .map(|(key, member)| (key.clone(), rejecting_items(member, &child(key), target)))
                .collect();
            json!({ "properties": properties })
        }
        Value::Array(items) => {
            let prefix: Vec<Value> = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let at = child(&index.to_string());
                    if target == at || target.starts_with(&format!("{at}/")) {
                        rejecting_items(item, &at, target)
                    } else {
                        json!({})
                    }
                })
                .collect();
            json!({ "prefixItems": prefix })
        }
        _ => json!({}),
    }
}

#[test]
fn schema_problems_with_list_items_are_marked_on_the_item() {
    let mut failures = Vec::new();
    for (language, lf) in cases() {
        let Ok(document) = language.parse(&lf) else {
            continue;
        };
        let mut paths = Vec::new();
        pointers(&document, "", &mut paths);
        for list in &paths {
            let Some(Value::Array(items)) = document.pointer(list) else {
                continue;
            };
            let schema = Schema::compile(&rejecting_items(&document, "", list).to_string())
                .expect("a valid schema");
            for ending in ["\n", "\r\n"] {
                let text = lf.replace('\n', ending);
                let problems = schema.validate(language, &text);
                for index in 0..items.len() {
                    let item = format!("{list}/{index}");
                    let Some(problem) = problems
                        .iter()
                        .find(|problem| problem.message.starts_with(&format!("{item}:")))
                    else {
                        failures.push(format!(
                            "{language} {ending:?} {item}: no problem among {problems:?}"
                        ));
                        continue;
                    };
                    if let Some(failure) = misplaced(language, &text, &document, &item, problem) {
                        failures.push(format!("{ending:?} schema {failure}"));
                    }
                }
            }
        }
    }
    report(failures);
}

#[test]
fn host_problems_at_any_path_are_marked_on_the_field() {
    let mut failures = Vec::new();
    for (language, lf) in cases() {
        let Ok(document) = language.parse(&lf) else {
            continue;
        };
        let mut paths = Vec::new();
        pointers(&document, "", &mut paths);
        // The text on screen has its lines ended by LF.
        let problems: Vec<HostDiagnostic> = paths
            .iter()
            .skip(1)
            .map(|path| HostDiagnostic::at_path(path.as_str(), Severity::Error, path.clone()))
            .collect();
        let placed = place(
            language.behavior(),
            &lf,
            &problems,
            RangeText::OnScreen,
            default_strings(),
        );
        for path in paths.iter().skip(1) {
            let Some(problem) = placed.iter().find(|problem| problem.message == *path) else {
                failures.push(format!("{language} host {path}: not placed"));
                continue;
            };
            if let Some(failure) = misplaced(language, &lf, &document, path, problem) {
                failures.push(format!("host {failure}"));
            }
        }
    }
    report(failures);
}

/// Why `problem` is not marked on the field at `pointer` (on the line its
/// range starts on, at a place `path_at` names it), or `None`.
fn misplaced(
    language: Language,
    text: &str,
    document: &Value,
    pointer: &str,
    problem: &Diagnostic,
) -> Option<String> {
    let range = language.range_of(text, pointer);
    let at = problem
        .line
        .zip(problem.column)
        .map(|(line, column)| TextPosition::new(line, column));
    let on = at
        .and_then(|at| language.path_at(text, at))
        .map(|path| path.to_pointer())
        .unwrap_or_default();
    (range.map(|range| range.start.line) != at.map(|at| at.line)
        || !names(language, document, pointer, &on))
    .then(|| {
        format!(
            "{language} {pointer}: marked at {at:?} (on {on:?}), the field is at {range:?} in\n{}",
            text.replace('\r', "")
        )
    })
}

#[track_caller]
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
