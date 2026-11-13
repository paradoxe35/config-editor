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
    /// A value at `pointer` the host's language would read back otherwise
    /// (XML has no empty or one-item list without a schema saying so).
    CannotKeep { pointer: String },
    /// The save would change a read-only value.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "nothing is protected without the feature")
    )]
    ReadOnly(ReadOnlyChange),
    /// The save would write text beside the placeholder the host gave as
    /// the whole value at `pointer`.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "nothing is protected without the feature")
    )]
    GluedPlaceholder { pointer: String },
    /// The save would write the placeholder of the field `owner` only at
    /// `pointer`, where it is plain text (the field's element copied into
    /// a list of copies): the secret would be gone.
    #[cfg_attr(
        not(feature = "protect"),
        allow(dead_code, reason = "nothing is protected without the feature")
    )]
    MovedPlaceholder { pointer: String, owner: String },
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
            SaveError::Convert(ConvertError::MixedContent { pointer }) => pointer.clone(),
            SaveError::Convert(error) => {
                let text = error.to_string();
                text.rsplit_once("(at ")
                    .and_then(|(_, rest)| rest.strip_suffix(')'))
                    .unwrap_or("")
                    .to_string()
            }
            SaveError::NoNull { pointer }
            | SaveError::NoNonFinite { pointer }
            | SaveError::CannotKeep { pointer }
            | SaveError::GluedPlaceholder { pointer }
            | SaveError::MovedPlaceholder { pointer, .. } => pointer.clone(),
            SaveError::ReadOnly(change) => change.pointer.clone(),
        }
    }

    /// What went wrong, for a `host` document.
    pub(in crate::editor) fn text(&self, strings: &EditorStrings, host: Language) -> String {
        match self {
            SaveError::Convert(error) => convert_text(error, strings),
            SaveError::NoNull { pointer } => fill(
                &strings.no_null,
                &[("host", &host.label()), ("pointer", pointer)],
            ),
            SaveError::NoNonFinite { pointer } => fill(
                &strings.no_non_finite,
                &[("host", &host.label()), ("pointer", pointer)],
            ),
            SaveError::CannotKeep { pointer } => fill(
                &strings.host_cannot_keep,
                &[("pointer", pointer), ("host", &host.label())],
            ),
            SaveError::ReadOnly(change) => {
                super::super::protect::save_refused(strings, &change.pointer)
            }
            SaveError::GluedPlaceholder { pointer } => {
                super::super::protect::glued_message(strings, pointer)
            }
            SaveError::MovedPlaceholder { pointer, owner } => {
                super::super::protect::moved_placeholder(strings, pointer, owner)
            }
        }
    }
}

/// What a conversion error says, worded by `strings` where it has a text.
fn convert_text(error: &ConvertError, strings: &EditorStrings) -> String {
    match error {
        ConvertError::MixedContent { pointer } => {
            fill(&strings.mixed_content, &[("pointer", pointer)])
        }
        other => other.to_string(),
    }
}

/// What an Edit-as problem says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum EditAsText {
    /// The edited language cannot show the value at `pointer` exactly.
    EditCannotShow { pointer: String, edit: Language },
    /// The edit is not saved.
    NotSaved { host: Language, error: SaveError },
    /// A switch to `edit` waits until the document converts.
    CannotSwitch {
        edit: Language,
        host: Language,
        error: SaveError,
    },
    /// A switch refused for good: one of the languages cannot write
    /// documents (`error` says which).
    SwitchRefused { edit: Language, error: String },
}

impl EditAsText {
    fn text(&self, strings: &EditorStrings) -> String {
        match self {
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
            EditAsText::CannotSwitch { edit, host, error } => fill(
                &strings.cannot_switch,
                &[
                    ("edit", &edit.label()),
                    ("error", &error.text(strings, *host)),
                ],
            ),
            EditAsText::SwitchRefused { edit, error } => fill(
                &strings.switch_refused,
                &[("edit", &edit.label()), ("error", error)],
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
