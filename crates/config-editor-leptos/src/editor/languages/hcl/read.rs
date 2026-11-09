//! A document read to its data, in time that grows with its length.
//!
//! `hcl-rs` reads a body to its JSON form through maps that look each key
//! up by comparing it with every key before it, and `hcl-edit` parses an
//! object the same way: a body or an object of thousands of keys (a
//! `.tfvars` file, a big `locals` map) took time growing with the square
//! of its keys. Here the body is mapped to data by its keys (as the HCL
//! JSON mapping says: blocks nest by type and labels, repeated blocks
//! are lists), each value read as `hcl-rs` reads it; and an object of many
//! entries is parsed in groups of entries (see [`super::big_objects`]), so
//! no parse compares a key with more than a group's.

use hcl_edit::expr::{Expression, Object, ObjectKey};
use hcl_edit::structure::Body;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

use super::big_objects::{self, Parsed};

/// Stands in, in the data read with `marks`, for each value written as an
/// expression (a reference, a call, a template with `${…}`): a string no
/// document holds.
pub(super) const MARK: &str = "\u{0}expression\u{0}";

/// Why a reading stopped: an error of the document, or a key two groups
/// of one object both write, which only a whole parse of the object tells
/// apart (an error, or two spellings of one key).
pub(super) enum Stop {
    Error(hcl::Error),
    Regroup,
}

impl From<hcl::Error> for Stop {
    fn from(error: hcl::Error) -> Self {
        Self::Error(error)
    }
}

/// `text` read to its data, each heredoc holding no template read as its
/// text and each whole float marked (see `super::floats`); with `marks`,
/// each value written as an expression is [`MARK`].
pub(super) fn read(text: &str, marks: bool) -> Result<Value, hcl::Error> {
    read_with(text, marks, true)
}

/// [`read`], with each object of many entries parsed in groups when
/// `grouping`, else whole.
pub(super) fn read_with(text: &str, marks: bool, grouping: bool) -> Result<Value, hcl::Error> {
    let parsed = big_objects::parse(text, grouping)?;
    match body_value(&parsed, &parsed.body, marks) {
        Ok(value) => Ok(value),
        Err(Stop::Error(error)) => Err(error),
        // A whole parse has no groups to send it back.
        Err(Stop::Regroup) => read_with(text, marks, false),
    }
}

/// A body's data under the HCL JSON mapping, as `hcl-rs` reads it.
enum Node<'a> {
    Map(Keyed<'a>),
    Bodies(Vec<&'a Body>),
    Value(&'a Expression),
}

fn body_value(parsed: &Parsed, body: &Body, marks: bool) -> Result<Value, Stop> {
    node_value(parsed, Node::Map(nodes(body)), marks)
}

/// Each name of `body` with what it holds: an attribute replaces a name
/// written before (keeping its place), blocks of one type merge by their
/// labels, and the bodies of blocks written alike become a list.
fn nodes(body: &Body) -> Keyed<'_> {
    let mut map = Keyed::default();
    for structure in body.iter() {
        if let Some(attribute) = structure.as_attribute() {
            map.insert(attribute.key.as_str().to_string(), Node::Value(&attribute.value));
        } else if let Some(block) = structure.as_block() {
            let mut node = Node::Bodies(vec![&block.body]);
            for label in block.labels.iter().rev() {
                let mut nested = Keyed::default();
                nested.insert(label.as_str().to_string(), node);
                node = Node::Map(nested);
            }
            map.merge(block.ident.as_str().to_string(), node);
        }
    }
    map
}

/// Nodes by name, in the order first written.
#[derive(Default)]
struct Keyed<'a> {
    at: HashMap<String, usize>,
    entries: Vec<(String, Node<'a>)>,
}

impl<'a> Keyed<'a> {
    fn insert(&mut self, key: String, node: Node<'a>) {
        match self.at.get(&key) {
            Some(at) => self.entries[*at].1 = node,
            None => {
                self.at.insert(key.clone(), self.entries.len());
                self.entries.push((key, node));
            }
        }
    }

    /// As `hcl-rs` merges a block into what its name holds: maps by key,
    /// lists of bodies one after the other, anything else replaced.
    fn merge(&mut self, key: String, node: Node<'a>) {
        match self.at.get(&key) {
            Some(at) => merge_into(&mut self.entries[*at].1, node),
            None => self.insert(key, node),
        }
    }
}

fn merge_into<'a>(into: &mut Node<'a>, node: Node<'a>) {
    match (into, node) {
        (Node::Map(map), Node::Map(other)) => {
            for (key, node) in other.entries {
                map.merge(key, node);
            }
        }
        (Node::Bodies(bodies), Node::Bodies(more)) => bodies.extend(more),
        (into, node) => *into = node,
    }
}

fn node_value(parsed: &Parsed, node: Node<'_>, marks: bool) -> Result<Value, Stop> {
    match node {
        Node::Map(map) => {
            let mut object = Map::with_capacity(map.entries.len());
            for (key, node) in map.entries {
                object.insert(key, node_value(parsed, node, marks)?);
            }
            Ok(Value::Object(object))
        }
        Node::Bodies(bodies) if bodies.len() == 1 => body_value(parsed, bodies[0], marks),
        Node::Bodies(bodies) => bodies
            .into_iter()
            .map(|body| body_value(parsed, body, marks))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Node::Value(expression) => expression_value(parsed, expression, marks),
    }
}

/// Whether `expression` is written as an expression rather than a value
/// (not a literal, a template of text, or a list or object of them).
fn is_expression(expression: &Expression) -> bool {
    let literal = |template: &hcl_edit::template::Template| {
        template.iter().all(|element| matches!(element, hcl_edit::template::Element::Literal(_)))
    };
    match expression {
        Expression::Null(_) | Expression::Bool(_) | Expression::Number(_) | Expression::String(_) => false,
        Expression::Array(_) | Expression::Object(_) => false,
        Expression::StringTemplate(template) => !literal(template),
        Expression::HeredocTemplate(heredoc) => !literal(&heredoc.template),
        _ => true,
    }
}

fn expression_value(parsed: &Parsed, expression: &Expression, marks: bool) -> Result<Value, Stop> {
    if marks && is_expression(expression) {
        return Ok(Value::String(MARK.to_string()));
    }
    match expression {
        Expression::Array(items) => items
            .iter()
            .map(|item| expression_value(parsed, item, marks))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Expression::Object(object) => match parsed.groups_of(object) {
            Some(groups) => grouped_object(groups, marks),
            None => {
                let mut map = Map::with_capacity(object.len());
                entries_into(parsed, object, marks, &mut map, false)?;
                Ok(Value::Object(map))
            }
        },
        other => Ok(Value::deserialize(hcl::Expression::from(other.clone()))?),
    }
}

/// An object parsed in groups: each group's entries in turn, a key written
/// in two groups sending the reading back to a whole parse.
fn grouped_object(groups: &[Parsed], marks: bool) -> Result<Value, Stop> {
    let mut map = Map::new();
    for group in groups {
        let object = group.object().ok_or(Stop::Regroup)?;
        entries_into(group, object, marks, &mut map, true)?;
    }
    Ok(Value::Object(map))
}

fn entries_into(parsed: &Parsed, object: &Object, marks: bool, map: &mut Map<String, Value>, grouped: bool) -> Result<(), Stop> {
    for (key, value) in object.iter() {
        let key = key_text(key)?;
        let value = expression_value(parsed, value.expr(), marks)?;
        if grouped && map.contains_key(&key) {
            return Err(Stop::Regroup);
        }
        map.insert(key, value);
    }
    Ok(())
}

/// An object key as `hcl-rs` reads it.
fn key_text(key: &ObjectKey) -> Result<String, hcl::Error> {
    match key {
        ObjectKey::Ident(ident) => Ok(ident.as_str().to_string()),
        ObjectKey::Expression(Expression::String(text)) => Ok(text.as_str().to_string()),
        ObjectKey::Expression(expression) => {
            let mut object = hcl::Object::new();
            object.insert(hcl::ObjectKey::Expression(hcl::Expression::from(expression.clone())), hcl::Expression::Null);
            let read = Value::deserialize(hcl::Expression::Object(object))?;
            Ok(read.as_object().and_then(|map| map.keys().next().cloned()).unwrap_or_default())
        }
    }
}
