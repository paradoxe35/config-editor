//! Language registry: the [`Language`] selector plus each variant's behavior.
//!
//! This is the only place (besides the `<ConfigEditor language=…>` call site)
//! that names document languages. Core (`language.rs`, `diagnostics.rs`,
//! `completion.rs`, `keyboard.rs`, `commands.rs`, `editor.rs`) never matches on
//! variants and never imports a language file — it calls `language.behavior()`
//! and programs against the [`LanguageBehavior`](super::language::LanguageBehavior)
//! trait. Adding a language means one new file here plus one line in the
//! `match` below — no core changes.

use super::language::LanguageBehavior;

mod json;
mod toml;
mod yaml;
use json::JsonBehavior;
use toml::TomlBehavior;
use yaml::YamlBehavior;

static YAML_BEHAVIOR: YamlBehavior = YamlBehavior;
static JSON_BEHAVIOR: JsonBehavior = JsonBehavior;
static TOML_BEHAVIOR: TomlBehavior = TomlBehavior;

/// Document language edited by the component.
///
/// A plain selector: no syntax lives here. Selects the text layer (parsing,
/// completion context, inserts, hover lookup, highlighting, newline/indent,
/// comments). Everything else — history, diffs, completion ranking, schema
/// navigation, popups — is language-independent and stays shared.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    /// YAML documents (default; preserves the original editor behavior).
    #[default]
    Yaml,
    /// JSON documents.
    Json,
    /// TOML documents.
    Toml,
}

impl Language {
    /// Short UI label: `"YAML"`, `"JSON"`, or `"TOML"`. Toolbar, aria labels, and
    /// diagnostic messages derive from this so no language name is
    /// hard-coded in markup.
    pub fn label(self) -> &'static str {
        match self {
            Language::Yaml => "YAML",
            Language::Json => "JSON",
            Language::Toml => "TOML",
        }
    }

    /// Single-letter file icon for the built-in toolbar.
    pub fn file_icon(self) -> &'static str {
        match self {
            Language::Yaml => "Y",
            Language::Json => "J",
            Language::Toml => "T",
        }
    }

    /// Default file name used by the built-in toolbar when the host provides
    /// none.
    pub fn default_file_name(self) -> &'static str {
        match self {
            Language::Yaml => "untitled.yaml",
            Language::Json => "untitled.json",
            Language::Toml => "untitled.toml",
        }
    }

    /// The behavior implementing this language's text layer. Returned as a
    /// `&'static` reference so call sites and closures share it freely.
    pub(super) fn behavior(self) -> &'static dyn LanguageBehavior {
        match self {
            Language::Yaml => &YAML_BEHAVIOR,
            Language::Json => &JSON_BEHAVIOR,
            Language::Toml => &TOML_BEHAVIOR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_language_is_yaml() {
        assert_eq!(Language::default(), Language::Yaml);
    }

    #[test]
    fn language_labels_and_icons_are_stable() {
        assert_eq!(Language::Yaml.label(), "YAML");
        assert_eq!(Language::Json.label(), "JSON");
        assert_eq!(Language::Toml.label(), "TOML");
        assert_eq!(Language::Yaml.file_icon(), "Y");
        assert_eq!(Language::Json.file_icon(), "J");
        assert_eq!(Language::Toml.file_icon(), "T");
    }

    #[test]
    fn default_file_names_carry_the_language_extension() {
        assert_eq!(Language::Yaml.default_file_name(), "untitled.yaml");
        assert_eq!(Language::Json.default_file_name(), "untitled.json");
        assert_eq!(Language::Toml.default_file_name(), "untitled.toml");
    }
}
