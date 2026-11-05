//! Document languages supported by the editor.
//!
//! Each language lives in its own file behind [`LanguageBehavior`] (defined
//! in Phase 2). This module currently holds the [`Language`] selector only;
//! per-language behavior moves here verbatim in Phase 2 (`yaml.rs`) and
//! Phase 3 (`json.rs`).

/// Document language edited by the component.
///
/// Selects the text layer (parsing, completion context, inserts, hover
/// lookup, highlighting, newline/indent, comments). Everything else —
/// history, diffs, completion ranking, schema navigation, popups — is
/// language-independent and stays shared.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    /// YAML documents (default; preserves the original editor behavior).
    #[default]
    Yaml,
    /// JSON documents.
    Json,
}

impl Language {
    /// Short UI label: `"YAML"` or `"JSON"`. Toolbar, aria labels, and
    /// diagnostic messages derive from this so no language name is
    /// hard-coded in markup.
    pub fn label(self) -> &'static str {
        match self {
            Language::Yaml => "YAML",
            Language::Json => "JSON",
        }
    }

    /// Single-letter file icon for the built-in toolbar.
    pub fn file_icon(self) -> &'static str {
        match self {
            Language::Yaml => "Y",
            Language::Json => "J",
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
        assert_eq!(Language::Yaml.file_icon(), "Y");
        assert_eq!(Language::Json.file_icon(), "J");
    }
}
