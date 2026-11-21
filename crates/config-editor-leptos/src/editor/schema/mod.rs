//! The compiled schema: the [`Schema`] a host compiles once and shares, the
//! [`SchemaSource`] an editor is given, and the walk every pass over the
//! schema text reads it with (see `subschemas`).

mod compile;
mod empty_refs;
#[cfg(feature = "schema-validation")]
mod false_schema;
mod subschemas;

pub(crate) use compile::{SchemaBundle, build_schema};
#[cfg(feature = "schema-validation")]
pub(crate) use false_schema::{for_validation, message as false_schema_message};
pub(crate) use subschemas::each_schema;
#[cfg(any(feature = "schema-validation", feature = "yaml"))]
pub(crate) use subschemas::{Holds, holds};

use super::diagnostics::Diagnostic;
use crate::EditorStrings;
use crate::strings::{default_strings, fill};
use serde_json::Value;

/// The message for a schema that does not compile, `error` as
/// [`build_schema`] gives it (empty: the text is).
pub(crate) fn schema_message(strings: &EditorStrings, error: &str) -> String {
    if error.is_empty() {
        strings.empty_schema.clone()
    } else {
        fill(&strings.invalid_schema, &[("error", &error)])
    }
}

/// A JSON Schema that failed to compile, with the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError(pub String);

impl std::fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SchemaError {}

/// A compiled JSON Schema, cheap to clone.
///
/// One schema validates documents in every language, because every language
/// parses into the same data model.
///
/// ```
/// # #[cfg(all(feature = "schema-validation", feature = "yaml", feature = "toml"))] {
/// use leptos_config_editor::{Language, Schema};
/// let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
/// assert!(schema.validate(Language::YAML, "listen: 7835\n").is_empty());
/// let problems = schema.validate(Language::TOML, "listen = \"x\"\n");
/// assert_eq!(problems[0].line, Some(1));
/// # }
/// ```
#[derive(Clone)]
pub struct Schema {
    pub(crate) bundle: std::sync::Arc<SchemaBundle>,
}

impl std::fmt::Debug for Schema {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let title = self.bundle.schema.get("title").and_then(Value::as_str);
        formatter
            .debug_struct("Schema")
            .field("title", &title)
            .finish_non_exhaustive()
    }
}

/// The `schema` of a [`ConfigEditor`](crate::ConfigEditor): a JSON Schema
/// document as JSON or YAML text, or a [`Schema`] compiled once and shared
/// between editors.
///
/// Converts from `&str`, `String`, and `Schema`, so `schema=SCHEMA_JSON` and
/// `schema=compiled.clone()` both work.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SchemaSource {
    /// A JSON Schema document as JSON text, or as YAML text (feature
    /// `yaml`), compiled when the editor mounts; a compile error is shown
    /// inline.
    Text(String),
    /// An already compiled schema.
    Compiled(Schema),
}

impl From<&str> for SchemaSource {
    fn from(text: &str) -> Self {
        SchemaSource::Text(text.to_string())
    }
}

impl From<String> for SchemaSource {
    fn from(text: String) -> Self {
        SchemaSource::Text(text)
    }
}

impl From<Schema> for SchemaSource {
    fn from(schema: Schema) -> Self {
        SchemaSource::Compiled(schema)
    }
}

impl Schema {
    /// Whether both are handles on one compiled schema (clones of each
    /// other), not merely equal ones.
    pub(crate) fn shares(&self, other: &Schema) -> bool {
        std::sync::Arc::ptr_eq(&self.bundle, &other.bundle)
    }
}

impl SchemaSource {
    pub(crate) fn resolve(self) -> Result<std::sync::Arc<SchemaBundle>, String> {
        match self {
            SchemaSource::Text(text) => build_schema(&text).map(std::sync::Arc::new),
            SchemaSource::Compiled(schema) => Ok(schema.bundle),
        }
    }
}

impl Schema {
    /// Compiles a JSON Schema document written as JSON, or as YAML
    /// (feature `yaml`). A text that is JSON is read as JSON; one that is
    /// not and does not start with `{` or `[` is read as YAML.
    ///
    /// ```
    /// # #[cfg(feature = "yaml")] {
    /// use leptos_config_editor::Schema;
    /// let from_yaml = Schema::compile("properties:\n  listen: {type: integer}\n").unwrap();
    /// assert!(from_yaml.warnings().is_empty());
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when `schema_text` is neither JSON nor YAML,
    /// or not a valid schema.
    pub fn compile(schema_text: &str) -> Result<Self, SchemaError> {
        build_schema(schema_text)
            .map(|bundle| Schema {
                bundle: std::sync::Arc::new(bundle),
            })
            .map_err(|error| SchemaError(schema_message(default_strings(), &error)))
    }

    /// What the editor cannot use in this schema, one message each, such as
    /// a `patternProperties` regex its engine cannot compile (keys it was
    /// meant to match get no completions or hover docs). The editor passes
    /// each to `on_schema_warning` when it mounts.
    ///
    /// ```
    /// use leptos_config_editor::Schema;
    /// let schema = Schema::compile(r#"{"patternProperties":{"^(?!x-)":{}}}"#).unwrap();
    /// assert_eq!(schema.warnings().len(), 1);
    /// ```
    pub fn warnings(&self) -> &[String] {
        &self.bundle.warnings
    }

    /// Syntax and schema diagnostics for `source`, as the editor shows them
    /// (the editor's own messages in English, the `strings` defaults).
    pub fn validate(&self, language: super::Language, source: &str) -> Vec<Diagnostic> {
        super::byte_order_mark::read(source, |source, _| {
            super::diagnostics::headless_validate(source, Some(&self.bundle), language.behavior())
        })
    }

    /// The suggestions the editor lists at `cursor` (a UTF-16 offset in
    /// `source`) on `Ctrl+Space`, best first: the keys and values this schema
    /// allows there, written as `language` writes them. Accept one with
    /// [`Suggestion::accept`](super::Suggestion::accept). A custom language
    /// checks its completion methods with it, no browser needed.
    ///
    /// ```
    /// # #[cfg(feature = "toml")] {
    /// use leptos_config_editor::{Language, Schema};
    /// let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
    /// let offered = schema.suggestions(Language::TOML, "lis", 3);
    /// assert_eq!(offered[0].label(), "listen");
    /// assert_eq!(offered[0].accept("lis").0, "listen = ");
    /// # }
    /// ```
    pub fn suggestions(
        &self,
        language: super::Language,
        source: &str,
        cursor: usize,
    ) -> Vec<super::Suggestion> {
        super::byte_order_mark::read(source, |source, to| {
            let source = super::line_breaks::as_lf(source);
            super::completion::schema_suggestions(
                &self.bundle,
                language.behavior(),
                &source,
                to.document(cursor),
            )
        })
    }

    /// The docs the hover card shows for the key at `position` in `source`
    /// (or for the key of a value the schema documents), as `language` reads
    /// it; `None` where no schema key is.
    pub fn hover(
        &self,
        language: super::Language,
        source: &str,
        position: super::TextPosition,
    ) -> Option<super::HoverDocs> {
        super::byte_order_mark::read(source, |source, to| {
            let source = super::line_breaks::as_lf(source);
            super::completion::schema_hover(
                &self.bundle,
                language.behavior(),
                &source,
                to.document(position),
            )
        })
    }

    /// Converts `source` from one language to another, reading it as this
    /// schema expects: a single XML element where the schema wants an array
    /// is a one-item array, and converts to one.
    ///
    /// # Errors
    ///
    /// As [`Language::convert`](super::Language::convert).
    pub fn convert(
        &self,
        from: super::Language,
        source: &str,
        to: super::Language,
    ) -> Result<super::Conversion, super::ConvertError> {
        super::byte_order_mark::read(source, |source, _| {
            super::codec::convert_away(
                source,
                from.behavior(),
                to.behavior(),
                Some(&self.bundle.schema),
            )
        })
    }
}
