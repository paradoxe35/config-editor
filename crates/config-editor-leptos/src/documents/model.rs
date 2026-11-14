//! What the host describes for each open document, and what the component
//! asks of the host.

use crate::{
    EditorSettings, FieldPath, HostDiagnostic, Language, SchemaSource, SecretOptions, TextRange,
};

/// One open document of a [`ConfigDocuments`](crate::ConfigDocuments): a
/// file with its own language, baseline, read-only state and schema, and
/// optionally its own protected fields, host problems and settings.
///
/// The host owns the list. It adds, removes, renames and reorders documents
/// by changing it, and keeps `value` up to date from `on_change`.
///
/// ```
/// # #[cfg(all(feature = "yaml", feature = "toml"))] {
/// use config_editor_leptos::{Document, Language};
///
/// let main = Document::new("main", "config.yaml", "listen: 7835\n");
/// let drop_in = Document::new("tls", "conf.d/tls.toml", "cert = \"a.pem\"\n").read_only(true);
/// assert_eq!(main.resolved_language(), Language::YAML);
/// assert_eq!(drop_in.resolved_language(), Language::TOML);
/// # }
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Document {
    /// Identifies the document across updates of the list, and in every
    /// callback. Unique within the list.
    pub id: String,
    /// The tab's label (a file name or path).
    pub file_name: String,
    /// The document's language. Unset, it comes from `file_name`'s
    /// extension ([`Language::from_path`]), else YAML.
    pub language: Option<Language>,
    /// The text, in the document's language. Changing it updates the editor
    /// (keeping the caret), as the `value` of a `ConfigEditor` does.
    pub value: String,
    /// The text to diff against and to tell unsaved edits by: the version
    /// last saved or loaded. Unset, it is `value` as the document was added.
    pub baseline: Option<String>,
    /// View mode for this document; switches live.
    pub read_only: bool,
    /// Why the document is read-only (a file the host manages, a missing
    /// permission), while `read_only` is on: its tab's tooltip and
    /// accessible description say it, and so does the
    /// "Cannot edit in a read-only editor" notice when the user tries to
    /// edit (and the text area's accessible description). Updates live.
    pub read_only_reason: Option<String>,
    /// Whether the user may close it: its close button, a middle click and
    /// `Delete` on its tab ask `on_close`. Default `true`; a document that
    /// must stay open (the main config) sets `false`, and its tab has no
    /// close button. Without `on_close` no tab has one. Switches live.
    pub closable: bool,
    /// The document's JSON Schema. Documents given the same text share
    /// one compiled schema; a [`Schema`](crate::Schema) passed as
    /// `Compiled` is shared as it is.
    pub schema: Option<SchemaSource>,
    /// Edit in another language than `language`; switches live. Asking
    /// for the same language again (after an undo stepped back across the
    /// switch) is [`DocumentsHandle::edit_as`](crate::DocumentsHandle::edit_as):
    /// a list with an unchanged `edit_as` changes nothing.
    pub edit_as: Option<Language>,
    /// The text on screen when the document's editor opens, in the
    /// `edit_as` language, in place of `value` converted: what the user had
    /// on screen in an earlier session (the host kept it from
    /// [`EditorHandle::text`](crate::EditorHandle::text) before the view
    /// unmounted). `value` and `baseline` stay the host's; the text converts
    /// back like an edit, so `on_change` hears it once it converts and,
    /// until then, the document is out of sync (`on_sync_change` says
    /// `false`, [`EditorHandle::value`](crate::EditorHandle::value) is
    /// `None`). Undo starts at it. Read when the editor mounts (the first
    /// time the document is shown); ignored without `edit_as`.
    pub edit_as_text: Option<String>,
    /// Settings for this document's editor, in place of the view's; switch
    /// live. Unset, it follows the view's `settings`. The view's `styled`
    /// applies to every editor.
    pub settings: Option<EditorSettings>,
    /// Problems the host found in this document, as the editor's
    /// `host_diagnostics`; a new list replaces the markers.
    pub host_diagnostics: Vec<HostDiagnostic>,
    /// Fields of this document the user cannot change, as the editor's
    /// `read_only_paths`; update live.
    pub read_only_paths: Vec<FieldPath>,
    /// Stretches of this document's text the user cannot change, as the
    /// editor's `read_only_ranges`; update live.
    pub read_only_ranges: Vec<TextRange>,
    /// Secret placeholders, as the editor's `secrets`. Read when the editor
    /// mounts (the first time the document is shown).
    pub secrets: Option<SecretOptions>,
}

impl Document {
    /// A document with an id, a file name and its text; the rest default
    /// (language from the file name, baseline the text, editable, closable,
    /// no schema).
    pub fn new(
        id: impl Into<String>,
        file_name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            file_name: file_name.into(),
            language: None,
            value: value.into(),
            baseline: None,
            read_only: false,
            read_only_reason: None,
            closable: true,
            schema: None,
            edit_as: None,
            edit_as_text: None,
            settings: None,
            host_diagnostics: Vec::new(),
            read_only_paths: Vec::new(),
            read_only_ranges: Vec::new(),
            secrets: None,
        }
    }

    /// Sets the language, instead of the file name's.
    #[must_use]
    pub fn language(mut self, language: Language) -> Self {
        self.language = Some(language);
        self
    }

    /// Sets the baseline.
    #[must_use]
    pub fn baseline(mut self, baseline: impl Into<String>) -> Self {
        self.baseline = Some(baseline.into());
        self
    }

    /// Sets view mode.
    #[must_use]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Sets view mode, saying why (see
    /// [`read_only_reason`](Self::read_only_reason)).
    #[must_use]
    pub fn read_only_reason(mut self, reason: impl Into<String>) -> Self {
        self.read_only = true;
        self.read_only_reason = Some(reason.into());
        self
    }

    /// Sets whether the user may close the document (default `true`).
    #[must_use]
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// Sets the schema: JSON or YAML text, or a compiled [`Schema`](crate::Schema).
    #[must_use]
    pub fn schema(mut self, schema: impl Into<SchemaSource>) -> Self {
        self.schema = Some(schema.into());
        self
    }

    /// Edits the document as another language.
    #[must_use]
    pub fn edit_as(mut self, language: Language) -> Self {
        self.edit_as = Some(language);
        self
    }

    /// Edits the document as `language`, opening with `text` on screen
    /// (see [`edit_as_text`](Self::edit_as_text)).
    #[must_use]
    pub fn edit_as_text(mut self, language: Language, text: impl Into<String>) -> Self {
        self.edit_as = Some(language);
        self.edit_as_text = Some(text.into());
        self
    }

    /// Gives the document's editor its own settings.
    #[must_use]
    pub fn settings(mut self, settings: EditorSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Sets the host's problems in the document.
    #[must_use]
    pub fn host_diagnostics(mut self, diagnostics: Vec<HostDiagnostic>) -> Self {
        self.host_diagnostics = diagnostics;
        self
    }

    /// Sets the fields the user cannot change (the `protect` feature).
    #[must_use]
    pub fn read_only_paths(mut self, paths: Vec<FieldPath>) -> Self {
        self.read_only_paths = paths;
        self
    }

    /// Sets the stretches of the text the user cannot change (the `protect`
    /// feature).
    #[must_use]
    pub fn read_only_ranges(mut self, ranges: Vec<TextRange>) -> Self {
        self.read_only_ranges = ranges;
        self
    }

    /// Sets how secret placeholders are matched, described and completed
    /// (the `protect` feature).
    #[must_use]
    pub fn secrets(mut self, secrets: SecretOptions) -> Self {
        self.secrets = Some(secrets);
        self
    }

    /// The language the document is in: `language`, else the one of
    /// `file_name`'s extension, else the first built-in language (YAML with
    /// the default features).
    ///
    /// # Panics
    ///
    /// When neither tells and no built-in language feature is enabled (as
    /// [`Language::default`]): give such documents a `language`.
    pub fn resolved_language(&self) -> Language {
        self.language
            .or_else(|| Language::from_path(&self.file_name))
            .unwrap_or_default()
    }
}

/// What `on_close` receives: the user asked to close a document (its close
/// button, a middle click, or `Delete` on its tab).
///
/// Nothing closes by itself: the host removes the document from its list,
/// after asking the user first when `dirty` says there are unsaved edits.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CloseRequest {
    /// The document's id.
    pub id: String,
    /// Whether it has unsaved edits (its text differs from its baseline).
    pub dirty: bool,
}

// The tests need a built-in language.
#[cfg(all(
    test,
    any(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    )
))]
mod tests {
    use super::*;

    #[test]
    fn a_document_s_language_comes_from_its_file_name_unless_given() {
        let failures: Vec<String> = Language::ALL
            .iter()
            .flat_map(|language| {
                language.extensions().iter().map(move |extension| {
                    let name = format!("conf.d/10-drop-in.{}", extension.to_uppercase());
                    (*language, Document::new("id", name, "").resolved_language())
                })
            })
            .filter(|(wanted, got)| wanted != got)
            .map(|(wanted, got)| format!("{wanted} file read as {got}"))
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
        for language in Language::ALL {
            let given = Document::new("id", "notes.unknown", "").language(*language);
            assert_eq!(given.resolved_language(), *language);
        }
        assert_eq!(
            Document::new("id", "README", "").resolved_language(),
            Language::default()
        );
    }
}
