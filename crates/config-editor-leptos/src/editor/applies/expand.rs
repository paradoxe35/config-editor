//! The one walk of applicators: each schema of an [`Entry`] with every
//! schema it applies to the value (its `$ref` target, `allOf`, its
//! `oneOf`/`anyOf` options narrowed by the value, the `then`/`else` the
//! value's `if` takes, the dependent schemas of the keys it has).

use super::narrow::states;
use super::node::{Choice, Entry, Node, Tree, all, any, either};
use super::{Seen, ref_target};
use crate::editor::work_count::{Work, note};
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// Nesting of applicators walked for one value, past which a schema says
/// nothing more (a reference cycle ends sooner, at its ancestor).
const MAX_NESTING: usize = 1024;

/// One walk: the schemas met (each walked once, shared where met again).
pub(super) struct Expand<'s, 'i> {
    pub(super) root: &'s Value,
    pub(super) ref_alone: bool,
    pub(super) seen: Seen<'i>,
    /// Each schema walked: `None` while it is being walked (met again
    /// then, it is its own ancestor: a cycle).
    schemas: HashMap<*const Value, Option<Tree<'s>>>,
    /// Each entry node walked.
    nodes: HashMap<*const Node<'s>, Tree<'s>>,
    nesting: usize,
    /// Whether the value decided anything here (an `if`, a dependent
    /// schema, options): only then does the walk depend on it.
    pub(super) decidable: bool,
}

impl<'s, 'i> Expand<'s, 'i> {
    pub(super) fn new(root: &'s Value, ref_alone: bool, seen: Seen<'i>) -> Self {
        Self {
            root,
            ref_alone,
            seen,
            schemas: HashMap::new(),
            nodes: HashMap::new(),
            nesting: 0,
            decidable: false,
        }
    }

    /// `entry` with its schemas' applicators walked.
    pub(super) fn entry(&mut self, entry: &Entry<'s>) -> Tree<'s> {
        self.node(&entry.0)
    }

    fn node(&mut self, node: &Tree<'s>) -> Tree<'s> {
        let key = Rc::as_ptr(node);
        if let Some(done) = self.nodes.get(&key) {
            return done.clone();
        }
        let walked = match &**node {
            Node::Leaf(schema) => {
                // One walk for each schema at the value (what it applies
                // is part of its walk).
                note(Work::BranchWalk);
                self.schema(schema)
            }
            Node::All(parts) => all(parts.iter().map(|part| self.node(part)).collect()),
            Node::Any(parts) => any(parts.iter().map(|part| self.node(part)).collect()),
            // What an `if` not decided says of a child.
            Node::Either(parts) => either(parts.iter().map(|part| self.node(part)).collect()),
            // Entries hold no walked options.
            Node::Options(_) | Node::Cut => node.clone(),
        };
        self.nodes.insert(key, walked.clone());
        walked
    }

    /// `schema` and every schema its applicators apply to the value.
    pub(super) fn schema(&mut self, schema: &'s Value) -> Tree<'s> {
        let key = std::ptr::from_ref(schema);
        match self.schemas.get(&key) {
            Some(Some(done)) => return done.clone(),
            Some(None) => return Rc::new(Node::Cut),
            None => {}
        }
        let leaf = Rc::new(Node::Leaf(schema));
        if !schema.is_object() {
            return leaf;
        }
        if self.nesting >= MAX_NESTING {
            return Rc::new(Node::Cut);
        }
        self.schemas.insert(key, None);
        self.nesting += 1;
        let mut parts = Vec::new();
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| ref_target(reference, self.root))
        {
            parts.push(self.schema(target));
        }
        if !(self.ref_alone && schema.get("$ref").is_some()) {
            parts.push(leaf);
            for keyword in ["anyOf", "oneOf"] {
                if let Some(options) = self.options(schema, keyword) {
                    parts.push(options);
                }
            }
            for part in array(schema, "allOf") {
                parts.push(self.schema(part));
            }
            if let Some(conditional) = self.conditional(schema) {
                parts.push(conditional);
            }
            for dependent in self.dependents(schema) {
                parts.push(self.schema(dependent));
            }
        }
        self.nesting -= 1;
        let walked = all(parts);
        self.schemas.insert(key, Some(walked.clone()));
        walked
    }

    /// The options of `schema`'s `keyword`, narrowed by the value.
    fn options(&mut self, schema: &'s Value, keyword: &str) -> Option<Tree<'s>> {
        let options: Vec<Tree<'s>> = array(schema, keyword)
            .map(|option| self.schema(option))
            .collect();
        if options.is_empty() {
            return None;
        }
        self.decidable = true;
        let states = states(&options, self.seen, self.root);
        Some(Rc::new(Node::Options(
            options
                .into_iter()
                .zip(states)
                .map(|(tree, state)| Choice { tree, state })
                .collect(),
        )))
    }

    /// The `then`/`else` of `schema`'s `if` the value takes, or either
    /// while it does not decide.
    fn conditional(&mut self, schema: &'s Value) -> Option<Tree<'s>> {
        let condition = schema.get("if")?;
        self.decidable = true;
        // An object known by its keys alone decides an `if` asking keys.
        let decided = match (self.seen.value, self.seen.keys) {
            (Some(value), _) => super::decides(condition, value, self.root),
            (None, Some(keys)) => super::accepts::decides_keys(condition, keys),
            (None, None) => None,
        };
        let branch = |this: &mut Self, keyword: &str| {
            schema
                .get(keyword)
                .map_or_else(|| all(Vec::new()), |branch| this.schema(branch))
        };
        Some(match decided {
            Some(true) => branch(self, "then"),
            Some(false) => branch(self, "else"),
            None => {
                let then = branch(self, "then");
                let otherwise = branch(self, "else");
                // Both kept apart, so the branches stay in the list of
                // every schema even where one says nothing.
                Rc::new(Node::Either(
                    [then, otherwise]
                        .into_iter()
                        .filter(|part| !super::node::is_empty(part))
                        .collect(),
                ))
            }
        })
    }

    /// The dependent schemas of `schema` whose key the object has (all
    /// of them while that is unknown).
    fn dependents(&mut self, schema: &'s Value) -> Vec<&'s Value> {
        let found: Vec<&'s Value> = ["dependentSchemas", "dependencies"]
            .into_iter()
            .filter_map(|keyword| schema.get(keyword).and_then(Value::as_object))
            .flatten()
            .filter(|(_, dependent)| dependent.is_object() || dependent.is_boolean())
            .filter(|(key, _)| self.seen.has_key(key))
            .map(|(_, dependent)| dependent)
            .collect();
        if schema.get("dependentSchemas").is_some() || schema.get("dependencies").is_some() {
            self.decidable = true;
        }
        found
    }
}

pub(super) fn array<'a>(schema: &'a Value, keyword: &str) -> impl Iterator<Item = &'a Value> {
    schema
        .get(keyword)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}
