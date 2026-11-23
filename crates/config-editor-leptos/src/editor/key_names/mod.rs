//! Names for fields whose value names a key defined elsewhere (a string
//! schema's `x-config-editor-keys`), and for the keys of an object whose
//! `propertyNames` has it ([`property_names`]): the keyword ([`keyword`]),
//! the names a host gives and the schema identity they are matched by
//! ([`names`]), go to definition ([`go_to`]) and Find uses ([`uses`] finds
//! them, [`find`] and [`list`] show them). Suggestions only: never a
//! diagnostic.

mod definition;
mod find;
mod go_to;
mod hidden;
mod keyword;
mod list;
mod names;
mod offer;
mod open_uses;
mod property_names;
mod use_types;
mod uses;

// Only the YAML-built uses tests read them.
#[cfg(all(test, feature = "yaml"))]
pub(in crate::editor) use find::{UseTarget, uses_said};
pub(in crate::editor) use find::{close_uses_on_moves, empty_text, title};
pub(crate) use hidden::hide_use_secrets_by_schema;
pub(crate) use keyword::{KEYWORD, key_places, pointers_in};
pub(crate) use list::{UseGroup, UsesList, UsesState};
pub use names::{GoToDefinition, KeyName, KeyNames};
pub(crate) use names::{KeptKeys, KeyNameSource, NoKeyNames, SchemaIdentity, keys_in};
pub(crate) use offer::{KeyOrigin, NameLookup, Offered};
pub(crate) use open_uses::{DocumentUses, NoOpenUses, OpenUses};
pub(crate) use property_names::key_branches;
pub use use_types::{FindUses, FoundUse, GoToUse, Use};
pub(crate) use use_types::{HostAnswer, UsesReply};
// Only the language matrices and the random suites, which need every
// built-in language, read them.
pub(crate) use uses::{NameRef, UseHit, uses_in, uses_in_language};
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
pub(crate) use {
    definition::{name_at, searched_at},
    uses::UsePart,
};

/// What an editor reads the names of its `x-config-editor-keys` fields
/// from, where go to definition goes for a name it does not hold, and
/// where Find uses looks beyond the document.
#[derive(Clone)]
pub(crate) struct KeyNamesInput {
    /// The names beyond the document's own (the host's `key_names`; a
    /// documents view's other open documents and host names).
    pub(crate) source: std::sync::Arc<dyn KeyNameSource>,
    /// Called for a name not defined in the document itself.
    pub(crate) on_go_to_definition: Option<leptos::prelude::Callback<GoToDefinition>>,
    /// The other open documents (a documents view's), searched for uses.
    pub(crate) open: std::sync::Arc<dyn OpenUses>,
    /// Asked for the uses in the files that are not open.
    pub(crate) on_find_uses: Option<leptos::prelude::Callback<FindUses>>,
    /// Told of a use the host listed that the user chose.
    pub(crate) on_go_to_use: Option<leptos::prelude::Callback<GoToUse>>,
}

impl Default for KeyNamesInput {
    fn default() -> Self {
        Self {
            source: std::sync::Arc::new(NoKeyNames),
            on_go_to_definition: None,
            open: std::sync::Arc::new(NoOpenUses),
            on_find_uses: None,
            on_go_to_use: None,
        }
    }
}

#[cfg(all(test, feature = "yaml"))]
mod tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "schema-validation"
))]
mod uses_list_tests;
#[cfg(all(test, feature = "yaml"))]
mod uses_tests;
