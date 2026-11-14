//! The XSD rules JSON Schema cannot hold, as checks the editor runs beside
//! the compiled schema (see `SchemaCheck`): each element's children
//! against its content model (`automaton`: order, choices, substitution
//! groups, occurrences, wildcards' namespaces) and its attributes against
//! an attribute wildcard's namespaces, and `xs:key`, `xs:keyref` and
//! `xs:unique` (`identity`). The compiler writes what they need into the
//! JSON Schema (`x-xsd-content`, `x-xsd-identity`); they walk the
//! document with it. In an XML document the children's order and
//! namespaces are read from the text (repeated elements interleaved as
//! written); in another language, the order is the value's keys' (each
//! list's items together, as they would be written back as XML), and no
//! namespace is known.

pub(super) mod automaton;
pub(super) mod identity;
mod text_order;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

use serde_json::{Map, Value};

use super::super::super::completion::{child_schemas, resolve_branches};
use super::super::super::language::SchemaPathSegment;
use super::super::{CheckProblem, CheckedDocument, SchemaCheck};
use automaton::{Automaton, Child, Mismatch, Namespace, Namespaces, Occurs};
use text_order::Element;

/// The keyword holding an element's content model.
pub(super) const CONTENT: &str = "x-xsd-content";
/// The keyword holding an element's identity constraints.
pub(super) const IDENTITY: &str = "x-xsd-identity";

/// Each element's children against its content model.
pub(super) struct ContentCheck;

/// `xs:key`, `xs:keyref`, `xs:unique`.
pub(super) struct IdentityCheck;

/// What `x-xsd-content` holds, read.
struct Content<'s> {
    model: Occurs,
    names: BTreeSet<String>,
    wildcard: bool,
    /// The declared attributes, when an attribute wildcard leaves the JSON
    /// Schema open (else it checks them).
    attributes: Option<Vec<&'s str>>,
    any_attribute: Option<Namespaces>,
    /// The namespaces its `xs:any`s allow.
    wildcards: Vec<Namespaces>,
    /// The element names in the order the model first names them.
    order: Vec<String>,
    /// The target namespace: an element written without a prefix in
    /// another language's value is in it.
    namespace: Option<String>,
    /// The schema compiled with `lax`: other children are no problem.
    lax: bool,
}

impl<'s> Content<'s> {
    fn read(value: &'s Value) -> Option<Content<'s>> {
        let model = Occurs::from_json(value.get("model")?)?;
        let mut names = BTreeSet::new();
        model.names(&mut names);
        let wildcard = model.has_wildcard();
        let attributes = value
            .get("attributes")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect());
        let any_attribute = value.get("anyAttribute").and_then(Namespaces::from_json);
        let lax = value.get("lax") == Some(&Value::Bool(true));
        let mut wildcards = Vec::new();
        model.wildcards(&mut wildcards);
        let mut order = Vec::new();
        model.ordered_names(&mut order);
        let namespace = value
            .get("namespace")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(Content {
            model,
            names,
            wildcard,
            attributes,
            any_attribute,
            wildcards,
            order,
            namespace,
            lax,
        })
    }
}

impl SchemaCheck for ContentCheck {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("XSD content models (element order, choices, occurrences, wildcards)")
    }

    fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem> {
        let text = text_order::read(document);
        let mut problems = Vec::new();
        let root = document.schema;
        // Each model's automaton, built once per check.
        let mut automata: HashMap<*const Value, Option<(Content<'_>, Automaton)>> = HashMap::new();
        let top = text.as_deref().map(Node::Elements);
        walk(
            document.value,
            &[root],
            root,
            top,
            String::new(),
            &mut |value, branches, node, pointer| {
                let Some(annotation) = branches.iter().find_map(|branch| branch.get(CONTENT))
                else {
                    return;
                };
                let Some((content, automaton)) = automata
                    .entry(std::ptr::from_ref(annotation))
                    .or_insert_with(|| {
                        let content = Content::read(annotation)?;
                        let automaton = Automaton::new(&content.model)?;
                        Some((content, automaton))
                    })
                    .as_ref()
                else {
                    return;
                };
                let empty = Map::new();
                let map = match value {
                    Value::Object(map) => map,
                    _ => &empty,
                };
                let element = match node {
                    Some(Node::Element(element)) => Some(element),
                    _ => None,
                };
                let mut refused = Vec::new();
                match (element, &content.attributes) {
                    (Some(element), Some(declared)) => {
                        attribute_problems(element, map, declared, content, pointer, &mut problems);
                    }
                    (None, Some(declared)) if !content.lax => {
                        refused = unknown_keys(map, declared, content);
                        for key in &refused {
                            let at = format!("{pointer}/{}", escape(key));
                            problems.push(CheckProblem::new(at, not_allowed(key)).at_key());
                        }
                    }
                    _ => {}
                }
                let names: Vec<(String, Namespace)> = match element {
                    Some(element) => element
                        .children
                        .iter()
                        .map(|child| {
                            (
                                child.name.clone(),
                                Namespace::Known(child.namespace.clone()),
                            )
                        })
                        .collect(),
                    // Another language's keys: in the model's order (the
                    // order is XML's; TOML and HCL write tables last).
                    None => {
                        let mut names: Vec<String> = children_of(map, branches)
                            .into_iter()
                            .filter(|name| !refused.contains(name))
                            .collect();
                        names.sort_by_key(|name| {
                            content
                                .order
                                .iter()
                                .position(|known| known == name)
                                .unwrap_or(usize::MAX)
                        });
                        names
                            .into_iter()
                            .map(|name| (name, Namespace::Unknown))
                            .collect()
                    }
                };
                if json_schema_reports(map, branches, content) {
                    // Its own problems come first: the order once they are
                    // fixed.
                    return;
                }
                let kept: Vec<usize> = (0..names.len())
                    .filter(|index| {
                        !content.lax || content.wildcard || content.names.contains(&names[*index].0)
                    })
                    .collect();
                let run = |kept: &[usize]| {
                    let children: Vec<Child<'_>> = kept
                        .iter()
                        .map(|index| Child {
                            name: &names[*index].0,
                            namespace: names[*index].1.clone(),
                        })
                        .collect();
                    automaton.run(&children)
                };
                let mut verdict = run(&kept);
                // Another language's key that is an attribute or an element
                // alike (a scalar named as both): either reading will do.
                let ambiguous: Vec<&str> = match element {
                    Some(_) => Vec::new(),
                    None => ambiguous_keys(map, branches),
                };
                if verdict.is_err() && !ambiguous.is_empty() {
                    let count = ambiguous.len().min(4);
                    let taken = (1..1_usize << count).any(|mask| {
                        let attributes: Vec<&str> = (0..count)
                            .filter(|bit| mask & (1 << bit) != 0)
                            .map(|bit| ambiguous[bit])
                            .collect();
                        let without: Vec<usize> = kept
                            .iter()
                            .copied()
                            .filter(|index| !attributes.contains(&names[*index].0.as_str()))
                            .collect();
                        run(&without).is_ok()
                    });
                    if taken {
                        verdict = Ok(());
                    }
                }
                match verdict {
                    Ok(()) => {}
                    Err(Mismatch::Unexpected { index, expected }) => {
                        let index = kept[index];
                        let name = &names[index].0;
                        let occurrence = names[..index]
                            .iter()
                            .filter(|(known, _)| known == name)
                            .count();
                        let child = match map.get(name) {
                            Some(Value::Array(_)) => {
                                format!("{pointer}/{}/{occurrence}", escape(name))
                            }
                            Some(_) => format!("{pointer}/{}", escape(name)),
                            None => pointer.to_string(),
                        };
                        problems
                            .push(CheckProblem::new(child, unexpected(name, &expected)).at_key());
                    }
                    Err(Mismatch::Missing { expected }) => {
                        problems.push(
                            CheckProblem::new(pointer.to_string(), missing(&expected)).at_key(),
                        );
                    }
                }
            },
        );
        problems
    }
}

impl SchemaCheck for IdentityCheck {
    fn name(&self) -> Cow<'static, str> {
        Cow::Borrowed("XSD identity constraints (xs:key, xs:keyref, xs:unique)")
    }

    fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem> {
        let root = document.schema;
        let mut scopes = Vec::new();
        walk(
            document.value,
            &[root],
            root,
            None,
            String::new(),
            &mut |value, branches, _, pointer| {
                for branch in branches {
                    if let Some(Value::Array(constraints)) = branch.get(IDENTITY) {
                        scopes.push((pointer.to_string(), value, constraints));
                    }
                }
            },
        );
        identity::check(&scopes)
    }
}

/// The keys of `map` (another language's value) that are neither declared
/// (as elements or attributes) nor allowed by a wildcard: an element
/// written without a prefix is in the target namespace, an attribute in
/// none.
fn unknown_keys(map: &Map<String, Value>, declared: &[&str], content: &Content<'_>) -> Vec<String> {
    let element = Namespace::Known(content.namespace.clone());
    let as_element = content
        .wildcards
        .iter()
        .any(|namespaces| namespaces.allows(&element));
    let as_attribute = content
        .any_attribute
        .as_ref()
        .is_some_and(|namespaces| namespaces.allows(&Namespace::Known(None)));
    if as_element || as_attribute {
        return Vec::new();
    }
    map.keys()
        .filter(|key| {
            !key.starts_with(['@', '#'])
                && !key.contains(':')
                && !content.names.contains(*key)
                && !declared.contains(&key.as_str())
        })
        .cloned()
        .collect()
}

/// A key (an attribute in XML) neither declared nor allowed by a wildcard:
/// the same words in every language.
fn not_allowed(name: &str) -> String {
    format!("\"{name}\" is neither declared here nor allowed by a wildcard")
}

/// The attributes of `element` neither declared nor allowed by the
/// attribute wildcard's namespaces.
fn attribute_problems(
    element: &Element,
    map: &Map<String, Value>,
    declared: &[&str],
    content: &Content<'_>,
    pointer: &str,
    problems: &mut Vec<CheckProblem>,
) {
    for (name, namespace) in &element.attributes {
        let known = match namespace.as_deref() {
            None => declared.contains(&name.as_str()),
            Some("http://www.w3.org/XML/1998/namespace") => {
                declared.contains(&format!("xml:{name}").as_str())
            }
            Some(_) => false,
        };
        let allowed = content
            .any_attribute
            .as_ref()
            .is_some_and(|namespaces| namespaces.allows(&Namespace::Known(namespace.clone())));
        if known || allowed || content.lax {
            continue;
        }
        let key = [format!("@{name}"), name.clone()]
            .into_iter()
            .find(|key| map.contains_key(key))
            .or_else(|| {
                map.keys()
                    .find(|key| key.ends_with(&format!(":{name}")))
                    .cloned()
            });
        let at = key.map_or_else(
            || pointer.to_string(),
            |key| format!("{pointer}/{}", escape(&key)),
        );
        let message = not_allowed(name);
        problems.push(CheckProblem::new(at, message).at_key());
    }
}

/// Whether the JSON Schema already finds a problem with the children of
/// `map`: a required one missing, a list of the wrong length, or one the
/// model does not name (in a closed model).
fn json_schema_reports(
    map: &Map<String, Value>,
    branches: &[&Value],
    content: &Content<'_>,
) -> bool {
    for branch in branches {
        let required = branch.get("required").and_then(Value::as_array);
        if required.is_some_and(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .any(|key| !map.contains_key(key))
        }) {
            return true;
        }
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        for (key, value) in map {
            let Some(property) = properties.get(key) else {
                continue;
            };
            if let Value::Array(items) = value {
                let count = items.len() as u64;
                let below = property
                    .get("minItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|min| count < min);
                let above = property
                    .get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|max| count > max);
                if below || above {
                    return true;
                }
            }
        }
    }
    !content.wildcard
        && !content.lax
        && children_of(map, branches)
            .iter()
            .any(|name| !content.names.contains(name))
}

/// The text of an XML document as elements, or where in it the walk is.
#[derive(Clone, Copy)]
enum Node<'a> {
    Elements(&'a [Element]),
    Element(&'a Element),
}

/// The element children of `map` (an element's value) in the order of its
/// keys, each as often as written (a list: once per item); attributes
/// (`xml.attribute`, `@name`, prefixed names) and text are no children.
fn children_of(map: &Map<String, Value>, branches: &[&Value]) -> Vec<String> {
    let property = |key: &str| {
        branches.iter().find_map(|branch| {
            branch
                .get("properties")
                .and_then(|properties| properties.get(key))
        })
    };
    let attribute = |key: &str| {
        key.starts_with(['@', '#'])
            || key.contains(':')
            || property(key).and_then(|property| property.pointer("/xml/attribute"))
                == Some(&Value::Bool(true))
    };
    let mut names = Vec::new();
    for (key, child) in map {
        if attribute(key) {
            continue;
        }
        let count = match child {
            Value::Array(items) => items.len(),
            _ => 1,
        };
        names.extend(std::iter::repeat_n(key.clone(), count));
    }
    names
}

/// The keys of `map` (another language's value) that may be an attribute
/// or a child element: a scalar where the schema has both (`@name` beside
/// `name`), the other spelling absent. XML reads an attribute named as a
/// child element as `name` when that element is not there.
fn ambiguous_keys<'m>(map: &'m Map<String, Value>, branches: &[&Value]) -> Vec<&'m str> {
    map.iter()
        .filter(|(key, child)| {
            !child.is_object()
                && !child.is_array()
                && !map.contains_key(&format!("@{key}"))
                && branches.iter().any(|branch| {
                    branch
                        .get("properties")
                        .is_some_and(|properties| properties.get(format!("@{key}")).is_some())
                })
        })
        .map(|(key, _)| key.as_str())
        .collect()
}

/// Walks `value` (at `pointer`) with its `schemas`, calling `visit` on
/// each value with its branches and, in an XML document, its element.
fn walk<'v, 's>(
    value: &'v Value,
    schemas: &[&'s Value],
    root: &'s Value,
    node: Option<Node<'_>>,
    pointer: String,
    visit: &mut impl FnMut(&'v Value, &[&'s Value], Option<Node<'_>>, &str),
) {
    if pointer.matches('/').count() > 256 {
        return;
    }
    let branches: Vec<&'s Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .collect();
    visit(value, &branches, node, &pointer);
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let segment = SchemaPathSegment::Key(key.clone());
                let child_schemas: Vec<&'s Value> = branches
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                let elements: Vec<&Element> = match node {
                    Some(Node::Elements(elements)) => elements
                        .iter()
                        .filter(|element| element.name == *key)
                        .collect(),
                    Some(Node::Element(element)) => element
                        .children
                        .iter()
                        .filter(|child| child.name == *key)
                        .collect(),
                    None => Vec::new(),
                };
                let child_pointer = format!("{pointer}/{}", escape(key));
                match child {
                    Value::Array(items) => {
                        let item_schemas: Vec<&'s Value> = child_schemas
                            .iter()
                            .flat_map(|schema| child_schemas_of_item(schema, root))
                            .collect();
                        for (index, item) in items.iter().enumerate() {
                            let element = elements.get(index).map(|element| Node::Element(element));
                            walk(
                                item,
                                &item_schemas,
                                root,
                                element,
                                format!("{child_pointer}/{index}"),
                                visit,
                            );
                        }
                    }
                    _ => {
                        let element = elements.first().map(|element| Node::Element(element));
                        walk(child, &child_schemas, root, element, child_pointer, visit);
                    }
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let segment = SchemaPathSegment::ArrayItem(index);
                let item_schemas: Vec<&'s Value> = branches
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                walk(
                    item,
                    &item_schemas,
                    root,
                    None,
                    format!("{pointer}/{index}"),
                    visit,
                );
            }
        }
        _ => {}
    }
}

/// The schemas of an item of a list `schema` describes; `schema` itself
/// when it describes no list (a lone element repeated in another
/// language's value).
fn child_schemas_of_item<'s>(schema: &'s Value, root: &'s Value) -> Vec<&'s Value> {
    let items = child_schemas(schema, root, &SchemaPathSegment::ArrayItem(0));
    if items.is_empty() {
        vec![schema]
    } else {
        items
    }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn unexpected(name: &str, expected: &[String]) -> String {
    if expected.is_empty() {
        format!("<{name}> is not expected here: no more elements are allowed")
    } else {
        format!(
            "<{name}> is not expected here; expected {}",
            listed(expected)
        )
    }
}

fn missing(expected: &[String]) -> String {
    format!(
        "an element is missing at the end: expected {}",
        listed(expected)
    )
}

fn listed(names: &[String]) -> String {
    match names {
        [one] => one.clone(),
        [rest @ .., last] => format!("one of {} or {last}", rest.join(", ")),
        [] => String::new(),
    }
}
