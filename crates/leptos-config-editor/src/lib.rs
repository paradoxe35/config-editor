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
//! use leptos_config_editor::{ConfigEditor, Language};
//!
//! let saved = RwSignal::new("name: app\n".to_string());
//! view! {
//!     <ConfigEditor
//!         language=Language::Yaml
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
//!   in: `language=Language::Xml edit_as=Language::Hcl` shows HCL and sends
//!   XML back through `on_change`.
//! - The toolbar and footer are opt-in; compose your own chrome with
//!   [`ConfigEditorToolbar`] and [`ConfigEditorFooter`]. Pick a bundled
//!   [`EditorTheme`], or set `styled=false` and style the semantic
//!   `config-editor__*` markup yourself ([`PRESET_CSS`] is the preset).
//!
//! # Headless use
//!
//! ```
//! # #[cfg(feature = "schema-validation")] {
//! use leptos_config_editor::{Language, Schema};
//!
//! let schema = Schema::compile(r#"{"properties":{"listen":{"type":"integer"}}}"#).unwrap();
//! assert!(schema.validate(Language::Toml, "listen = 7835\n").is_empty());
//!
//! let hcl = Language::Yaml.convert("listen: 7835\n", Language::Hcl).unwrap();
//! assert_eq!(hcl.text, "listen = 7835\n");
//! # }
//! ```
//!
//! # Features
//!
//! - `schema-validation` (default): JSON Schema diagnostics, completions, and
//!   hover docs, through the `jsonschema` crate. Without it the editor still
//!   checks syntax and converts.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

mod editor;

pub use editor::{
    ConfigEditor, ConfigEditorFooter, ConfigEditorToolbar, Conversion, ConvertError, Diagnostic,
    EditorTheme, Language, PRESET_CSS, ParseError, Schema, SchemaError, SchemaSource, Severity,
    UnknownLanguage, UnknownTheme, check_syntax,
};
