//! The shape of what applies to one value: a tree (a DAG: subtrees met
//! twice are shared) of schemas each applying, one of which applies, and
//! so on.

use serde_json::Value;
use std::rc::Rc;

/// A shared subtree.
pub(crate) type Tree<'s> = Rc<Node<'s>>;

#[derive(Debug)]
pub(crate) enum Node<'s> {
    /// One schema. Before its applicators are walked (in an [`Entry`]) it
    /// stands for itself and all it applies; after (in a [`Here`]) for its
    /// own keywords (`properties`, `items`, `type`, …).
    Leaf(&'s Value),
    /// Each applies: a schema beside its `$ref` target, its `allOf`, the
    /// `then`/`else` its `if` takes, its dependent schemas.
    All(Vec<Tree<'s>>),
    /// One of these readings is the value's: the options a step under a
    /// `oneOf` left open, an array and its item. What each says is read
    /// as said of the value (as a flat list of the schemas there would).
    Any(Vec<Tree<'s>>),
    /// The `then` and `else` of an `if` the value does not decide (or
    /// what they say of a child): one part alone stands beside a branch
    /// saying nothing, which takes any value.
    Either(Vec<Tree<'s>>),
    /// A `oneOf`/`anyOf`, each option with what the value tells of it.
    Options(Vec<Choice<'s>>),
    /// A reference cycle or the nesting limit: says nothing more, and an
    /// evaluation there cannot tell.
    Cut,
}

/// One option of a `oneOf`/`anyOf`.
#[derive(Debug)]
pub(crate) struct Choice<'s> {
    /// It, its applicators walked.
    pub(crate) tree: Tree<'s>,
    pub(crate) state: State,
}

/// What the value tells of one option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    /// It may be the value's.
    Open,
    /// It is the value's: the only one it can be in, or the one its keys
    /// or `const` values pick.
    Matched,
    /// It is the one the value fits by a quick look at its values (not
    /// a validation): what suggestions and hover go by, never what reading
    /// a value's type goes by.
    Fitted,
    /// The value is not in it (a kind or a listed value it refuses).
    Refuted,
}

impl State {
    /// Whether the value is taken to be in this option (matched or fitted).
    pub(crate) fn taken(self) -> bool {
        matches!(self, State::Matched | State::Fitted)
    }
}

/// What applies to a value before the applicators of its schemas are
/// walked: `Leaf`, `All`, `Any` and `Either` only.
#[derive(Clone, Debug)]
pub(crate) struct Entry<'s>(pub(crate) Tree<'s>);

impl<'s> Entry<'s> {
    /// One schema.
    pub(crate) fn schema(schema: &'s Value) -> Self {
        Self(Rc::new(Node::Leaf(schema)))
    }

    /// Each of `schemas` a reading of the value.
    pub(crate) fn any_of(schemas: &[&'s Value]) -> Self {
        Self(any(schemas
            .iter()
            .map(|schema| Rc::new(Node::Leaf(schema)))
            .collect()))
    }

    /// Whether nothing applies (a key no schema names).
    pub(crate) fn is_empty(&self) -> bool {
        is_empty(&self.0)
    }

    /// Every schema named, once, in order.
    pub(crate) fn schemas(&self) -> Vec<&'s Value> {
        super::query::leaves(&self.0, Walked::Any)
    }
}

/// Which leaves a walk of a tree takes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Walked {
    /// Every one.
    Any,
    /// Those certain to apply: not a branch of an `if` not decided, nor
    /// in an option not matched.
    Certain,
    /// Those a value may still be read by: none of a refuted option, and
    /// only the matched option of a `oneOf` that has one.
    Offered,
}

/// Whether `tree` names no schema.
pub(crate) fn is_empty(tree: &Tree<'_>) -> bool {
    match &**tree {
        Node::All(parts) | Node::Any(parts) | Node::Either(parts) => parts.is_empty(),
        Node::Options(choices) => choices.is_empty(),
        Node::Leaf(_) => false,
        Node::Cut => true,
    }
}

/// `parts` all applying: those naming nothing left out, each shared part
/// once, one part alone as itself.
pub(crate) fn all<'s>(parts: Vec<Tree<'s>>) -> Tree<'s> {
    joined(parts, Node::All)
}

/// One of `parts` applying, read as those that say something: an option
/// silent on a key leaves the value to the options typing it, as
/// validation of the object in one of those would.
pub(crate) fn any<'s>(parts: Vec<Tree<'s>>) -> Tree<'s> {
    joined(parts, Node::Any)
}

/// The branches of an `if` not decided: those naming nothing left out,
/// the node kept for one alone (the other takes any value).
pub(crate) fn either<'s>(parts: Vec<Tree<'s>>) -> Tree<'s> {
    let kept: Vec<Tree<'s>> = parts.into_iter().filter(|part| !is_empty(part)).collect();
    Rc::new(Node::Either(kept))
}

fn joined<'s>(parts: Vec<Tree<'s>>, node: fn(Vec<Tree<'s>>) -> Node<'s>) -> Tree<'s> {
    let mut kept: Vec<Tree<'s>> = Vec::with_capacity(parts.len());
    for part in parts {
        if !is_empty(&part) && !kept.iter().any(|known| Rc::ptr_eq(known, &part)) {
            kept.push(part);
        }
    }
    if kept.len() == 1 {
        return kept.remove(0);
    }
    Rc::new(node(kept))
}
