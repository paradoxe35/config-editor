//! The names a field's schemas offer: the keys at each place the keyword
//! names, in the document itself first, then from the editor's
//! [`KeyNameSource`] (the host's, a documents view's other documents), each
//! name once, with where it comes from.

use std::collections::HashSet;

use serde_json::Value;

use super::{KeptKeys, KeyNameSource, SchemaIdentity, key_places, keys_in};
use crate::EditorStrings;
use crate::strings::fill;

/// Where a name offered comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyOrigin {
    /// A key of the document itself.
    ThisDocument,
    /// A name with this source (a file name, a document id).
    Source(String),
    /// A name given with no source.
    Unnamed,
}

impl KeyOrigin {
    /// The source as shown (the `keyNameThisDocument` text for the
    /// document itself); `None` for a name given with none.
    pub(crate) fn label(&self, strings: &EditorStrings) -> Option<String> {
        match self {
            Self::ThisDocument => Some(strings.key_name_this_document.clone()),
            Self::Source(source) => Some(source.clone()),
            Self::Unnamed => None,
        }
    }

    /// What a suggestion shows beside the name: its source, with the
    /// name's own `detail` when it has one.
    pub(crate) fn detail(&self, detail: Option<&str>, strings: &EditorStrings) -> String {
        match (
            self.label(strings),
            detail.filter(|detail| !detail.is_empty()),
        ) {
            (Some(source), Some(detail)) => fill(
                &strings.key_name_detail,
                &[("source", &source), ("detail", &detail)],
            ),
            (Some(source), None) => source,
            (None, detail) => detail.unwrap_or_default().to_string(),
        }
    }

    /// The hover card's line for the name: "defined in <source>".
    pub(crate) fn defined_in(&self, strings: &EditorStrings) -> Option<String> {
        let source = self.label(strings)?;
        Some(fill(&strings.key_name_defined_in, &[("source", &source)]))
    }
}

/// A name a field may take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offered {
    pub(crate) name: String,
    pub(crate) origin: KeyOrigin,
    /// The name's own detail (a host's [`KeyName::detail`](super::KeyName)).
    pub(crate) detail: Option<String>,
    /// The schema of the documents it is a key of.
    pub(crate) schema: SchemaIdentity,
    /// The pointer of the map it is a key of.
    pub(crate) pointer: String,
}

/// What finding the names of a field needs besides its schemas: the
/// identity of the document's schema and the names beyond the document.
#[derive(Clone, Copy)]
pub(crate) struct NameLookup<'a> {
    pub(crate) identity: &'a SchemaIdentity,
    pub(crate) source: &'a dyn KeyNameSource,
    /// The document's keys from its last text that parsed, given while
    /// its text does not parse: they stand for the document's own.
    pub(crate) kept: Option<&'a KeptKeys>,
}

impl NameLookup<'_> {
    /// The names `branches` (the schemas applying to one value) offer, in
    /// order: for each place their keyword names, the keys at it in
    /// `document` (read only when some branch has the keyword), then the
    /// source's for this schema and each `from` schema. Each name once,
    /// the first found.
    pub(crate) fn offered<'d>(
        &self,
        branches: &[&Value],
        document: impl FnOnce() -> Option<&'d Value>,
    ) -> Vec<Offered> {
        let places: Vec<_> = branches
            .iter()
            .flat_map(|branch| key_places(branch))
            .collect();
        if places.is_empty() {
            return Vec::new();
        }
        let document = document();
        let mut seen = HashSet::new();
        let mut offered = Vec::new();
        let mut offer = |offered: &mut Vec<Offered>, name: Offered| {
            if seen.insert(name.name.clone()) {
                offered.push(name);
            }
        };
        for place in &places {
            let own = self
                .kept
                .and_then(|kept| kept.at(&place.pointer))
                .map(<[String]>::to_vec)
                .or_else(|| document.and_then(|document| keys_in(document, &place.pointer)));
            for name in own.into_iter().flatten() {
                offer(
                    &mut offered,
                    Offered {
                        name,
                        origin: KeyOrigin::ThisDocument,
                        detail: None,
                        schema: self.identity.clone(),
                        pointer: place.pointer.clone(),
                    },
                );
            }
        }
        for place in &places {
            let schemas = std::iter::once(self.identity.clone()).chain(
                place
                    .from
                    .iter()
                    .filter(|id| self.identity.id() != Some(id.as_str()))
                    .map(|id| SchemaIdentity::Id(id.clone())),
            );
            for schema in schemas {
                for name in self.source.names(&schema, &place.pointer) {
                    offer(
                        &mut offered,
                        Offered {
                            origin: name.source.map_or(KeyOrigin::Unnamed, KeyOrigin::Source),
                            name: name.name,
                            detail: name.detail,
                            schema: schema.clone(),
                            pointer: place.pointer.clone(),
                        },
                    );
                }
            }
        }
        offered
    }
}
