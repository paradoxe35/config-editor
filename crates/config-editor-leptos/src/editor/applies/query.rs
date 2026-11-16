//! Reading a walked tree: its schemas (all, certain, or offered), each
//! once in order, each shared subtree read once.

use super::node::{Choice, Node, State, Tree, Walked};
use serde_json::Value;
use std::collections::HashSet;
use std::rc::Rc;

/// The schemas of `tree` a walk `walked` takes, once each, in order.
pub(crate) fn leaves<'s>(tree: &Tree<'s>, walked: Walked) -> Vec<&'s Value> {
    let mut found = Leaves {
        walked,
        visited: HashSet::new(),
        seen: HashSet::new(),
        leaves: Vec::new(),
    };
    found.of(tree);
    found.leaves
}

/// Whether `tree` holds a `oneOf`/`anyOf` option the value refutes.
pub(crate) fn refutes(tree: &Tree<'_>) -> bool {
    finds_options(tree, |choices| {
        choices.iter().any(|choice| choice.state == State::Refuted)
    })
}

/// Whether `tree` holds a `oneOf`/`anyOf` whose options `found` says yes
/// to.
fn finds_options(tree: &Tree<'_>, found: impl Fn(&[Choice<'_>]) -> bool) -> bool {
    let mut visited = HashSet::new();
    let mut next = vec![tree.clone()];
    while let Some(tree) = next.pop() {
        if !visited.insert(Rc::as_ptr(&tree)) {
            continue;
        }
        match &*tree {
            Node::Options(choices) if found(choices) => return true,
            Node::Options(choices) => next.extend(choices.iter().map(|choice| choice.tree.clone())),
            Node::All(parts) | Node::Any(parts) | Node::Either(parts) => {
                next.extend(parts.iter().cloned());
            }
            Node::Leaf(_) | Node::Cut => {}
        }
    }
    false
}

struct Leaves<'s> {
    walked: Walked,
    visited: HashSet<*const Node<'s>>,
    seen: HashSet<*const Value>,
    leaves: Vec<&'s Value>,
}

impl<'s> Leaves<'s> {
    fn of(&mut self, tree: &Tree<'s>) {
        if !self.visited.insert(Rc::as_ptr(tree)) {
            return;
        }
        match &**tree {
            Node::Leaf(schema) => {
                if self.seen.insert(std::ptr::from_ref(*schema)) {
                    self.leaves.push(schema);
                }
            }
            Node::All(parts) => parts.iter().for_each(|part| self.of(part)),
            Node::Any(parts) => parts.iter().for_each(|part| self.of(part)),
            Node::Either(parts) => {
                if self.walked != Walked::Certain {
                    parts.iter().for_each(|part| self.of(part));
                }
            }
            Node::Options(choices) => {
                let matched = choices.iter().any(|choice| choice.state.taken());
                for choice in choices {
                    let takes = match self.walked {
                        Walked::Any => true,
                        Walked::Certain => choice.state.taken(),
                        Walked::Offered => {
                            choice.state.taken() || (!matched && choice.state == State::Open)
                        }
                    };
                    if takes {
                        self.of(&choice.tree);
                    }
                }
            }
            Node::Cut => {}
        }
    }
}
