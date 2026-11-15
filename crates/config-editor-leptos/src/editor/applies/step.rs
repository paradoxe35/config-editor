//! From what applies to a value to what applies to one of its children:
//! each schema's own child schemas, an option's only where the value may
//! be in it.

use super::node::{Entry, Node, State, Tree, Walked, all, any};
use super::query::leaves;
use super::{item_schema, named_properties};
use crate::editor::language::SchemaPathSegment;
use serde_json::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// What applies to the child `segment` of a value `tree` (walked) applies
/// to; `entry` is what applied to the value before the walk.
pub(super) fn child<'s>(
    entry: &Entry<'s>,
    tree: &Tree<'s>,
    segment: &SchemaPathSegment,
) -> Entry<'s> {
    let found = child_only(tree, segment);
    // An item of something the schema has as one value (a block or an
    // element written twice): each copy is that value.
    if found.is_empty() && matches!(segment, SchemaPathSegment::ArrayItem(_)) {
        return entry.clone();
    }
    found
}

/// [`child`] of what the schemas name alone (no value read as its own
/// item).
pub(super) fn child_only<'s>(tree: &Tree<'s>, segment: &SchemaPathSegment) -> Entry<'s> {
    named(tree, segment, true)
}

/// [`child_only`]; `trust_fit`: an option the value only fits by a quick
/// look is taken as its own.
pub(super) fn named<'s>(
    tree: &Tree<'s>,
    segment: &SchemaPathSegment,
    trust_fit: bool,
) -> Entry<'s> {
    let mut step = Step {
        segment,
        trust_fit,
        nodes: HashMap::new(),
        leaves: HashMap::new(),
    };
    let found = step.of(tree);
    if !super::node::is_empty(&found) {
        return Entry(found);
    }
    // What no schema evaluates is the `unevaluatedProperties` (or
    // `unevaluatedItems`) schema's.
    let keyword = match segment {
        SchemaPathSegment::Key(_) => "unevaluatedProperties",
        SchemaPathSegment::ArrayItem(_) => "unevaluatedItems",
    };
    let unevaluated = all(leaves(tree, Walked::Any)
        .into_iter()
        .filter_map(|schema| schema.get(keyword))
        .filter(|unevaluated| unevaluated.is_object())
        .map(|schema| step.leaf(schema))
        .collect());
    Entry(unevaluated)
}

struct Step<'s, 'g> {
    segment: &'g SchemaPathSegment,
    /// Whether an option the value only fits is taken as its own.
    trust_fit: bool,
    nodes: HashMap<*const Node<'s>, Tree<'s>>,
    /// Each child schema's leaf, shared: a schema reached twice is one.
    leaves: HashMap<*const Value, Tree<'s>>,
}

impl<'s> Step<'s, '_> {
    fn leaf(&mut self, schema: &'s Value) -> Tree<'s> {
        self.leaves
            .entry(std::ptr::from_ref(schema))
            .or_insert_with(|| Rc::new(Node::Leaf(schema)))
            .clone()
    }

    fn of(&mut self, tree: &Tree<'s>) -> Tree<'s> {
        let key = Rc::as_ptr(tree);
        if let Some(done) = self.nodes.get(&key) {
            return done.clone();
        }
        let found = match &**tree {
            Node::Leaf(schema) => {
                let own = own(schema, self.segment);
                all(own.into_iter().map(|schema| self.leaf(schema)).collect())
            }
            Node::All(parts) => all(parts.iter().map(|part| self.of(part)).collect()),
            Node::Any(parts) | Node::Either(parts) => {
                any(parts.iter().map(|part| self.of(part)).collect())
            }
            Node::Options(choices) => {
                // A fitted option is a guess: reading a value's type goes
                // by every option it may be in.
                let trust_fit = self.trust_fit;
                let taken = move |state: State| {
                    state == State::Matched || (trust_fit && state == State::Fitted)
                };
                let matched = choices.iter().any(|choice| taken(choice.state));
                let kept: Vec<Tree<'s>> = choices
                    .iter()
                    .filter(|choice| match choice.state {
                        State::Refuted => false,
                        state if taken(state) => true,
                        _ => !matched,
                    })
                    .map(|choice| self.of(&choice.tree))
                    .collect();
                any(kept)
            }
            Node::Cut => all(Vec::new()),
        };
        self.nodes.insert(key, found.clone());
        found
    }
}

/// The child's schemas `schema` names itself: its property and the
/// patterns matching the key, else `additionalProperties`; an item's
/// tuple position or `items`. An index names an item of an array, and in
/// an object the key of its digits when the object names that key; a key
/// written as an index reads as one in an array.
pub(super) fn own<'s>(schema: &'s Value, segment: &SchemaPathSegment) -> Vec<&'s Value> {
    let named = named_properties(schema, &segment.as_key());
    let item = segment
        .as_index()
        .and_then(|index| item_schema(schema, index));
    match segment {
        SchemaPathSegment::ArrayItem(_) => item.map_or(named, |item| vec![item]),
        SchemaPathSegment::Key(_) if !named.is_empty() => named,
        SchemaPathSegment::Key(_) => item
            .or_else(|| {
                schema
                    .get("additionalProperties")
                    .filter(|additional| additional.is_object())
            })
            .into_iter()
            .collect(),
    }
}
