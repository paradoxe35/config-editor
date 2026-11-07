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

/// What Replace All adds when it left `count` matches in protected text
/// alone.
pub(crate) fn skipped_matches(strings: &EditorStrings, count: usize) -> String {
    fill(&strings.replace_skipped, &[("count", &count)])
}

/// The default placeholder pattern.
pub(crate) const SECRET_PATTERN: &str = "«secret [0-9a-f]+»";

/// The default reference forms offered in a secret field.
pub(crate) const REFERENCE_PREFIXES: [&str; 2] = ["${", "file:"];

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
