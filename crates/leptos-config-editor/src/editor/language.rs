//! Core language contract. Language-agnostic: defines the
//! [`LanguageBehavior`] text-layer trait and the shared signature types.
//! Shared orchestration (`diagnostics.rs`, `completion.rs`, `keyboard.rs`,
//! `commands.rs`) only depends on this module. The [`Language`](super::languages::Language)
//! selector and its registry live in `languages/`; language implementations
//! (`languages/yaml.rs`, `languages/json.rs`, …) depend on this contract from
//! the other side. Production code in core never names a language file and
//! never matches on language variants — it programs against the trait.
//!
//! Test code is the deliberate exception: tests pin a concrete language via
//! `Language::Yaml.behavior()` to exercise the shared orchestration.

use leptos::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

/// A parse failure, mapped 1:1 to an inline diagnostic by the shared
/// validation orchestration. The marker width rule (`end_column` one past
/// `column`) lives at the map site, not here.
pub(super) struct ParseError {
    pub(super) message: String,
    pub(super) line: Option<usize>,
    pub(super) column: Option<usize>,
}

/// A path segment addressing a value inside the parsed document. Shared by
/// every language: schema navigation reads parsed `Value`s, never syntax.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum SchemaPathSegment {
    Key(String),
    ArrayItem(usize),
}

/// Where the caret is inside the document's structure: the schema path of the
/// enclosing mapping, the partially typed query, the replace range, and —
/// when editing a mapping value — which key's value is being edited.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct CompletionContext {
    pub(super) path: Vec<SchemaPathSegment>,
    pub(super) typed: String,
    pub(super) replace_start: usize,
    pub(super) replace_end: usize,
    pub(super) value_key: Option<String>,
    pub(super) value_needs_space: bool,
    pub(super) is_array_item: bool,
    pub(super) base_indent: String,
}

/// Splits a JSON-pointer path into unescaped segments. Generic path utility
/// used by every language's span mapping; lives in the core contract so both
/// the shared orchestration and the implementations share one definition.
#[cfg(feature = "schema-validation")]
pub(super) fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// The text layer each document language implements. The shared orchestration
/// (validation flow, completion ranking, hover docs, pairing mechanics) calls
/// into this; per-language files implement it.
/// Every method is exercised by the editor or the shared orchestrators —
/// helpers used only inside one language stay free functions in its file.
pub(super) trait LanguageBehavior: Send + Sync {
    /// Parses the document into a schema-navigable `Value`.
    fn parse(&self, source: &str) -> Result<Value, ParseError>;
    /// Source span of the key at a JSON-pointer path.
    #[cfg(feature = "schema-validation")]
    fn span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Source span of the value at a JSON-pointer path.
    #[cfg(feature = "schema-validation")]
    fn value_span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Source span of the first non-blank content, for whole-document errors.
    #[cfg(feature = "schema-validation")]
    fn first_span(&self, source: &str) -> Option<(usize, usize, usize)>;
    /// Structural context of the caret for completions and hover.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext;
    /// Keys already present in the mapping at `path`, to filter suggestions.
    /// `cursor_byte` is a byte index; the active line is ignored so a
    /// partially typed key does not hide the keys around it.
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>>;
    /// Insert text (plus caret offset) for a schema property completion.
    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        base_indent: &str,
        unit: &str,
    ) -> Option<(String, usize)>;
    /// Render a schema value (default, const, example) as document text.
    fn format_value(&self, value: &Value) -> String;
    /// The mapping key under `column` (a character index) on a single line,
    /// with its starting column. Comment-only lines yield `None`.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)>;
    /// Syntax-highlighted rendering of a single document line.
    fn highlight_line(&self, line: &str) -> AnyView;
    /// Indented newline insertion honoring the document's own style.
    fn newline_with_indent(&self, source: &str, cursor: usize) -> String;
    /// Toggles line comments over the selection. Returns the updated
    /// document with remapped UTF-16 selection bounds.
    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)>;
    /// Whether the caret sits inside a string or comment: the pairing guard.
    fn lexical_state(&self, source: &str, cursor: usize) -> (Option<char>, bool);
    /// How strongly `source` looks like this language (0–255). Reserved for a
    /// future `Language::Auto` selector that will detect the document type at
    /// runtime from the initial/baseline value and pick the highest-scoring
    /// behavior. Defaults to zero so a new language compiles before tuning it.
    #[allow(dead_code)]
    fn confidence(&self, _source: &str) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "schema-validation")]
    #[test]
    fn pointer_segments_unescape_in_order() {
        use super::pointer_segments;
        assert_eq!(pointer_segments("/a~1b/~0c"), vec!["a/b".to_string(), "~c".to_string()]);
        assert_eq!(pointer_segments(""), Vec::<String>::new());
    }
}
