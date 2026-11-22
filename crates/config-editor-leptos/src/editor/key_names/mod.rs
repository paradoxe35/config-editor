//! Names for fields whose value names a key defined elsewhere (a string
//! schema's `x-config-editor-keys`), and for the keys of an object whose
//! `propertyNames` has it ([`property_names`]): the keyword ([`keyword`]),
//! the names a host gives and the schema identity they are matched by
//! ([`names`]). Suggestions only: never a diagnostic.

mod definition;
mod go_to;
mod keyword;
mod names;
mod offer;
mod property_names;

pub(crate) use keyword::{KEYWORD, key_places, pointers_in};
pub use names::{GoToDefinition, KeyName, KeyNames};
pub(crate) use names::{KeptKeys, KeyNameSource, NoKeyNames, SchemaIdentity, keys_in};
pub(crate) use offer::{KeyOrigin, NameLookup, Offered};
pub(crate) use property_names::key_branches;

/// What an editor reads the names of its `x-config-editor-keys` fields
/// from, and where go to definition goes for a name it does not hold.
#[derive(Clone)]
pub(crate) struct KeyNamesInput {
    /// The names beyond the document's own (the host's `key_names`; a
    /// documents view's other open documents and host names).
    pub(crate) source: std::sync::Arc<dyn KeyNameSource>,
    /// Called for a name not defined in the document itself.
    pub(crate) on_go_to_definition: Option<leptos::prelude::Callback<GoToDefinition>>,
}

impl Default for KeyNamesInput {
    fn default() -> Self {
        Self {
            source: std::sync::Arc::new(NoKeyNames),
            on_go_to_definition: None,
        }
    }
}

#[cfg(all(test, feature = "yaml"))]
mod tests;
