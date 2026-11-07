//! The built-in texts of protected fields, in one place.

/// Why a field is read-only when neither the schema (`x-readonly-reason`)
/// nor the host says.
pub(crate) const DEFAULT_REASON: &str = "This field is read-only";

/// Why text in a read-only range the host gave cannot change.
pub(crate) const RANGE_REASON: &str = "This text is read-only";

/// Why renaming or moving the key above a read-only field is refused.
pub(crate) fn holds_locked(reason: &str) -> String {
    format!("Holds a read-only field: {reason}")
}

/// A save refused because it would change a read-only value.
pub(crate) fn save_refused(pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    format!("{place} is read-only and this change would alter it")
}

/// The gutter lock's label, before the reason.
pub(crate) const LOCK_LABEL: &str = "Read-only";

/// The gutter key's label.
pub(crate) const SECRET_LABEL: &str = "Secret placeholder";

/// What a schema message says instead of a secret field's value.
#[cfg(feature = "schema-validation")]
pub(crate) const SECRET_VALUE: &str = "the secret value";

/// A placeholder the host did not give, in a secret field.
pub(crate) fn unknown_placeholder(pointer: &str) -> String {
    format!(
        "{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"
    )
}

/// What a reference suggestion is, in its detail.
pub(crate) fn reference_detail(prefix: &str) -> String {
    match prefix {
        "${" => "Read the secret from an environment variable".to_string(),
        "file:" => "Read the secret from a file".to_string(),
        other => format!("A reference starting with {other}"),
    }
}

/// The label of a reference suggestion: the prefix with a stand-in for
/// what follows it (`${VAR}`, `file:path`).
pub(crate) fn reference_label(prefix: &str, closer: &str) -> String {
    let stand_in = match prefix {
        "${" => "VAR",
        "file:" => "path",
        _ => "…",
    };
    format!("{prefix}{stand_in}{closer}")
}

/// The type column of a reference suggestion.
pub(crate) const REFERENCE_TYPE: &str = "reference";
