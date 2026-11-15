//! Reading a walked tree: its schemas (all, certain, or offered), each
//! once in order, each shared subtree read once.

use super::node::{Node, State, Tree, Walked};
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
