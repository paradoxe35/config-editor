//! Which host element each element of a value written over a host is, by
//! its path in the written text: the save's pairing
//! ([`Pairing`](crate::editor::item_sources::Pairing): the links first,
//! values telling the rest). A rewrite takes each element's name, markup
//! and layout from its own host element (`attribute_layout`,
//! `written_names`), and a save checks that it did (`kept_markup`).

use super::Seg;
use crate::editor::item_sources::{ItemSources, Pairing, Step};
use serde_json::Value;

pub(super) use crate::editor::item_sources::Side as Host;

/// An edit of `old` (the host's value) to `new`, read by element paths.
pub(super) struct Identity<'a>(Pairing<'a>);

impl<'a> Identity<'a> {
    /// `old` is `None` when the host does not read: each element is the
    /// host's at its own path.
    pub(super) fn of(old: Option<&'a Value>, new: &'a Value) -> Self {
        Identity(Pairing::of(old, new, true))
    }

    /// The identity the links tell (`sources`, with `origin` the value of
    /// the earlier host text they name), values telling the rest.
    pub(super) fn told(self, sources: Option<&'a ItemSources>, origin: Option<&'a Value>) -> Self {
        Identity(self.0.told(sources, origin))
    }

    /// The host path (as the host's text writes it: an index only for a
    /// repeated element) of the element at `path` in a text written from
    /// `new`, and the text it is in, or `None` for an element the edit
    /// added.
    pub(super) fn host_path(&self, path: &[Seg]) -> Option<(Vec<String>, Host)> {
        self.0.host_path(&steps(path)?)
    }

    /// For an element the edit added as an item of a list, the host path
    /// of the item it copies: one of that list's host items holding the
    /// same data.
    pub(super) fn copied(&self, path: &[Seg]) -> Option<Vec<String>> {
        self.0.copied(&steps(path)?)
    }
}

/// An element path as the pairing's steps: each name, with the index
/// after it when it has one.
fn steps(path: &[Seg]) -> Option<Vec<Step>> {
    let mut out: Vec<Step> = Vec::with_capacity(path.len());
    for segment in path {
        match segment {
            Seg::Key(key) => out.push((key.clone(), None)),
            Seg::Index(index) => out.last_mut()?.1 = Some(*index),
        }
    }
    Some(out)
}
