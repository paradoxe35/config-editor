//! A reusable YAML editor component for Leptos applications.
//!
//! The editor is an input component, not a page or application shell. Its
//! toolbar and footer are omitted by default and can be composed with the
//! exported [`YamlEditorToolbar`] and [`YamlEditorFooter`] components.
//! Diagnostics, completions, highlighting, and preset styling can be enabled
//! independently. The optional JSON Schema string enables validation and
//! schema-aware suggestions. Suggestions appear while typing by default; set
//! `auto_trigger_completions=false` to require `Ctrl+Space`. Set
//! `read_only=true` for a view mode that keeps highlighting, diagnostics,
//! diffs, and hover docs while disabling all edits.
//!
//! ```ignore
//! use leptos_config_editor::YamlEditor;
//!
//! view! {
//!     <YamlEditor
//!         initial_value="name: app\n".to_string()
//!         schema_json=r#"{"type":"object","properties":{"name":{"type":"string"}}}"#.to_string()
//!         class="settings-editor"
//!     />
//! }
//! ```
//!
//! Set `styled=false` to use the semantic editor markup without preset styles,
//! then provide your own CSS. The preset stylesheet is also available as
//! [`PRESET_CSS`].

mod editor;

pub use editor::{EditorTheme, PRESET_CSS, YamlEditor, YamlEditorFooter, YamlEditorToolbar};
