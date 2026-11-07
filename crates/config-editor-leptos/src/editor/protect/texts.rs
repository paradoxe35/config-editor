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

/// What Replace All adds when it left matches in protected text alone.
pub(crate) fn skipped_matches(count: usize) -> String {
    format!("; skipped {count} in read-only text")
}

/// What Replace All says when every match is in read-only text.
pub(crate) const NOTHING_REPLACED: &str = "Every match is in read-only text; nothing was replaced";
