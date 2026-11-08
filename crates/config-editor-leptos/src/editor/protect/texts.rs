//! How protected fields are worded, from the `strings` setting, and the
//! defaults of secret placeholders that are patterns, not texts.

use crate::EditorStrings;
use crate::strings::fill;

/// Why a field or a stretch of text is read-only.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Reason {
    /// The schema's `x-readonly-reason`.
    Given(String),
    /// A read-only field that says no more (the `readOnlyField` text).
    Field,
    /// A read-only range the host gave (the `readOnlyText` text).
    Range,
}

impl Reason {
    /// The reason as shown.
    pub(crate) fn text(&self, strings: &EditorStrings) -> String {
        match self {
            Reason::Given(reason) => reason.clone(),
            Reason::Field => strings.read_only_field.clone(),
            Reason::Range => strings.read_only_text.clone(),
        }
    }

    /// Why renaming or moving the key above a field locked for this reason
    /// is refused.
    pub(crate) fn holding(&self, strings: &EditorStrings) -> String {
        fill(&strings.holds_read_only, &[("reason", &self.text(strings))])
    }

    /// Why an edit making the text parse is refused: the field at
    /// `pointer`, locked for this reason, would read otherwise than in the
    /// host's text.
    pub(crate) fn changed(&self, strings: &EditorStrings, pointer: &str) -> String {
        let place = if pointer.is_empty() { "/" } else { pointer };
        fill(
            &strings.read_only_changed,
            &[("pointer", &place), ("reason", &self.text(strings))],
        )
    }

    /// Why an edit making the text parse is refused when the lines removed
    /// that may hold the field at `pointer` are too many to read.
    pub(crate) fn unchecked(&self, strings: &EditorStrings, pointer: Option<&str>) -> String {
        let place = match pointer {
            Some("") | None => "/",
            Some(pointer) => pointer,
        };
        fill(
            &strings.read_only_unchecked,
            &[("pointer", &place), ("reason", &self.text(strings))],
        )
    }

    /// "Read-only: …", the gutter lock's label.
    pub(crate) fn label(&self, strings: &EditorStrings) -> String {
        labelled(strings, &strings.read_only_label, &self.text(strings))
    }
}

/// "Secret placeholder: …", the gutter key's label.
pub(crate) fn secret_label(strings: &EditorStrings, hint: &str) -> String {
    labelled(strings, &strings.secret_label, hint)
}

fn labelled(strings: &EditorStrings, label: &str, message: &str) -> String {
    fill(
        &strings.protected_note,
        &[("label", &label), ("message", &message)],
    )
}

/// A save refused because it would change the read-only value at
/// `pointer`.
pub(crate) fn save_refused(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.save_refused, &[("pointer", &place)])
}

/// A save waiting for a text that parses: the read-only value at
/// `pointer` cannot be read in it to be checked.
pub(crate) fn save_waits(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.save_waits, &[("pointer", &place)])
}

/// A placeholder the host did not give, in a secret field.
pub(crate) fn unknown_placeholder(strings: &EditorStrings, pointer: &str) -> String {
    fill(&strings.unknown_placeholder, &[("pointer", &pointer)])
}

/// One of the host's placeholders outside the field it stands in.
pub(crate) fn moved_placeholder(strings: &EditorStrings, pointer: &str, owner: &str) -> String {
    fill(
        &strings.moved_placeholder,
        &[("pointer", &pointer), ("owner", &owner)],
    )
}

/// What a reference suggestion is, in its detail.
pub(crate) fn reference_detail(strings: &EditorStrings, prefix: &str) -> String {
    match prefix {
        "${" => strings.env_reference.clone(),
        "file:" => strings.file_reference.clone(),
        other => fill(&strings.other_reference, &[("prefix", &other)]),
    }
}

/// The label of a reference suggestion: the prefix with a stand-in for
/// what follows it (`${VAR}`, `file:path`), as code is written.
pub(crate) fn reference_label(prefix: &str, closer: &str) -> String {
    let stand_in = match prefix {
        "${" => "VAR",
        "file:" => "path",
        _ => "…",
    };
    format!("{prefix}{stand_in}{closer}")
}
