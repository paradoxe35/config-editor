//! The host sends its change, not only a whole text (`applyChanges`, as
//! CodeMirror's dispatch): made on a text it knows (its base revision,
//! else its text now), rebased over the edits it had not heard, and made
//! through the one edit pipeline (`Origin::HostChange`), so locks, folds,
//! secret placeholders and the caret follow it exactly, with no diff.

use leptos::prelude::*;

use super::super::edit::{Change, Edit};
use super::super::paths;
use super::super::state::EditorState;
use super::super::text::{TextRange, TextSize, byte_of, text_size, utf16_of};
use super::HostUpdate;
use super::echoes::Print;
use super::host_inputs::carry::Lines;
use super::host_inputs::{Link, RangesFor};
use super::host_log::Named;

/// One replacement the host made: `range` (lines and columns of the text
/// it made it on) replaced by `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostChange {
    /// What it replaced, in the text it made the change on.
    pub range: paths::TextRange,
    /// What it wrote there.
    pub insert: String,
}

impl HostChange {
    /// `range` replaced by `insert`.
    pub fn new(range: paths::TextRange, insert: impl Into<String>) -> Self {
        Self {
            range,
            insert: insert.into(),
        }
    }
}

/// A host's change as given: by lines and columns (Leptos), or by UTF-16
/// indexes of the text it was made on (JavaScript, as `rangeOf` gives).
#[derive(Clone, Debug)]
pub enum HostEdits {
    /// Ranges by lines and columns.
    Lines(Vec<HostChange>),
    /// `(from, to, insert)` in UTF-16 indexes.
    Utf16(Vec<(usize, usize, String)>),
}

/// Why a host's change was refused (nothing changed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeError {
    /// The base revision can no longer be named (the host passed a newer
    /// one back, or gave a text of its own since).
    OutOfDate(u64),
    /// A base revision the editor never sent.
    Unknown(u64),
    /// A base text that is none the editor can name: not one it sent or
    /// was given (kept), nor the host's text now, nor the document.
    UnknownText,
    /// The replacements are not sorted and apart inside the base text.
    BadRanges,
    /// No editor is mounted.
    Unmounted,
}

impl std::fmt::Display for ChangeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeError::OutOfDate(revision) => write!(
                formatter,
                "applyChanges: revision {revision} is out of date: make the change on that revision or a later one"
            ),
            ChangeError::Unknown(revision) => write!(
                formatter,
                "applyChanges: revision {revision} was never sent by an editor"
            ),
            ChangeError::UnknownText => write!(
                formatter,
                "applyChanges: revision names a text the editor does not know: give the revision onChange sent it with, or a text the editor sent or was given, the host's text now or the document"
            ),
            ChangeError::BadRanges => write!(
                formatter,
                "applyChanges: the changes must be sorted, apart, and inside the text they were made on"
            ),
            ChangeError::Unmounted => write!(formatter, "applyChanges: the editor is not mounted"),
        }
    }
}

impl std::error::Error for ChangeError {}

impl HostEdits {
    /// The change these edits make of `text`.
    fn change_of(&self, text: &str) -> Option<Change> {
        let edits: Vec<(TextRange, String)> = match self {
            HostEdits::Lines(changes) => {
                let lines = Lines::new(text);
                changes
                    .iter()
                    .map(|change| {
                        let start = text_size(lines.byte(change.range.start));
                        let end = text_size(lines.byte(change.range.end));
                        (start <= end).then(|| (TextRange::new(start, end), change.insert.clone()))
                    })
                    .collect::<Option<_>>()?
            }
            HostEdits::Utf16(changes) => changes
                .iter()
                .map(|(from, to, insert)| {
                    let start = utf16_byte(text, *from)?;
                    let end = utf16_byte(text, *to)?;
                    (start <= end).then(|| (TextRange::new(start, end), insert.clone()))
                })
                .collect::<Option<_>>()?,
        };
        Change::replacing(TextSize::of(text), edits)
    }
}

/// The byte of UTF-16 index `units` of `text`, when it is inside it (one
/// inside a character is its end).
fn utf16_byte(text: &str, units: usize) -> Option<TextSize> {
    (units <= utf16_of(text, TextSize::of(text))).then(|| byte_of(text, units))
}

/// The revision of `text`, a text a host made its change on: a version the
/// log keeps of it (a text sent or given, as the editor reads it), else the
/// host's text now, else the document (named by a mark).
fn revision_of_text(link: &mut Link, text: &str) -> Option<u64> {
    if let Some(revision) = link.log.printed(Print::of(text)) {
        return Some(revision);
    }
    let text = crate::editor::line_endings::editor_text(text.to_string());
    let host = link.log.host_revision();
    if link.log.text_at(host).is_some_and(|host| *host == *text) {
        return Some(host);
    }
    (**link.log.document() == *text).then(|| link.log.mark())
}

/// Makes the host's `edits`, made on its text at `base` (a revision, or
/// the text itself; else its text now), in the document (see the module). Returns the revision of the
/// new document (recorded as sent: an exact echo when the host renders it
/// with that revision), or `None` when the edits change nothing. When the
/// edits were rebased over the user's newer ones, `on_change` hears the
/// result, which the host does not know.
pub(crate) fn apply_host_changes(
    state: EditorState,
    edits: &HostEdits,
    base: Option<&RangesFor>,
) -> Result<Option<u64>, ChangeError> {
    let in_host_language =
        state.config().host_language == state.sync().edit_language.get_untracked();
    let prepared = state
        .sync()
        .link(|link| {
            let base = match base {
                None => link.log.host_revision(),
                Some(RangesFor::Revision(revision)) => *revision,
                Some(RangesFor::Text(text)) => {
                    revision_of_text(link, text).ok_or(ChangeError::UnknownText)?
                }
            };
            match link.log.named(base) {
                Named::Kept => {}
                Named::OutOfDate => return Err(ChangeError::OutOfDate(base)),
                Named::Unknown => return Err(ChangeError::Unknown(base)),
            }
            let text = link.log.text_at(base).ok_or(ChangeError::OutOfDate(base))?;
            let change = edits.change_of(&text).ok_or(ChangeError::BadRanges)?;
            let since = link.log.change_since(base);
            Ok((text, change, since, link.log.document().clone()))
        })
        .ok_or(ChangeError::Unmounted)??;
    let (base_text, change, since, document) = prepared;
    if change.is_identity(&base_text) {
        return Ok(None);
    }
    let rebased = !since
        .as_ref()
        .is_some_and(|since| since.is_identity(&base_text));
    // Over the edits since the base, the host's insert first where both
    // wrote at one place; with no record of them (a text no edit made),
    // made on the text it names and moved by the safe diff.
    let mapped = match &since {
        Some(since) if rebased => change.map(since, true),
        Some(_) => Some(change.clone()),
        None => None,
    };
    let Some(mapped) = mapped.filter(|_| in_host_language) else {
        // Editing as another language, or no record from the base: the
        // host's text with its change, as a text of its own.
        let host = state.sync().host_text.get_value();
        let base_changed = change.apply(&base_text).ok_or(ChangeError::BadRanges)?;
        let text = if *base_text == *host {
            base_changed
        } else {
            match since.and_then(|since| change.map(&since, true)) {
                Some(mapped) => mapped.apply(&document).ok_or(ChangeError::BadRanges)?,
                None => base_changed,
            }
        };
        let taken = super::take_host_update(
            state,
            HostUpdate {
                value: Some(text),
                reset: true,
                ..HostUpdate::default()
            },
        )
        .map_err(|_| ChangeError::BadRanges)?;
        // The revision the reset's text is recorded at (a host with no
        // echoes has no copy on its way to name it).
        return Ok(taken
            .shown
            .then(|| state.sync().link(|link| link.log.host_revision()))
            .flatten());
    };
    let current = state.doc().text.get_untracked();
    let edit = Edit::host_change(current, mapped).ok_or(ChangeError::BadRanges)?;
    if !state.commit(edit) {
        return Ok(None);
    }
    let text = state.doc().text.shared_untracked();
    if rebased {
        // The result holds the user's newer edits: the host hears it.
        state.notify_change();
    } else {
        // The host's text with its change: what it holds, recorded as
        // sent, not told.
        state.sync().typed_in_host_language(&text);
        state.sync().link(|link| link.sent(&text));
    }
    Ok(state.sync().link(|link| link.echoes.last_sent()).flatten())
}

#[cfg(all(test, feature = "yaml", feature = "protect"))]
#[path = "host_changes_tests.rs"]
mod tests;
