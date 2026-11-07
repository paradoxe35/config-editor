//! What the host describes for each open document, and what the component
//! asks of the host.

use crate::{Language, SchemaSource};

/// One open document of a [`ConfigDocuments`](crate::ConfigDocuments): a
/// file with its own language, baseline, read-only state and schema.
///
/// The host owns the list. It adds, removes, renames and reorders documents
/// by changing it, and keeps `value` up to date from `on_change`.
///
/// ```
/// # #[cfg(all(feature = "yaml", feature = "toml"))] {
/// use config_editor_leptos::{Document, Language};
///
/// let main = Document::new("main", "burrow.yaml", "listen: 7835\n");
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
    /// The document's JSON Schema. Documents given the same JSON text share
    /// one compiled schema; a [`Schema`](crate::Schema) passed as
    /// `Compiled` is shared as it is.
    pub schema: Option<SchemaSource>,
    /// Edit in another language than `language`; switches live.
    pub edit_as: Option<Language>,
}

impl Document {
    /// A document with an id, a file name and its text; the rest default
    /// (language from the file name, baseline the text, editable, no
    /// schema).
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
            schema: None,
            edit_as: None,
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

    /// Sets the schema: JSON text or a compiled [`Schema`](crate::Schema).
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

    /// The language the document is in: `language`, else the one of
    /// `file_name`'s extension, else YAML.
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

#[cfg(test)]
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
