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
