//! The problems Edit-as reports, kept as what happened and worded from the
//! `strings` setting each time they are shown, so new texts reword them.

use super::super::codec::ConvertError;
use super::super::diagnostics::Diagnostic;
use super::super::languages::Language;
use super::super::protect::ReadOnlyChange;
use crate::EditorStrings;
use crate::strings::fill;

/// Why an edit cannot be saved in the host's language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum SaveError {
    /// The conversion failed (the host's language cannot express the
    /// document).
    Convert(ConvertError),
    /// A `null` at `pointer` the host's language would read back as
    /// something else (XML writes it as empty text).
    NoNull { pointer: String },
    /// An infinity or NaN at `pointer` the host's language has no number
    /// for (JSON, HCL and XML would write it as text).
    NoNonFinite { pointer: String },
    /// The save would change a read-only value.
    ReadOnly(ReadOnlyChange),
}

impl From<ConvertError> for SaveError {
    fn from(error: ConvertError) -> Self {
        SaveError::Convert(error)
    }
}

impl SaveError {
    /// The JSON pointer of the value it is about, when it names one (a
    /// conversion error's `… (at /pointer)`).
    pub(in crate::editor) fn pointer(&self) -> String {
        match self {
            SaveError::Convert(error) => {
                let text = error.to_string();
                text.rsplit_once("(at ")
                    .and_then(|(_, rest)| rest.strip_suffix(')'))
                    .unwrap_or("")
                    .to_string()
            }
            SaveError::NoNull { pointer } | SaveError::NoNonFinite { pointer } => pointer.clone(),
            SaveError::ReadOnly(change) => change.pointer.clone(),
        }
    }

    /// What went wrong, for a `host` document.
    pub(in crate::editor) fn text(&self, strings: &EditorStrings, host: Language) -> String {
        match self {
            SaveError::Convert(error) => error.to_string(),
            SaveError::NoNull { pointer } => fill(
                &strings.no_null,
                &[("host", &host.label()), ("pointer", pointer)],
            ),
            SaveError::NoNonFinite { pointer } => fill(
                &strings.no_non_finite,
                &[("host", &host.label()), ("pointer", pointer)],
            ),
            SaveError::ReadOnly(change) => {
                super::super::protect::save_refused(strings, &change.pointer)
            }
        }
    }
}

/// What an Edit-as problem says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum EditAsText {
    /// The host's language reads the value at `pointer` back differently.
    HostCannotKeep { pointer: String, host: Language },
    /// The edited language cannot show the value at `pointer` exactly.
    EditCannotShow { pointer: String, edit: Language },
    /// The edit is not saved.
    NotSaved { host: Language, error: SaveError },
    /// The host's document does not convert for editing.
    CannotEditAs {
        host: Language,
        edit: Language,
        error: ConvertError,
    },
    /// A switch to `edit` waits until the document converts.
    CannotSwitch {
        edit: Language,
        host: Language,
        error: SaveError,
    },
}

impl EditAsText {
    fn text(&self, strings: &EditorStrings) -> String {
        match self {
            EditAsText::HostCannotKeep { pointer, host } => fill(
                &strings.host_cannot_keep,
                &[("pointer", pointer), ("host", &host.label())],
            ),
            EditAsText::EditCannotShow { pointer, edit } => fill(
                &strings.edit_cannot_show,
                &[("pointer", pointer), ("edit", &edit.label())],
            ),
            EditAsText::NotSaved { host, error } => fill(
                &strings.not_saved,
                &[
                    ("host", &host.label()),
                    ("error", &error.text(strings, *host)),
                ],
            ),
            EditAsText::CannotEditAs { host, edit, error } => fill(
                &strings.cannot_edit_as,
                &[
                    ("host", &host.label()),
                    ("edit", &edit.label()),
                    ("error", error),
                ],
            ),
            EditAsText::CannotSwitch { edit, host, error } => fill(
                &strings.cannot_switch,
                &[
                    ("edit", &edit.label()),
                    ("error", &error.text(strings, *host)),
                ],
            ),
        }
    }
}

/// An Edit-as problem: where it is (a [`Diagnostic`] without its message)
/// and what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) struct EditAsProblem {
    pub(in crate::editor) at: Diagnostic,
    pub(in crate::editor) text: EditAsText,
}

impl EditAsProblem {
    /// The problem as shown, worded by `strings`.
    pub(in crate::editor) fn worded(&self, strings: &EditorStrings) -> Diagnostic {
        Diagnostic {
            message: self.text.text(strings),
            ..self.at.clone()
        }
    }
}
