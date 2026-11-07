//! A lightweight, VS Code–style config editor component for Leptos, plus a
//! headless API for the same parsing, validation, and conversion.
//!
//! Languages: YAML (default), JSON, TOML, HCL, and XML. They all parse into
//! one data model (a [`serde_json::Value`]), so a single JSON Schema drives
//! diagnostics, completions, and hover docs for every language, and a
//! document converts between any two of them.
//!
//! # The component
//!
//! ```ignore
//! use leptos::prelude::*;
//! use config_editor_leptos::{ConfigEditor, Language};
//!
//! let saved = RwSignal::new("name: app\n".to_string());
//! view! {
//!     <ConfigEditor
//!         language=Language::YAML
//!         value=saved
//!         on_change=move |text| saved.set(text)
//!         schema=include_str!("schema.json")
//!     />
//! }
//! ```
//!
//! - `value`/`on_change` keep the host as the source of truth; `baseline`
//!   adds a diff gutter; `read_only` gives a view mode.
//! - `edit_as` edits a document in another language than the host stores it
//!   in: `language=Language::XML edit_as=Language::HCL` shows HCL and sends
//!   XML back through `on_change`.
//! - `handle` takes an [`EditorHandle`]: where a [`FieldPath`] is written
//!   (`range_of`), which path a position is on (`path_at`), the caret's path
//!   (`current_path`, or the `on_path_change` callback) and "jump to field"
//!   (`reveal`). The same lookups work headless on [`Language`].
//! - `host_diagnostics` shows problems the host found (a server's
//!   validation, as [`HostDiagnostic`]s by path or range) next to the
//!   editor's own.
//! - [`ConfigDocuments`] shows several [`Document`]s (a main file and its
//!   drop-ins) as tabs, each with its own language, baseline and read-only
//!   state, one editor per document kept across switches.
//! - The toolbar and footer are opt-in; compose your own chrome with
//!   [`ConfigEditorToolbar`] and [`ConfigEditorFooter`]. Pick a bundled
//!   [`EditorTheme`], adjust any color of it with [`ThemeColors`], or set
//!   `styled=false` and style the semantic
//!   `config-editor__*` markup yourself ([`PRESET_CSS`] is the preset).
//!
//! # Headless use
//!
//! ```
//! # #[cfg(all(feature = "schema-validation", feature = "toml", feature = "yaml", feature = "hcl"))] {
//! use config_editor_leptos::{Language, Schema};
//!
//! let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
//! assert!(schema.validate(Language::TOML, "listen = 7835\n").is_empty());
//!
//! let hcl = Language::YAML.convert("listen: 7835\n", Language::HCL).unwrap();
//! assert_eq!(hcl.text, "listen = 7835\n");
//! # }
//! ```
//!
//! # Your own language
//!
//! Implement [`language::ConfigLanguage`] and wrap it with [`Language::new`];
//! see the [`language`] module.
//!
//! # Features
//!
//! All on by default:
//!
//! - `schema-validation`: live JSON Schema diagnostics, through the
//!   `jsonschema` crate (the heaviest dependency). Completions, hover docs and
//!   schema-shaped reading work without it, from the schema alone; leave it
//!   out when documents are validated elsewhere (on a server). Syntax is
//!   always checked.
//! - `yaml`, `json`, `toml`, `hcl`, `xml`: the built-in languages
//!   ([`Language::YAML`], …). Without any, use your own through
//!   [`Language::new`].

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

mod documents;
mod editor;

pub use documents::{CloseRequest, ConfigDocuments, Document, DocumentsHandle};
pub use editor::language;

pub use editor::{
    BaselineConflict, ChangeSummary, ConfigEditor, ConfigEditorFooter, ConfigEditorToolbar,
    Conversion, ConvertError, Diagnostic, DiagnosticOrigin, DiagnosticTarget, EditorHandle,
    EditorTheme, FieldPath, HostDiagnostic, Language, PRESET_CSS, ParseError, SaveRequest, Schema,
    SchemaError, SchemaSource, SecretOptions, Severity, TextPosition, TextRange, ThemeColor,
    ThemeColors, UnknownLanguage, UnknownTheme, check_syntax, flush_pending_edits,
    flush_pending_input, unified_diff,
};
