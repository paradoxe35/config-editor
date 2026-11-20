//! How protected fields are worded, from the `strings` setting, and the
//! defaults of secret placeholders that are patterns, not texts.

use super::locks::HoldCause;
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

/// A save waiting for a text that parses: the read-only value at
/// `pointer` cannot be read in it to be checked.
pub(crate) fn save_waits(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.save_waits, &[("pointer", &place)])
}

/// A save waiting while the read-only field at `pointer`, locked for
/// `reason`, is held for `cause` (see `locks::Protected::held`).
pub(crate) fn save_held(
    strings: &EditorStrings,
    pointer: &str,
    reason: &Reason,
    cause: HoldCause,
) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    let template = match cause {
        HoldCause::Duplicate => &strings.save_held_duplicate,
        HoldCause::Renamed => &strings.save_held_renamed,
        HoldCause::Moved => &strings.save_held_moved,
        HoldCause::ListItem => &strings.save_held_list_item,
        HoldCause::Commented => &strings.save_held_commented,
        HoldCause::Removed => &strings.save_held_removed,
        HoldCause::Added => &strings.save_held_added,
        HoldCause::Spanless => &strings.save_held_spanless,
        HoldCause::RangeMoved => &strings.save_held_range,
        HoldCause::ChangedBeforeLock => &strings.save_held_before_lock,
        HoldCause::Unclear => &strings.save_held,
    };
    // Each text says the field is read-only: only a reason the schema
    // gives says more.
    let why = match reason {
        Reason::Given(given) => fill(&strings.save_held_reason, &[("reason", given)]),
        Reason::Field | Reason::Range => String::new(),
    };
    fill(template, &[("pointer", &place), ("reason", &why)])
}

/// A save waiting while the secret field at `pointer` reads text beside a
/// placeholder the host gave as its whole value.
pub(crate) fn save_held_placeholder(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.save_held_placeholder, &[("pointer", &place)])
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

/// One of another document's placeholders (`source`: its file name).
pub(crate) fn foreign_placeholder(strings: &EditorStrings, source: &str) -> String {
    fill(&strings.foreign_placeholder, &[("source", &source)])
}

/// A copy of one of the host's placeholders in its own field, past those
/// the host wrote there.
pub(crate) fn extra_placeholder(strings: &EditorStrings, pointer: &str) -> String {
    fill(&strings.extra_placeholder, &[("pointer", &pointer)])
}

/// One of the host's field placeholders written in a comment.
pub(crate) fn placeholder_in_comment(strings: &EditorStrings, owner: &str) -> String {
    fill(&strings.placeholder_in_comment, &[("owner", &owner)])
}

/// A placeholder the host wrote in a comment, in a field.
pub(crate) fn comment_placeholder(strings: &EditorStrings, pointer: &str) -> String {
    fill(&strings.comment_placeholder, &[("pointer", &pointer)])
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

/// `pointer` as a message names it: each key holding a piece of one of
/// `tokens` (a placeholder a broken text cut in two, as an XML name ends
/// at its space: `<se«secret 3f2a»ret>` reads as `se«secret`) shown as
/// `…`, as a syntax message hides what it quotes of one. A key holding a
/// token whole and no piece is the user's own key, shown as written.
pub(crate) fn shown_pointer(pointer: &str, tokens: &[&str]) -> String {
    pointer
        .split('/')
        .map(|key| {
            if cuts_a_token(key, tokens) {
                "…"
            } else {
                key
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether `key` (a pointer's, escaped) ends with the start of one of
/// `tokens` or starts with its end, past the copies of it it holds whole.
/// A piece is one holding a placeholder's marks (`«`, `${`, `}}`): one of
/// letters and digits alone cannot be told from the key's own (a key
/// `pass` ends with the `s` a token `s3cr…` starts with).
fn cuts_a_token(key: &str, tokens: &[&str]) -> bool {
    let key = key.replace("~1", "/").replace("~0", "~");
    tokens
        .iter()
        .filter(|token| !token.is_empty())
        .any(|token| {
            let rest = key.replace(token, "");
            let piece = |part: &str| {
                part.len() < token.len()
                    && part
                        .chars()
                        .any(|character| !character.is_alphanumeric() && character != '_')
            };
            let ends = rest
                .char_indices()
                .filter_map(|(at, _)| rest.get(at..))
                .any(|tail| piece(tail) && token.starts_with(tail));
            let starts = rest
                .char_indices()
                .filter_map(|(at, character)| rest.get(..at + character.len_utf8()))
                .any(|head| piece(head) && token.ends_with(head));
            ends || starts
        })
}
