//! A live [`EditorState`] for native unit tests: the signals and memos
//! work without a browser (no textarea is mounted, so the parts that need
//! one skip their work), which lets tests drive whole edits through the
//! editor's own code.

use leptos::prelude::*;

use super::languages::Language;
use super::paths::{FieldPath, TextRange};
use super::protect::ProtectInit;
use super::state::{EditorState, Init};
use crate::EditorSettings;

/// What a test editor is mounted with.
pub(super) struct TestEditor {
    pub(super) language: Language,
    pub(super) edit_as: Option<Language>,
    pub(super) text: String,
    pub(super) read_only_paths: Vec<FieldPath>,
    pub(super) read_only_ranges: Vec<TextRange>,
    pub(super) on_change: Option<Callback<String>>,
    pub(super) on_save: Option<Callback<super::host_editing::SaveRequest>>,
    pub(super) read_only: bool,
    /// The `secrets` prop.
    pub(super) secrets: Option<super::secret_options::SecretOptions>,
    /// The `host_diagnostics` prop.
    pub(super) host_diagnostics: Signal<Vec<super::host_diagnostics::HostDiagnostic>>,
    /// The `settings` prop; a signal the test may switch.
    pub(super) settings: Signal<EditorSettings>,
    /// A text in the `edit_as` language to open with, as a documents
    /// view's `Document::edit_as_text`.
    pub(super) opening: Option<String>,
    /// The `read_only_reason` prop.
    pub(super) read_only_reason: Option<String>,
}

impl TestEditor {
    pub(super) fn new(language: Language, text: &str) -> Self {
        Self {
            language,
            edit_as: None,
            text: text.to_string(),
            read_only_paths: Vec::new(),
            read_only_ranges: Vec::new(),
            on_change: None,
            on_save: None,
            read_only: false,
            secrets: None,
            host_diagnostics: Signal::default(),
            settings: Signal::default(),
            opening: None,
            read_only_reason: None,
        }
    }

    /// Mounts it under a fresh owner, which the caller keeps alive for as
    /// long as it uses the state.
    pub(super) fn mount(self) -> (Owner, EditorState) {
        let owner = Owner::new();
        owner.set();
        let (state, _) = EditorState::new(Init {
            host_language: self.language,
            edit_language: self.edit_as.unwrap_or(self.language),
            schema: None,
            schema_error: None,
            host_text: self.text.clone(),
            baseline: self.text,
            opening: self.opening,
            read_only: Signal::from(self.read_only),
            read_only_reason: Signal::stored(self.read_only_reason),
            settings: self.settings,
            host_on_change: self.on_change,
            host_diagnostics: self.host_diagnostics,
            protect: ProtectInit {
                paths: Some(Signal::stored(self.read_only_paths)),
                ranges: Some(Signal::stored(self.read_only_ranges)),
                secrets: self.secrets,
            },
            on_save: self.on_save,
            on_baseline_conflict: None,
        });
        (owner, state)
    }
}
