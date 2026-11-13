//! What a typed character finishes: a line that only now shows it closes a
//! block re-indents ([`ConfigLanguage::reindent_on_type`]), and markup it
//! begins is completed ([`ConfigLanguage::complete_on_type`]).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::Edit;
use super::super::keyboard::{replace_selection, utf16_to_byte_index};
use super::super::language::ConfigLanguage;
use super::super::state::EditorState;
use super::super::typing::Closer;

/// `source` with `typed` typed at `start` (UTF-16) on a line that re-indents
/// for it, and the caret after it; `None` when the line keeps its indent.
pub(in crate::editor) fn reindented(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: usize,
    typed: char,
) -> Option<(String, usize)> {
    let indent = behavior.reindent_on_type(source, start, typed)?;
    let byte = utf16_to_byte_index(source, start);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let content = source[line_start..byte].trim_start();
    let line_start_utf16 = source[..line_start].encode_utf16().count();
    Some(replace_selection(
        source,
        line_start_utf16,
        start,
        &format!("{indent}{content}{typed}"),
    ))
}

/// What typing a character that finishes markup writes.
pub(in crate::editor) struct Completed {
    pub(in crate::editor) text: String,
    /// Right after the typed character: the completion follows it.
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
    let (probe, probe_caret) = replace_selection(source, start, start, leading);
    let (text, end) = reindented(behavior, &probe, probe_caret, last)
        .unwrap_or_else(|| replace_selection(source, start, start, &written));
    let len = rest.encode_utf16().count();
    Some(Completed {
        text,
        caret: end - len,
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
    // Opened by the `<` before the typed `/`, so Enter right after it steps
    // past the completion rather than splitting it.
    state.remember(Closer {
        distance: done.text.encode_utf16().count() - done.caret,
        len: done.len,
        opener: Some('<'),
        completion: true,
    });
    state.commit(
        Edit::new(before, (start, start), done.text, done.caret)
            .typed("insertText", event.time_stamp()),
    );
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
    let Some((updated, caret)) = reindented(state.behavior(), &before, start, typed) else {
        return false;
    };
    event.prevent_default();
    state.commit(
        Edit::new(before, (start, start), updated, caret).typed("insertText", event.time_stamp()),
    );
    state.close_suggestions();
    true
}
