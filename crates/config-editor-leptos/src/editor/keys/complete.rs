//! What a typed character finishes: a line that only now shows it closes a
//! block re-indents ([`Typing::reindent_on_type`]), and markup it
//! begins is completed ([`Typing::complete_on_type`]).

use web_sys::KeyboardEvent;

use super::super::edit::{Change, Edit};
use super::super::language::ConfigLanguage;
use super::super::state::EditorState;
use super::super::typing::Closer;
use crate::editor::text::{TextRange, TextSize, byte_range, utf16_of};

/// `typed` typed at byte `start` of `source` on a line that re-indents
/// for it: the line up to it written again at the new indent, the caret
/// after it; `None` when the line keeps its indent.
pub(in crate::editor) fn reindented(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: TextSize,
    typed: char,
) -> Option<Change> {
    let start = start.min(TextSize::of(source));
    // The language API takes the caret in UTF-16 units.
    let indent = behavior.reindent_on_type(source, utf16_of(source, start), typed)?;
    let byte = usize::from(start);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let content = source[line_start..byte].trim_start();
    Some(Change::replace(
        source,
        byte_range(line_start..byte),
        &format!("{indent}{content}{typed}"),
    ))
}

/// What typing a character that finishes markup writes.
pub(in crate::editor) struct Completed {
    pub(in crate::editor) change: Change,
    /// Right after the typed character (a byte of the text after): the
    /// completion follows it.
    pub(in crate::editor) caret: TextSize,
    /// The completion's length in bytes, typed over as a closer is.
    pub(in crate::editor) len: TextSize,
}

/// `source` with `typed` typed at byte `start` and what it finishes
/// written after it, the line re-indented as for the completion's last
/// character; `None` when it finishes nothing.
pub(in crate::editor) fn completed(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: TextSize,
    typed: char,
) -> Option<Completed> {
    let start = start.min(TextSize::of(source));
    let rest = behavior.complete_on_type(source, utf16_of(source, start), typed)?;
    let written = format!("{typed}{rest}");
    let last = written.chars().next_back()?;
    let leading = &written[..written.len() - last.len_utf8()];
    // All but its last character written, then the last typed there (the
    // line re-indented for it), as one change.
    let lead = Change::replace(source, TextRange::empty(start), leading);
    let probe = lead.apply(source)?;
    let probe_caret = start + TextSize::of(leading);
    let change = match reindented(behavior, &probe, probe_caret, last) {
        Some(reindent) => lead.compose(&reindent)?,
        None => Change::replace(source, TextRange::empty(start), &written),
    };
    let end = change.hull().map(|(_, written)| written.end())?;
    let len = TextSize::of(rest.as_str());
    Some(Completed {
        caret: end.checked_sub(len)?,
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
    start: TextSize,
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
    start: TextSize,
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
