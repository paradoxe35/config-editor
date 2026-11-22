//! The names a host gives for fields that name a key defined elsewhere
//! ([`KeyNames`]), the identity of a schema they are matched by
//! ([`SchemaIdentity`]), and the one question the suggestions, the hover
//! and go to definition ask of every place names come from
//! ([`KeyNameSource`]).

use std::hash::{Hash, Hasher};

use serde_json::Value;

/// A name a field may take: a key of the map some field's
/// `x-config-editor-keys` points at.
///
/// ```
/// use leptos_config_editor::KeyName;
/// let name = KeyName::new("home").with_source("server.d/home.yaml");
/// assert_eq!(name.source.as_deref(), Some("server.d/home.yaml"));
/// assert_eq!(KeyName::from("work").source, None);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyName {
    /// The key.
    pub name: String,
    /// Where it is defined (a file name, a document id), shown beside it
    /// and handed to `on_go_to_definition`.
    pub source: Option<String>,
    /// A word about it, shown with its source.
    pub detail: Option<String>,
}

impl KeyName {
    /// The name `name`, with no source or detail.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: None,
            detail: None,
        }
    }

    /// The name, defined in `source`.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// The name, with `detail` shown beside it.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl From<&str> for KeyName {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for KeyName {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

/// The names a host knows for the map at `pointer` of the documents of one
/// schema (files that are not open): the `key_names` of a
/// [`ConfigEditor`](crate::ConfigEditor).
///
/// `schema` is the `$id` of the schema the names belong to; an entry with
/// none applies only to documents whose schema has no `$id`.
///
/// ```
/// use leptos_config_editor::{KeyName, KeyNames};
/// let names = KeyNames::new("/clients", ["home", "work"])
///     .for_schema("https://example.com/server");
/// assert_eq!(names.names[1], KeyName::new("work"));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyNames {
    /// The `$id` of the schema of the documents the names are keys of.
    pub schema: Option<String>,
    /// The JSON pointer of the map whose keys they are (`/clients`).
    pub pointer: String,
    /// The names, in the order offered.
    pub names: Vec<KeyName>,
}

impl KeyNames {
    /// The names of the map at `pointer`, for documents whose schema has
    /// no `$id`.
    pub fn new<N: Into<KeyName>>(
        pointer: impl Into<String>,
        names: impl IntoIterator<Item = N>,
    ) -> Self {
        Self {
            schema: None,
            pointer: pointer.into(),
            names: names.into_iter().map(Into::into).collect(),
        }
    }

    /// The names, for documents whose schema's `$id` is `id`.
    #[must_use]
    pub fn for_schema(mut self, id: impl Into<String>) -> Self {
        self.schema = Some(id.into());
        self
    }
}

/// What a name's place in the user's go to definition asks the host to
/// open: the name, the map it is a key of, and where it is defined. Given
/// to `on_go_to_definition` for a name not in the document itself.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GoToDefinition {
    /// The `$id` of the schema of the documents the name is a key of
    /// (`None`: the document's own schema, which has no `$id`).
    pub schema: Option<String>,
    /// The JSON pointer of the map the name is a key of.
    pub pointer: String,
    /// The name.
    pub name: String,
    /// Where it is defined, as its [`KeyName`] says.
    pub source: Option<String>,
}

/// Which schema a document is of, for matching names to it: its `$id`,
/// else its text (two documents of the same schema text share it, as a
/// documents view shares one compiled schema between them).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SchemaIdentity {
    /// The schema's `$id`.
    Id(String),
    /// A hash of the schema text, for a schema with no `$id`.
    Text(u64),
}

impl SchemaIdentity {
    /// The identity of `schema`, compiled from `text`.
    pub(crate) fn of(schema: &Value, text: &str) -> Self {
        match schema.get("$id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => Self::Id(id.to_string()),
            _ => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                text.hash(&mut hasher);
                Self::Text(hasher.finish())
            }
        }
    }

    /// The `$id`, if the schema has one.
    pub(crate) fn id(&self) -> Option<&str> {
        match self {
            Self::Id(id) => Some(id),
            Self::Text(_) => None,
        }
    }

    /// Whether a host entry for the schema `entry` (its `$id`, `None` for
    /// one written without) applies to documents of this schema.
    pub(crate) fn takes(&self, entry: Option<&str>) -> bool {
        self.id() == entry
    }
}

/// Where the names a field may take come from, beyond the document's own
/// keys: the host's [`KeyNames`], and in a documents view, the other open
/// documents too. Asked per suggestion run, hover and go to definition;
/// an implementation reading signals is tracked by the run that asks.
pub(crate) trait KeyNameSource: Send + Sync {
    /// The names at `pointer` of the documents whose schema is `schema`,
    /// in the order offered (a name may repeat with another source: the
    /// first is offered).
    fn names(&self, schema: &SchemaIdentity, pointer: &str) -> Vec<KeyName>;
}

impl KeyNameSource for [KeyNames] {
    fn names(&self, schema: &SchemaIdentity, pointer: &str) -> Vec<KeyName> {
        self.iter()
            .filter(|entry| entry.pointer == pointer && schema.takes(entry.schema.as_deref()))
            .flat_map(|entry| entry.names.iter().cloned())
            .collect()
    }
}

impl KeyNameSource for Vec<KeyNames> {
    fn names(&self, schema: &SchemaIdentity, pointer: &str) -> Vec<KeyName> {
        self.as_slice().names(schema, pointer)
    }
}

/// The host's names as the `key_names` signal holds them, read tracked.
impl KeyNameSource for leptos::prelude::Signal<Vec<KeyNames>> {
    fn names(&self, schema: &SchemaIdentity, pointer: &str) -> Vec<KeyName> {
        use leptos::prelude::With;
        self.try_with(|entries| entries.names(schema, pointer))
            .unwrap_or_default()
    }
}

/// No names: an editor without `key_names`, the headless suggestions.
pub(crate) struct NoKeyNames;

impl KeyNameSource for NoKeyNames {
    fn names(&self, _: &SchemaIdentity, _: &str) -> Vec<KeyName> {
        Vec::new()
    }
}

/// The keys at each pointer the schema's keyword names, as the last text
/// of an editor's document that parsed has them: what the document offers
/// while its text does not parse.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeptKeys(std::collections::HashMap<String, Vec<String>>);

impl KeptKeys {
    /// Keeps the keys `document` (a text that parsed) has at each of
    /// `pointers`.
    pub(crate) fn remember(&mut self, document: &Value, pointers: &[String]) {
        for pointer in pointers {
            let keys = keys_in(document, pointer).unwrap_or_default();
            if self.0.get(pointer) != Some(&keys) {
                self.0.insert(pointer.clone(), keys);
            }
        }
    }

    /// The keys kept at `pointer`, if any text that parsed was read.
    pub(crate) fn at(&self, pointer: &str) -> Option<&[String]> {
        self.0.get(pointer).map(Vec::as_slice)
    }
}

/// The keys of the map at `pointer` in `document`, in the order written;
/// `None` when no map is there.
pub(crate) fn keys_in(document: &Value, pointer: &str) -> Option<Vec<String>> {
    document
        .pointer(pointer)?
        .as_object()
        .map(|map| map.keys().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_entry_with_no_schema_applies_only_to_a_schema_with_no_id() {
        let with_id = SchemaIdentity::of(&json!({"$id": "https://x/s"}), "{}");
        let without = SchemaIdentity::of(&json!({}), "{}");
        let entries = vec![
            KeyNames::new("/clients", ["plain"]),
            KeyNames::new("/clients", ["identified"]).for_schema("https://x/s"),
            KeyNames::new("/clients", ["other"]).for_schema("https://x/t"),
            KeyNames::new("/groups", ["elsewhere"]),
        ];
        let names = |schema: &SchemaIdentity| -> Vec<String> {
            entries
                .names(schema, "/clients")
                .into_iter()
                .map(|name| name.name)
                .collect()
        };
        assert_eq!(names(&with_id), ["identified"]);
        assert_eq!(names(&without), ["plain"]);
    }

    #[test]
    fn a_schema_with_no_id_is_known_by_its_text() {
        let schema = json!({"type": "object"});
        assert_eq!(
            SchemaIdentity::of(&schema, "a"),
            SchemaIdentity::of(&schema, "a")
        );
        assert_ne!(
            SchemaIdentity::of(&schema, "a"),
            SchemaIdentity::of(&schema, "b")
        );
        assert_eq!(
            SchemaIdentity::of(&json!({"$id": "s"}), "a"),
            SchemaIdentity::of(&json!({"$id": "s"}), "b")
        );
    }

    #[test]
    fn keys_in_reads_the_map_at_the_pointer_only() {
        let document = json!({"clients": {"home": {}, "work": 1}, "list": [1], "a/b": {"c": 1}});
        assert_eq!(
            keys_in(&document, "/clients"),
            Some(vec!["home".to_string(), "work".to_string()])
        );
        assert_eq!(keys_in(&document, "/a~1b"), Some(vec!["c".to_string()]));
        assert_eq!(keys_in(&document, "/list"), None);
        assert_eq!(keys_in(&document, "/missing"), None);
        assert_eq!(keys_in(&document, "").map(|keys| keys.len()), Some(3));
    }
}
