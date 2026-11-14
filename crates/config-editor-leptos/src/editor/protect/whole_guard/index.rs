//! The host's placeholders given as a field's whole value, looked up by
//! token and by field: built once for the host's tokens (when they
//! change), so an edit asks where it is among them, never passes over
//! them all (thousands of fields holding one fixed mask cost every key a
//! pass over each field for each place the mask is written).

use std::collections::HashMap;
use std::ops::Range;

use super::super::super::language::{ConfigLanguage, Repetition};
use super::super::super::paths::field_key;
use super::super::super::work_count::{Work, note};
use super::super::tokens::Known;

/// Lookups among the host's tokens given whole in a field.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct WholeIndex {
    /// Where the first such token of each text is in the host's tokens.
    by_token: HashMap<String, usize>,
    /// Where each is, by its text and its field (lone repeated elements
    /// read either way).
    by_field: HashMap<(String, String), usize>,
    /// All of them, in the host's order.
    all: Vec<usize>,
    /// Their fields, sorted: as written, and without the `/0` steps of lone
    /// repeated elements (the form a language with them compares).
    fields: Vec<String>,
    lone_fields: Vec<String>,
    /// Whether one of them has a field the host's text could not tell
    /// (see `Known::placed`).
    unplaced: bool,
}

impl WholeIndex {
    pub(crate) fn new(known: &[Known]) -> Self {
        let mut index = WholeIndex::default();
        for (at, known) in known.iter().enumerate() {
            let Some(field) = known.field.as_deref().filter(|_| known.whole.is_some()) else {
                continue;
            };
            index.by_token.entry(known.token.clone()).or_insert(at);
            // One whose field the host's text could not tell is placed by
            // where it is written, never by that field.
            if !known.placed {
                index.unplaced = true;
                continue;
            }
            index
                .by_field
                .entry((known.token.clone(), field_key(field)))
                .or_insert(at);
            index.all.push(at);
            index.fields.push(field.to_string());
            index.lone_fields.push(field_key(field));
        }
        for fields in [&mut index.fields, &mut index.lone_fields] {
            fields.sort_unstable();
            fields.dedup();
        }
        index
    }

    /// Whether a token's field is to be read where it is written (see
    /// `Known::placed`).
    pub(crate) fn has_unplaced(&self) -> bool {
        self.unplaced
    }

    /// Whether no token is given whole.
    pub(crate) fn is_empty(&self) -> bool {
        self.by_token.is_empty()
    }

    /// The tokens given whole, as positions in the host's tokens.
    pub(super) fn all(&self) -> impl Iterator<Item = usize> + '_ {
        self.all.iter().copied()
    }

    /// The position of the first token written `token` given whole.
    pub(super) fn first_of(&self, token: &str) -> Option<usize> {
        self.by_token.get(token).copied()
    }

    /// The position of the token written `token` given whole in the field
    /// at `pointer` (lone repeated elements read either way): one lookup.
    pub(super) fn in_field(&self, token: &str, pointer: &str) -> Option<usize> {
        note(Work::TokenScan);
        self.by_field
            .get(&(token.to_string(), field_key(pointer)))
            .copied()
    }

    /// Whether `path` (as `behavior` compares paths) is, holds or is inside
    /// a field holding a token given whole: a lookup for each step of the
    /// path, and one for the fields under it.
    pub(super) fn reads(&self, behavior: &dyn ConfigLanguage, path: &str) -> bool {
        let (path, fields) = if behavior.repetition() == Repetition::None {
            (path.to_string(), &self.fields)
        } else {
            (field_key(path), &self.lone_fields)
        };
        let has = |pointer: &str| {
            fields
                .binary_search_by(|field| field.as_str().cmp(pointer))
                .is_ok()
        };
        // A field holding the path (the root's too) or the path itself.
        let holding = std::iter::once(0)
            .chain(
                path.match_indices('/')
                    .map(|(at, _)| at)
                    .filter(|at| *at > 0),
            )
            .chain(std::iter::once(path.len()))
            .any(|end| has(&path[..end]));
        if holding {
            return true;
        }
        // A field inside the path.
        let under = format!("{path}/");
        let next = fields.partition_point(|field| field.as_str() < under.as_str());
        path.is_empty() && !fields.is_empty()
            || fields
                .get(next)
                .is_some_and(|field| field.starts_with(&under))
    }
}

/// Whether one of `tokens` (UTF-16 ranges, in order and apart) is on the
/// UTF-16 `lines` (ends included).
pub(super) fn on_lines(tokens: &[Range<usize>], lines: &Range<usize>) -> bool {
    let next = tokens.partition_point(|token| token.end < lines.start);
    tokens
        .get(next)
        .is_some_and(|token| token.start <= lines.end)
}
