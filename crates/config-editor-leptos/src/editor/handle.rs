//! [`EditorHandle`]: what a host holds to ask a mounted editor where things
//! are and to move it there.

use leptos::prelude::*;

use super::paths::{FieldPath, TextPosition, TextRange, field_range, offset_of, path_at};
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
    /// Shows the editor when it is hidden (a `ConfigDocuments` panel not
    /// selected) before it takes the focus.
    show: StoredValue<Option<Callback<()>>>,
    /// The text the editor opens with in place of the host's (a
    /// `ConfigDocuments` document's `edit_as_text`), taken when it mounts.
    opening: StoredValue<Option<String>>,
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
            show: StoredValue::new(None),
            opening: StoredValue::new(None),
        }
    }

    /// Attaches the editor that mounted with this handle, until it unmounts.
    pub(super) fn attach(self, state: EditorState) {
        self.state.set(Some(state));
        on_cleanup(move || {
            let _ = self.state.try_set(None);
        });
    }

    /// Calls `show` before the editor takes the focus (`reveal`,
    /// `reveal_range`, `focus`): the documents view selects its document.
    pub(crate) fn show_with(self, show: Callback<()>) {
        let _ = self.show.try_set_value(Some(show));
    }

    /// The editor mounting next with this handle opens with `text`, in its
    /// `edit_as` language, in place of the host's text converted.
    pub(crate) fn open_with(self, text: Option<String>) {
        let _ = self.opening.try_set_value(text);
    }

    /// The text to open with, once.
    pub(super) fn take_opening(self) -> Option<String> {
        self.opening.try_update_value(std::mem::take).flatten()
    }

    /// The language on screen.
    pub(crate) fn language_on_screen(self) -> Option<crate::Language> {
        self.editor()?.sync().edit_language.try_get_untracked()
    }

    /// Asks for `language` on screen, as a new `edit_as` does, also when it
    /// is the one asked for last (see `EditorState::ask_edit_as`).
    pub(crate) fn ask_edit_as(self, language: crate::Language) {
        if let Some(state) = self.editor() {
            untrack(|| state.ask_edit_as(language));
        }
    }

    /// Shows the editor where it is hidden, before it takes the focus.
    fn show(self) {
        if let Some(show) = self.show.try_get_value().flatten() {
            show.run(());
        }
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

    /// The text on screen, in the language on screen (the Edit-as language
    /// while editing as another one); use [`value`](Self::value) for the
    /// host's document.
    pub fn text(&self) -> Option<String> {
        self.editor()?.doc().text.try_get_untracked()
    }

    /// The document in the host's language, with pending edits applied: a
    /// pending Edit-as edit is converted first (and reaches `on_change`
    /// first), as [`save`](Self::save) does. `None` before the editor
    /// mounts, and while an Edit-as edit cannot be written in the host's
    /// language (invalid text, or a value the language cannot hold). Written
    /// with the host's line endings (CRLF when its text had them): the text
    /// `on_change` and `on_save` get.
    pub fn value(&self) -> Option<String> {
        let state = self.editor()?;
        let text = state.host_document()?;
        Some(state.sync().line_endings.restore(text))
    }

    /// The unified diff (`diff -u`) of the baseline against
    /// [`value`](Self::value), both in the host's language, with `context`
    /// unchanged lines around each change; empty when they are the same.
    /// Its lines end with LF whatever the host's line endings (as
    /// [`unified_diff`](crate::unified_diff) and the JS `unifiedDiff()`
    /// write it). `None` when `value` is. Needs the `diff` feature.
    #[cfg(feature = "diff")]
    pub fn unified_diff(&self, context: usize) -> Option<String> {
        let state = self.editor()?;
        let current = state.host_document()?;
        let baseline = state.editing().baseline.try_get_untracked()?;
        Some(super::changes::unified_diff(&baseline, &current, context))
    }

    /// Where the field at `path` is written: its key and value when they share
    /// a line, the key alone when the value is a block below it, the value
    /// alone for an array item, the dash or header of an item holding a
    /// block (see [`Language::range_of`](crate::Language::range_of)).
    pub fn range_of(&self, path: impl Into<FieldPath>) -> Option<TextRange> {
        let state = self.editor()?;
        let pointer = path.into().to_pointer();
        let behavior = state.behavior();
        state
            .doc()
            .text
            .try_with_untracked(|text| {
                state.doc().spans.try_update_value(|kept| {
                    kept.with(behavior, text, || field_range(behavior, text, &pointer))
                })
            })
            .flatten()
            .flatten()
    }

    /// [`range_of`](Self::range_of), with the UTF-16 offsets in the text
    /// on screen its start and end are at, read from what is kept for the
    /// text (for the browser bindings, whose ranges carry string indices).
    pub(crate) fn range_and_offsets_of(
        &self,
        path: impl Into<FieldPath>,
    ) -> Option<(TextRange, usize, usize)> {
        let state = self.editor()?;
        let pointer = path.into().to_pointer();
        let behavior = state.behavior();
        state
            .doc()
            .text
            .try_with_untracked(|text| {
                state.doc().spans.try_update_value(|kept| {
                    kept.with(behavior, text, || {
                        let range = field_range(behavior, text, &pointer)?;
                        let line_breaks_only = !text.contains('\r');
                        let offset = |position| offset_of(text, position, line_breaks_only);
                        Some((range, offset(range.start), offset(range.end)))
                    })
                })
            })
            .flatten()
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
    /// field was found. The editor of a `ConfigDocuments` document not
    /// shown selects its document first.
    pub fn reveal(&self, path: impl Into<FieldPath>) -> bool {
        let Some(state) = self.editor() else {
            return false;
        };
        let Some(range) = self.range_of(path) else {
            return false;
        };
        let Some((start, end)) = self.offsets_of(range) else {
            return false;
        };
        self.show();
        state.hide_shown_problem();
        state.focus_range(start, end).is_some()
    }

    /// Selects `range`, scrolls it into view inside the editor and focuses
    /// the editor without moving the page, closing popups: what
    /// [`reveal`](Self::reveal) does, for any stretch of text (a comment, a
    /// blank line, part of a value). The range is in the text on screen (see
    /// [`TextPosition`]: one-based lines, character columns), in the
    /// `edit_as` language while editing as another one; a range past the
    /// text's end is clamped to it, and one ending before it starts is read
    /// from its end to its start. Returns `false` before the editor mounts
    /// and for an empty document.
    pub fn reveal_range(&self, range: TextRange) -> bool {
        let Some(state) = self.editor() else {
            return false;
        };
        let Some((start, end)) = self.offsets_of(range) else {
            return false;
        };
        self.show();
        state.hide_shown_problem();
        state.focus_range(start, end).is_some()
    }

    /// The UTF-16 offsets of `range` in the text on screen, clamped to it and
    /// in order; `None` before the editor mounts and for an empty document.
    pub(super) fn offsets_of(&self, range: TextRange) -> Option<(usize, usize)> {
        let state = self.editor()?;
        state
            .doc()
            .text
            .try_with_untracked(|text| {
                if text.is_empty() {
                    return None;
                }
                let start = range.start.to_offset(text);
                let end = range.end.to_offset(text);
                Some((start.min(end), start.max(end)))
            })
            .flatten()
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

    /// Focuses the editor, keeping its selection; in `ConfigDocuments`,
    /// selecting its document first.
    pub fn focus(&self) {
        if self.editor().is_some() {
            self.show();
        }
        if let Some(element) = self.editor().and_then(|state| state.textarea()) {
            let _ = element.focus_in_place();
        }
    }
}
