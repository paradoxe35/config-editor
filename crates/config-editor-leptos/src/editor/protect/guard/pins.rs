//! The values locks were pinned with, compared with the edited text.

use std::collections::HashSet;

use serde_json::Value;

use super::super::locks::Protected;
use super::super::texts::Reason;

/// The reason of the first field the language could not locate (it has no
/// span but its value) whose value in `document` differs from the one
/// pinned, if any.
pub(super) fn unplaced_hold<'a>(document: &Value, protected: &'a Protected) -> Option<&'a Reason> {
    protected
        .unplaced
        .iter()
        .find(|(pointer, _)| {
            protected
                .pins
                .iter()
                .find(|(pinned, _)| pinned == pointer)
                .is_some_and(|(_, value)| document.pointer(pointer) != Some(value))
        })
        .map(|(_, reason)| reason)
}

/// The reason of the first field the language could not locate whose
/// pinned value's text (a scalar as written: `7835`, a string's content)
/// `after`, which does not parse, holds fewer times whole than `before`
/// (see [`whole_copies`]): with no span, its value's text is its span until
/// the text parses again.
pub(super) fn unplaced_text_cut<'a>(
    protected: &'a Protected,
    before: &str,
    after: &str,
) -> Option<&'a Reason> {
    protected
        .unplaced
        .iter()
        .find(|(pointer, _)| {
            let text = protected
                .pins
                .iter()
                .find(|(pinned, _)| pinned == pointer)
                .and_then(|(_, value)| match value {
                    Value::String(text) => Some(text.clone()),
                    Value::Number(number) => Some(number.to_string()),
                    Value::Bool(flag) => Some(flag.to_string()),
                    _ => None,
                })
                .filter(|text| !text.is_empty());
            text.is_some_and(|text| whole_copies(after, &text) < whole_copies(before, &text))
        })
        .map(|(_, reason)| reason)
}

/// How many times `text` holds `value` with nothing glued onto it (no
/// letter, digit or `_` right before or after it: `783599` holds no
/// `7835`).
fn whole_copies(text: &str, value: &str) -> usize {
    let glues =
        |character: Option<char>| character.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(value)
        .filter(|(at, _)| {
            !glues(text[..*at].chars().next_back())
                && !glues(text[at + value.len()..].chars().next())
        })
        .count()
}

/// Whether every locked field has a value pinned to compare with (one
/// found in a text that never parsed has none: whitespace inside it is not
/// known to be free there).
pub(super) fn all_pinned(protected: &Protected) -> bool {
    let pointers: HashSet<&str> = protected
        .pins
        .iter()
        .map(|(pointer, _)| pointer.as_str())
        .collect();
    protected.locks.iter().all(|lock| {
        lock.pointer
            .as_deref()
            .is_none_or(|pointer| pointers.contains(pointer))
    })
}
