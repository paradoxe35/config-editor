//! What a typed character finishes: a line that only now shows it closes a
//! block re-indents ([`Typing::reindent_on_type`]), and markup it
//! begins is completed ([`Typing::complete_on_type`]).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::{Change, Edit};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::ConfigLanguage;
use super::super::state::EditorState;
use super::super::typing::Closer;

/// `typed` typed at `start` (UTF-16 of `source`) on a line that re-indents
/// for it: the line up to it written again at the new indent, the caret
/// after it; `None` when the line keeps its indent.
pub(in crate::editor) fn reindented(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: usize,
    typed: char,
) -> Option<Change> {
    let indent = behavior.reindent_on_type(source, start, typed)?;
    let byte = utf16_to_byte_index(source, start);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let content = source[line_start..byte].trim_start();
    Some(Change::replace_bytes(
        source,
        line_start..byte,
        &format!("{indent}{content}{typed}"),
    ))
}

/// What typing a character that finishes markup writes.
pub(in crate::editor) struct Completed {
    pub(in crate::editor) change: Change,
    /// Right after the typed character (UTF-16 of the text after): the
    /// completion follows it.
    pub(in crate::editor) caret: usize,
    /// The completion's length (UTF-16), typed over as a closer is.
    pub(in crate::editor) len: usize,
}

/// `source` with `typed` typed at `start` (UTF-16) and what it finishes
/// written after it, the line re-indented as for the completion's last
/// character; `None` when it finishes nothing.
pub(in crate::editor) fn completed(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: usize,
    typed: char,
) -> Option<Completed> {
    let rest = behavior.complete_on_type(source, start, typed)?;
    let written = format!("{typed}{rest}");
    let last = written.chars().next_back()?;
    let leading = &written[..written.len() - last.len_utf8()];
    // All but its last character written, then the last typed there (the
    // line re-indented for it), as one change.
    let lead = Change::insert_units(source, start, leading);
    let probe = lead.apply(source)?;
    let probe_caret = start + leading.encode_utf16().count();
    let change = match reindented(behavior, &probe, probe_caret, last) {
        Some(reindent) => lead.compose(&reindent)?,
        None => Change::insert_units(source, start, &written),
    };
    let end = change.hull().map(|(_, written)| written.end)?;
    let text = change.apply(source)?;
    let len = rest.encode_utf16().count();
    Some(Completed {
        caret: text[..end].encode_utf16().count() - len,
        change,
        len,
    })
}

/// Types `typed` with what it finishes (`</` → `</|a>`): the caret stays
/// after the typed character, and the rest is the editor's closer, typed
/// over character by character (typing the end tag whole writes it once),
/// and stepped past by Enter.
pub(super) fn complete(
    state: EditorState,
    event: &KeyboardEvent,
    start: usize,
    typed: char,
) -> bool {
    let before = state.doc().text.get_untracked();
    let Some(done) = completed(state.behavior(), &before, start, typed) else {
        return false;
    };
    event.prevent_default();
    let edit = Edit::command(before, done.change, (start, start))
        .caret_at(done.caret)
        .typed("insertText", event.time_stamp());
    // Opened by the `<` before the typed `/`, so Enter right after it steps
    // past the completion rather than splitting it.
    if state.commit(edit) {
        state.remember(Closer {
            start: done.caret,
            len: done.len,
            opener: Some('<'),
            completion: true,
        });
    }
    state.close_suggestions();
    true
}

/// A line that only now shows it closes a block re-indents.
pub(super) fn reindent(
    state: EditorState,
    event: &KeyboardEvent,
    start: usize,
    typed: char,
) -> bool {
    let before = state.doc().text.get_untracked();
    let Some(change) = reindented(state.behavior(), &before, start, typed) else {
        return false;
    };
    event.prevent_default();
    state.commit(
        Edit::command(before, change, (start, start)).typed("insertText", event.time_stamp()),
    );
    state.close_suggestions();
    true
}
