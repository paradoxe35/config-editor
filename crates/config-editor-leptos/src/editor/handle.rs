//! [`EditorHandle`]: what a host holds to ask a mounted editor where things
//! are and to move it there.

use leptos::prelude::*;

use super::paths::{FieldPath, TextPosition, TextRange, field_range, path_at};
use super::state::EditorState;
use crate::reactive::later::Alive;

/// A handle on a mounted [`ConfigEditor`](crate::ConfigEditor), for paths
/// and positions: where a field is written, which field a position is on,
/// and "jump to field".
///
/// Create one, pass it as the `handle` prop, and call it from anywhere
/// (it is `Copy`). Before the editor mounts, and after it unmounts, the
/// queries answer `None` and the actions do nothing.
///
/// Positions refer to the text on screen: in the `edit_as` language while
/// editing as another one. Paths are the same in every language.
///
/// ```ignore
/// use config_editor_leptos::{ConfigEditor, EditorHandle};
///
/// let editor = EditorHandle::new();
/// view! {
///     <button on:click=move |_| { editor.reveal("/web/listen"); }>"Go to web.listen"</button>
///     <span class="breadcrumb">{move || editor.current_path().map(|path| path.to_pointer())}</span>
///     <ConfigEditor handle=editor value=text on_change=move |t| text.set(t) />
/// }
/// ```
#[derive(Clone, Copy)]
pub struct EditorHandle {
    state: RwSignal<Option<EditorState>>,
}

impl Default for EditorHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EditorHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorHandle")
            .field("mounted", &self.is_mounted())
            .finish()
    }
}

impl EditorHandle {
    /// A handle not attached to an editor yet.
    pub fn new() -> Self {
        EditorHandle {
            state: RwSignal::new(None),
        }
    }

    /// Attaches the editor that mounted with this handle, until it unmounts.
    pub(super) fn attach(self, state: EditorState) {
        self.state.set(Some(state));
        on_cleanup(move || {
            let _ = self.state.try_set(None);
        });
    }

    pub(super) fn editor(self) -> Option<EditorState> {
        self.state
            .try_get_untracked()
            .flatten()
            .filter(|state| state.is_alive())
    }

    /// The mounted editor, tracked: a closure reading it re-runs when the
    /// editor mounts or unmounts.
    pub(super) fn editor_tracked(self) -> Option<EditorState> {
        self.state.try_get().flatten()
    }

    /// Whether an editor is mounted with this handle. Reactive.
    pub fn is_mounted(&self) -> bool {
        self.state.try_with(Option::is_some).unwrap_or(false)
    }

    /// The text on screen.
    pub fn text(&self) -> Option<String> {
        self.editor()?.doc().text.try_get_untracked()
    }

    /// Where the field at `path` is written: its key and value when they share
    /// a line, the key alone when the value is a block below it, the value
    /// alone for an array item (see [`Language::range_of`](crate::Language::range_of)).
    pub fn range_of(&self, path: impl Into<FieldPath>) -> Option<TextRange> {
        let state = self.editor()?;
        let pointer = path.into().to_pointer();
        state
            .doc()
            .text
            .try_with_untracked(|text| field_range(state.behavior(), text, &pointer))
            .flatten()
    }

    /// The path of what `position` is on (see
    /// [`Language::path_at`](crate::Language::path_at)).
    pub fn path_at(&self, position: TextPosition) -> Option<FieldPath> {
        let state = self.editor()?;
        state
            .doc()
            .text
            .try_with_untracked(|text| {
                if position.line > text.split('\n').count() {
                    return None;
                }
                path_at(state.behavior(), text, position.to_offset(text))
            })
            .flatten()
    }

    /// The path of what the caret is on, the root at the top level. Reactive:
    /// read it in a closure (a breadcrumb) to follow the caret; it changes
    /// only when the path does. See also the `on_path_change` prop.
    pub fn current_path(&self) -> Option<FieldPath> {
        self.editor_tracked()
            .and_then(|state| state.doc().caret_path.try_get())
    }

    /// Scrolls to the field at `path`, selects it (as
    /// [`range_of`](Self::range_of) gives it: the key and an inline value, or
    /// the key of a block), and focuses the editor. Returns whether the
    /// field was found.
    pub fn reveal(&self, path: impl Into<FieldPath>) -> bool {
        let Some(state) = self.editor() else {
            return false;
        };
        let Some(range) = self.range_of(path) else {
            return false;
        };
        let (start, end) = state
            .doc()
            .text
            .with_untracked(|text| (range.start.to_offset(text), range.end.to_offset(text)));
        state.hide_shown_problem();
        state.focus_range(start, end).is_some()
    }

    /// Saves as `Ctrl+S` does: sends the document, in the host's language,
    /// to the `on_save` prop (see [`SaveRequest`](crate::SaveRequest)).
    /// Returns whether it was sent: `false` without `on_save`, before the
    /// editor mounts, and while an Edit-as edit cannot convert back (the
    /// caret then goes to the problem).
    pub fn save(&self) -> bool {
        self.editor().is_some_and(|state| state.save())
    }

    /// Whether the text differs from the baseline (see the
    /// `on_dirty_change` prop). Reactive; `false` before the editor mounts.
    pub fn is_dirty(&self) -> bool {
        self.editor_tracked()
            .and_then(|state| state.editing().dirty.try_get())
            .unwrap_or(false)
    }

    /// Focuses the editor, keeping its selection.
    pub fn focus(&self) {
        if let Some(element) = self.editor().and_then(|state| state.textarea()) {
            let _ = element.focus();
        }
    }
}
