//! `xs:key`, `xs:keyref` and `xs:unique` on the data model: the XPath
//! subset XSD allows for them (`a/b`, `.//b`, `*`, `.`, `|`, a field's
//! final `@attr`), evaluated on an element's value, where attributes and
//! child elements are both keys.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::super::super::CheckProblem;
use crate::editor::languages::XML_TEXT_KEY;

/// One constraint as the compiler writes it (`x-xsd-identity`).
#[derive(Clone, Debug, PartialEq)]
pub(in super::super) struct Constraint {
    pub(in super::super) kind: String,
    pub(in super::super) name: String,
    pub(in super::super) selector: String,
    pub(in super::super) fields: Vec<String>,
    pub(in super::super) refer: Option<String>,
}

impl Constraint {
    pub(in super::super) fn to_json(&self) -> Value {
        json!({"kind": self.kind, "name": self.name, "selector": self.selector, "fields": self.fields, "refer": self.refer})
    }

    fn from_json(value: &Value) -> Option<Constraint> {
        Some(Constraint {
            kind: value.get("kind")?.as_str()?.to_string(),
            name: value.get("name")?.as_str()?.to_string(),
            selector: value.get("selector")?.as_str()?.to_string(),
            fields: value
                .get("fields")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            refer: value
                .get("refer")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// A step of a path: a child (by local name), any child, or itself; the
/// first step may search all descendants (`.//`).
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Child(String),
    AnyChild,
    Attribute(String),
}

/// A path: whether it searches descendants, and its steps.
#[derive(Clone, Debug, PartialEq)]
struct Path {
    descendants: bool,
    steps: Vec<Step>,
}

/// `text` (a selector or field) as paths, or `None` beyond the subset.
fn parse(text: &str, field: bool) -> Option<Vec<Path>> {
    text.split('|')
        .map(|alternative| {
            let mut rest = alternative.trim();
            let mut descendants = false;
            if let Some(after) = rest.strip_prefix(".//") {
                descendants = true;
                rest = after;
            }
            let mut steps = Vec::new();
            for (index, step) in rest.split('/').enumerate() {
                let step = step.trim();
                let last = index == rest.split('/').count() - 1;
                match step {
                    "." if steps.is_empty() => {}
                    "*" => steps.push(Step::AnyChild),
                    "" => return None,
                    _ if step.starts_with('@') && field && last => {
                        let name = step[1..].rsplit(':').next()?.to_string();
                        steps.push(Step::Attribute(name));
                    }
                    _ if step.chars().all(|character| {
                        character.is_alphanumeric() || "_-.:".contains(character)
                    }) =>
                    {
                        let name = step.rsplit(':').next()?.to_string();
                        steps.push(Step::Child(name));
                    }
                    _ => return None,
                }
            }
            Some(Path { descendants, steps })
        })
        .collect()
}

/// Whether the compiler can write `constraint` as a check (its paths are
/// in the subset).
pub(in super::super) fn supported(selector: &str, fields: &[String]) -> bool {
    parse(selector, false).is_some() && fields.iter().all(|field| parse(field, true).is_some())
}

/// The values `path` reaches from `value` (at `pointer`).
fn select<'v>(value: &'v Value, pointer: &str, path: &Path) -> Vec<(&'v Value, String)> {
    let mut found: Vec<(&'v Value, String)> = vec![(value, pointer.to_string())];
    if path.descendants {
        let mut all = Vec::new();
        for (value, pointer) in found {
            descendants(value, pointer, &mut all);
        }
        found = all;
    }
    for step in &path.steps {
        let mut next = Vec::new();
        for (value, pointer) in &found {
            let Value::Object(map) = value else {
                continue;
            };
            let mut take = |key: &str, child: &'v Value| {
                let child_pointer =
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                match child {
                    Value::Array(items) => {
                        for (index, item) in items.iter().enumerate() {
                            next.push((item, format!("{child_pointer}/{index}")));
                        }
                    }
                    other => next.push((other, child_pointer)),
                }
            };
            match step {
                Step::Child(name) => {
                    if let Some(child) = map.get(name) {
                        take(name, child);
                    }
                }
                Step::Attribute(name) => {
                    let key = if map.contains_key(&format!("@{name}")) {
                        format!("@{name}")
                    } else {
                        name.clone()
                    };
                    if let Some(child) = map.get(&key) {
                        next.push((child, format!("{pointer}/{key}")));
                    }
                }
                Step::AnyChild => {
                    for (key, child) in map {
                        if !key.starts_with('#') && !key.starts_with('@') {
                            take(key, child);
                        }
                    }
                }
            }
        }
        found = next;
    }
    found
}

/// `value` (at `pointer`) and every value under it.
fn descendants<'v>(value: &'v Value, pointer: String, into: &mut Vec<(&'v Value, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                descendants(
                    child,
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                    into,
                );
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                descendants(item, format!("{pointer}/{index}"), into);
            }
            return;
        }
        _ => {}
    }
    if !value.is_array() {
        into.push((value, pointer));
    }
}

/// A field's value as text (an element with attributes: its text).
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(_) | Value::Bool(_) => Some(value.to_string()),
        Value::Object(map) => map.get(XML_TEXT_KEY).and_then(text),
        _ => None,
    }
}

/// The problems of every constraint of `scopes`: each element instance
/// (its pointer and value) with the constraints declared on it.
pub(super) fn check(scopes: &[(String, &Value, &Vec<Value>)]) -> Vec<CheckProblem> {
    let mut problems = Vec::new();
    // The values of each key and unique, by name, for keyrefs.
    let mut keys: HashMap<String, HashSet<Vec<String>>> = HashMap::new();
    let mut refs: Vec<(Constraint, Vec<String>, String)> = Vec::new();
    for (pointer, value, constraints) in scopes {
        for constraint in constraints.iter().filter_map(Constraint::from_json) {
            let Some(selector) = parse(&constraint.selector, false) else {
                continue;
            };
            let fields: Option<Vec<Vec<Path>>> = constraint
                .fields
                .iter()
                .map(|field| parse(field, true))
                .collect();
            let Some(fields) = fields else {
                continue;
            };
            let mut seen: HashMap<Vec<String>, String> = HashMap::new();
            for path in &selector {
                for (node, node_pointer) in select(value, pointer, path) {
                    let tuple: Vec<Option<String>> = fields
                        .iter()
                        .map(|alternatives| {
                            alternatives
                                .iter()
                                .flat_map(|field| select(node, &node_pointer, field))
                                .next()
                                .and_then(|(found, _)| text(found))
                        })
                        .collect();
                    if tuple.iter().any(Option::is_none) {
                        if constraint.kind == "key" {
                            problems.push(CheckProblem::new(
                                node_pointer.clone(),
                                format!(
                                    "the xs:key \"{}\" needs a value for {}",
                                    constraint.name,
                                    constraint.fields.join(", ")
                                ),
                            ));
                        }
                        continue;
                    }
                    let tuple: Vec<String> = tuple.into_iter().flatten().collect();
                    if constraint.kind == "keyref" {
                        refs.push((constraint.clone(), tuple, node_pointer));
                        continue;
                    }
                    if let Some(first) = seen.get(&tuple) {
                        problems.push(CheckProblem::new(
                            node_pointer.clone(),
                            format!(
                                "\"{}\" is already the value of the xs:{} \"{}\" at {first}",
                                tuple.join(", "),
                                constraint.kind,
                                constraint.name
                            ),
                        ));
                    } else {
                        seen.insert(tuple.clone(), node_pointer.clone());
                    }
                    keys.entry(constraint.name.clone())
                        .or_default()
                        .insert(tuple);
                }
            }
        }
    }
    for (constraint, tuple, pointer) in refs {
        let refer = constraint.refer.clone().unwrap_or_default();
        if !keys.get(&refer).is_some_and(|known| known.contains(&tuple)) {
            problems.push(CheckProblem::new(
                pointer,
                format!(
                    "\"{}\" is no value of the xs:key \"{refer}\" (xs:keyref \"{}\")",
                    tuple.join(", "),
                    constraint.name
                ),
            ));
        }
    }
    problems
}
