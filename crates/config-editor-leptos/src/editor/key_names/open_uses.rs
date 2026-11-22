//! The open documents an editor's Find uses reads beyond its own (a
//! documents view's), and the way to one of their places.

use super::{NameRef, SchemaIdentity, UseHit};
use crate::FieldPath;

/// The uses of a name in one open document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocumentUses {
    /// The document's id.
    pub(crate) id: String,
    /// Its file name, as its tab shows it.
    pub(crate) label: String,
    pub(crate) hits: Vec<UseHit>,
}

/// The open documents beside an editor's own.
pub(crate) trait OpenUses: Send + Sync {
    /// The uses of `name` in each open document but the asking one's, in
    /// tab order (those with none left out): one walk of each document
    /// whose schema names the name's map.
    fn uses(&self, name: &NameRef) -> Vec<DocumentUses>;

    /// Whether an open document's schema names the map at `pointer` of the
    /// documents of `schema` (by its `from`): its keys are names then.
    fn names_map(&self, schema: &SchemaIdentity, pointer: &str) -> bool;

    /// Whether `source` is an open document's id: its live uses stand for
    /// the ones a host lists for it.
    fn is_open(&self, source: &str) -> bool;

    /// Shows open document `id` with its caret on `path`. Returns whether
    /// it could.
    fn go_to(&self, id: &str, path: &FieldPath) -> bool;
}

/// No other open document: a lone editor.
pub(crate) struct NoOpenUses;

impl OpenUses for NoOpenUses {
    fn uses(&self, _: &NameRef) -> Vec<DocumentUses> {
        Vec::new()
    }

    fn names_map(&self, _: &SchemaIdentity, _: &str) -> bool {
        false
    }

    fn is_open(&self, _: &str) -> bool {
        false
    }

    fn go_to(&self, _: &str, _: &FieldPath) -> bool {
        false
    }
}
