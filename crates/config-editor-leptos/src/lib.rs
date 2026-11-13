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
//! - `host_diagnostics` shows problems the host found (its own
//!   checks: a service, a build step, as [`HostDiagnostic`]s by path or range) next to the
//!   editor's own.
//! - [`ConfigDocuments`] shows several [`Document`]s (a main file and its
//!   drop-ins) as tabs, each with its own language, baseline and read-only
//!   state, one editor per document kept across switches.
//! - `settings` takes [`EditorSettings`]: the theme and every on/off switch
//!   (toolbar, footer, diagnostics, diff, completions, search, folding, …),
//!   as a value or a signal that switches them live.
//! - The toolbar and footer are opt-in; compose your own chrome with
//!   [`ConfigEditorToolbar`] and [`ConfigEditorFooter`]. Pick a bundled
//!   [`EditorTheme`], adjust any color of it with [`ThemeColors`], or turn
//!   the `styled` setting off and style the semantic `config-editor__*`
//!   markup yourself ([`PRESET_CSS`] is the preset).
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
//! # Custom languages
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
//!   out when documents are validated elsewhere (by the host's own checks). Syntax is
//!   always checked.
//! - `yaml`, `json`, `toml`, `hcl`, `xml`: the built-in languages
//!   ([`Language::YAML`], …). Without any, use your own through
//!   [`Language::new`].
//! - `search`: find and replace, go to line.
//! - `diff`: changes against the baseline (gutter markers, preview, revert,
//!   the change count, `unified_diff`).
//! - `folding`: folding by structure, and the handle's folding methods.
//! - `protect`: read-only fields and secrets.
//!
//! Without a feature, what it adds is gone, except what keeps a host's code
//! the same in every build: the `ConfigEditor` props and the
//! [`EditorSettings`] fields stay and do nothing (each names its feature),
//! as do the [`language::ConfigLanguage`] hooks. The README's "Cargo
//! features" section lists the sizes.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

mod cssom;
mod documents;
mod editor;
mod focus;
mod ime;
mod live_region;
mod page_scroll;
mod preset_style;
mod reactive;
mod settings;
mod strings;

pub use documents::{CloseRequest, ConfigDocuments, Document, DocumentsHandle};
pub use editor::language;
pub use settings::EditorSettings;
pub use strings::{EditorStrings, StringsProblem};

#[cfg(feature = "diff")]
pub use editor::unified_diff;
pub use editor::{
    BaselineConflict, ChangeSummary, CompiledSchema, ConfigEditor, ConfigEditorFooter,
    ConfigEditorToolbar, Conversion, ConvertError, Diagnostic, DiagnosticOrigin, DiagnosticTarget,
    EditorHandle, EditorTheme, FieldPath, HostDiagnostic, HoverDocs, Language, LossEffect,
    PRESET_CSS, ParseError, SaveRequest, Schema, SchemaCompiler, SchemaError, SchemaFormat,
    SchemaFormatInfo, SchemaInput, SchemaLoss, SchemaResources, SchemaSource, SecretOptions,
    Severity, Suggestion, TextPosition, TextRange, ThemeColor, ThemeColors, UnknownLanguage,
    UnknownSchemaFormat, UnknownTheme, check_syntax, float_of, float_value, flush_pending_edits,
    string_of, string_value,
};

/// What the browser bindings (`config-editor-js`, published to npm as
/// `@pngwasi/config-editor`) need from the editor that is not part of its
/// Rust API: no stability promise, and not for other crates.
#[doc(hidden)]
pub mod js_support {
    pub use crate::editor::{flush_pending_input, is_web_url};

    /// A timer [`after`] set, to [`cancel`](Self::cancel) it.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Timer(crate::reactive::later::Timeout);

    impl Timer {
        /// Drops the timer's work if it has not run yet.
        pub fn cancel(self) {
            self.0.cancel();
        }
    }

    /// Runs `run(target)` after `ms` milliseconds, if `target` (a signal or
    /// stored value of the bindings') is still alive: the bindings'
    /// deferred work goes through the editor's one door for it.
    pub fn after<T: leptos::prelude::IsDisposed + Copy + 'static>(
        target: T,
        ms: i32,
        run: impl FnOnce(T) + 'static,
    ) -> Timer {
        Timer(crate::reactive::later::after(target, ms, run))
    }

    /// Whether the secret placeholder pattern of `options` can be used: it
    /// compiles, and it matches no empty text (`.*` would make every value
    /// a placeholder). The error says what is wrong with it.
    ///
    /// ```
    /// use config_editor_leptos::SecretOptions;
    /// use config_editor_leptos::js_support::check_placeholder;
    /// assert!(check_placeholder(&SecretOptions::default()).is_ok());
    /// assert!(check_placeholder(&SecretOptions::default().with_placeholder("(")).is_err());
    /// assert!(check_placeholder(&SecretOptions::default().with_placeholder(".*")).is_err());
    /// assert!(config_editor_leptos::js_support::is_web_url("https://example.com/docs"));
    /// assert!(!config_editor_leptos::js_support::is_web_url("javascript:alert(1)"));
    /// assert!(!config_editor_leptos::js_support::is_web_url("http://"));
    /// ```
    pub fn check_placeholder(options: &crate::SecretOptions) -> Result<(), String> {
        options.check_placeholder()
    }

    /// `handle.range_of(path)`, with the UTF-16 offsets in the text on
    /// screen its start and end are at (what a JS range's `start` and `end`
    /// are), read from the spans kept for the text, not from the top.
    pub fn range_and_offsets_of(
        handle: &crate::EditorHandle,
        path: impl Into<crate::FieldPath>,
    ) -> Option<(crate::TextRange, usize, usize)> {
        handle.range_and_offsets_of(path)
    }

    /// `handle.path_at` the UTF-16 offset `offset` of the text on screen
    /// (what a JS string index is), read from what is kept for the text:
    /// no copy of the text, and no read of it up to the offset, per call.
    pub fn path_at_offset(handle: &crate::EditorHandle, offset: usize) -> Option<crate::FieldPath> {
        handle.path_at_offset(offset)
    }

    /// Makes the view of `handle` take `documents` (the list just given to
    /// its `documents` signal) now, not once its effects run, so the
    /// active id and every query answer for the new list at once.
    pub fn take_documents(handle: &crate::DocumentsHandle, documents: &[crate::Document]) {
        handle.take_documents(documents);
    }

    /// The language the document `id` of the view of `handle` is edited
    /// as, as its list or the handle's `edit_as` asked last (`None` inside:
    /// its own); `None` when it is not open.
    pub fn edit_as(handle: &crate::DocumentsHandle, id: &str) -> Option<Option<crate::Language>> {
        handle.edit_as_of(id)
    }
}
