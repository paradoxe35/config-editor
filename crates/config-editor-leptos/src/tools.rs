//! Helpers for a host working with documents outside an editor.

use crate::Language;

/// The keys of the map at the JSON pointer `pointer` of `text`, read as
/// `language`, in the order written: the names a host gives in
/// [`KeyNames`](crate::KeyNames) for a file that is not open. `None` when
/// the text does not parse (keep the last good names), and an empty list
/// when no map is at `pointer`.
///
/// ```
/// # #[cfg(feature = "yaml")] {
/// use leptos_config_editor::{Language, tools::keys_at};
/// let text = "clients:\n  home: {}\n  work: {}\n";
/// let keys = keys_at(text, Language::YAML, "/clients");
/// assert_eq!(keys, Some(vec!["home".to_string(), "work".to_string()]));
/// assert_eq!(keys_at(text, Language::YAML, "/servers"), Some(Vec::new()));
/// assert_eq!(keys_at("clients: [", Language::YAML, "/clients"), None);
/// # }
/// ```
pub fn keys_at(text: &str, language: Language, pointer: &str) -> Option<Vec<String>> {
    let document = language.parse(text).ok()?;
    Some(crate::editor::key_names::keys_in(&document, pointer).unwrap_or_default())
}

/// The uses of `name` in `text` (read as `language`, its schema `schema`):
/// every value, and every key an object's `propertyNames` reads, whose
/// schema's `x-config-editor-keys` names the map at `pointer` and that is
/// `name`, in the order written. It is what the editor lists for an open
/// document on `Shift+F12`, for a host answering
/// [`FindUses`](crate::FindUses) about its files that are not open (see
/// [`FoundUse::into_use`](crate::FoundUse::into_use)).
///
/// `of` is the `$id` of the schema of the documents the name is a key of
/// (the request's `schema`), for a file whose schema names them through
/// `from`; `None`: `schema` itself. `None` when the text does not parse.
///
/// ```
/// # #[cfg(feature = "yaml")] {
/// use leptos_config_editor::{FieldPath, Language, Schema, tools::uses_at};
/// let schema = Schema::compile(r#"{"properties": {
///     "clients": {"type": "object"},
///     "default_client": {"type": "string", "x-config-editor-keys": "/clients"}
/// }}"#).unwrap();
/// let text = "clients:\n  home: {}\ndefault_client: home\n";
/// let found = uses_at(text, Language::YAML, &schema, "/clients", "home", None).unwrap();
/// assert_eq!(found.len(), 1);
/// assert_eq!(found[0].path, FieldPath::from("/default_client"));
/// assert_eq!((found[0].line, found[0].text.as_str()), (3, "default_client: home"));
/// assert_eq!(uses_at("a: [", Language::YAML, &schema, "/clients", "home", None), None);
/// # }
/// ```
pub fn uses_at(
    text: &str,
    language: Language,
    schema: &crate::Schema,
    pointer: &str,
    name: &str,
    of: Option<&str>,
) -> Option<Vec<crate::FoundUse>> {
    use crate::editor::key_names::{NameRef, SchemaIdentity, uses_in_language};
    language.parse(text).ok()?;
    let bundle = &schema.bundle;
    let name = NameRef {
        schema: of.map_or_else(
            || bundle.identity.clone(),
            |id| SchemaIdentity::Id(id.to_string()),
        ),
        pointer: pointer.to_string(),
        name: name.to_string(),
    };
    let hits = uses_in_language(text, language, &bundle.schema, &bundle.identity, &name);
    Some(hits.into_iter().map(|hit| hit.found(text)).collect())
}
