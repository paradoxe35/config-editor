//! [`EditorHandle`]: what a host holds to ask a mounted editor where things
//! are and to move it there.

use leptos::prelude::*;

use super::bridge::links::Clipboard;

use super::paths::{
    FieldPath, TextPosition, TextRange, api_units, field_range, lines, offset_of, path_at,
};
use super::protect::DocumentPlaceholders;
use super::state::EditorState;
use crate::editor::text::byte_of;
use crate::reactive::later::Alive;
use std::sync::Arc;

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
/// use leptos_config_editor::{ConfigEditor, EditorHandle};
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
    /// The placeholders of every editor of a `ConfigDocuments` view, taken
    /// when the editor mounts (see `protect::foreign`).
    foreign: StoredValue<Option<Signal<Arc<Vec<DocumentPlaceholders>>>>>,
    /// The clipboard records the editor mounting next shares (a
    /// `ConfigDocuments` view's editors of one host language), taken when
    /// it mounts.
    clipboard: StoredValue<Option<StoredValue<Clipboard>>>,
    /// The document's record of its host the editor mounting next takes
    /// over (a binding's, or a documents view's), taken when it mounts.
    channel: StoredValue<Option<super::sync::HostChannel>, LocalStorage>,
    /// The echo record of the editor that showed this document before (a
    /// documents view's document listed again with its id), taken when the
    /// editor mounts.
    handed: StoredValue<Option<super::sync::Handoff>>,
    /// Where the editor mounting next reads the names of its
    /// `x-config-editor-keys` fields and sends go to definition (a
    /// documents view's, over its open documents), taken when it mounts.
    key_names: StoredValue<Option<super::key_names::KeyNamesInput>>,
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
            foreign: StoredValue::new(None),
            clipboard: StoredValue::new(None),
            channel: StoredValue::new_local(None),
            handed: StoredValue::new(None),
            key_names: StoredValue::new(None),
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

    /// The editor mounting next with this handle reads the placeholders of
    /// a documents view's editors from `placeholders` (a signal the
    /// mounting view owns).
    pub(crate) fn share_placeholders(self, placeholders: Signal<Arc<Vec<DocumentPlaceholders>>>) {
        let _ = self.foreign.try_set_value(Some(placeholders));
    }

    /// The editor mounting next with this handle keeps its copies' records
    /// in `clipboard`, which other editors read and write too: an item
    /// copied in one pasted in another keeps its source.
    pub(crate) fn share_clipboard(self, clipboard: StoredValue<Clipboard>) {
        let _ = self.clipboard.try_set_value(Some(clipboard));
    }

    /// The editor mounting next with this handle keeps its host's record in
    /// `channel` (see `sync::host_inputs`): the echoes on their way, the
    /// host log and the lists given, made before it mounted or by the
    /// editor it replaces.
    pub(crate) fn give_channel(self, channel: super::sync::HostChannel) {
        let _ = self.channel.try_set_value(Some(channel));
    }

    /// The record to take over, once.
    pub(super) fn take_channel(self) -> Option<super::sync::HostChannel> {
        self.channel.try_update_value(std::mem::take).flatten()
    }

    /// The editor mounting next with this handle takes over `handed`, the
    /// echo record of the editor that showed its document before (see
    /// [`HostChannel::handoff`](super::sync::HostChannel::handoff)).
    pub(crate) fn hand_on(self, handed: super::sync::Handoff) {
        let _ = self.handed.try_set_value(Some(handed));
    }

    /// The echo record to take over, once.
    pub(super) fn take_handed(self) -> Option<super::sync::Handoff> {
        self.handed.try_update_value(std::mem::take).flatten()
    }

    /// The texts the mounted editor sent that its host has not handed back
    /// yet, for the editor made next for its document; `None` when none is
    /// on its way or no editor is mounted.
    pub(crate) fn handoff(self) -> Option<super::sync::Handoff> {
        self.editor()?.sync().channel.try_get_value()?.handoff()
    }

    /// Takes `update` from the host through the editor's one entry (see
    /// `sync::host_inputs`); `None` while no editor is mounted.
    pub(crate) fn host_update(
        &self,
        update: super::sync::HostUpdate,
    ) -> Option<Result<super::sync::Taken, super::sync::AnchorError>> {
        let state = self.editor()?;
        Some(untrack(|| super::sync::take_raw_host_update(state, update)))
    }

    /// The editor mounting next with this handle reads the names of its
    /// `x-config-editor-keys` fields from `input` (in place of its
    /// `key_names` and `on_go_to_definition` props).
    pub(crate) fn give_key_names(self, input: super::key_names::KeyNamesInput) {
        let _ = self.key_names.try_set_value(Some(input));
    }

    /// The names' input to take over, once.
    pub(super) fn take_key_names(self) -> Option<super::key_names::KeyNamesInput> {
        self.key_names.try_update_value(std::mem::take).flatten()
    }

    /// The clipboard to share, once.
    pub(super) fn take_clipboard(self) -> Option<StoredValue<Clipboard>> {
        self.clipboard.try_update_value(std::mem::take).flatten()
    }

    /// The placeholders of a documents view's editors, once.
    pub(super) fn take_foreign(self) -> Option<Signal<Arc<Vec<DocumentPlaceholders>>>> {
        self.foreign.try_update_value(std::mem::take).flatten()
    }

    /// The real placeholders of the editor, once mounted (`source`: its
    /// document's file name). Tracked.
    pub(crate) fn placeholders(self, source: String) -> Option<DocumentPlaceholders> {
        let state = self.state.try_get().flatten()?;
        if !state.is_alive() {
            return None;
        }
        state.protect().own_placeholders(source)
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
    /// language (invalid text, or a value the language cannot hold), and
    /// while a read-only field reads otherwise than the host gave it (a
    /// duplicate shadows it: the save waits too, saying why). Written
    /// with the host's line endings (CRLF when its text had them): the text
    /// `on_change` and `on_save` get.
    pub fn value(&self) -> Option<String> {
        let state = self.editor()?;
        let text = state.to_host(super::sync::Route::Value).ok()?;
        Some(state.sync().line_endings.restore(text))
    }

    /// The revision of the text `on_change` sent last, while the host has
    /// not handed it back (with the `echoes` prop): read it in `on_change`
    /// and pass it back as the `value_revision` prop with that text, for an
    /// exact echo match. `None` before the editor mounts, without `echoes`,
    /// and once the host has handed back every text sent.
    pub fn sent_revision(&self) -> Option<u64> {
        self.editor()?
            .sync()
            .link(|link| link.echoes.latest_revision())
            .flatten()
    }

    /// Makes the host's own change of its text (as CodeMirror's dispatch):
    /// `changes` (ranges by lines and columns, sorted and apart) made on the
    /// text `base` names: the revision `on_change` sent it with
    /// ([`sent_revision`](Self::sent_revision); `7.into()`), or the text
    /// itself (a text the editor sent or was given, the host's text now or
    /// the document; `"…".into()`), else on the host's text now (the text
    /// `on_change` sent last, or the host gave last). Edits the user made since are kept, the
    /// host's rebased over them (at one place, the host's first); locks,
    /// folds, secret placeholders and the caret follow it exactly, and Undo
    /// does not undo it. Returns the revision of the new document (pass it
    /// back as `value_revision` with that text), `None` when nothing was
    /// sent: no change, no editor mounted, or the save is held (the
    /// host's change reaches the host with the next `on_change` once the
    /// save goes through). When it was rebased over newer edits,
    /// `on_change` hears the result.
    pub fn apply_changes(
        &self,
        changes: Vec<super::sync::HostChange>,
        base: Option<super::sync::RangesFor>,
    ) -> Option<u64> {
        let state = self.editor()?;
        untrack(|| {
            super::sync::apply_host_changes(
                state,
                &super::sync::HostEdits::Lines(changes),
                base.as_ref(),
            )
        })
        .ok()
        .flatten()
    }

    /// [`apply_changes`](Self::apply_changes) with every refusal said.
    pub(crate) fn apply_edits(
        &self,
        edits: &super::sync::HostEdits,
        base: Option<&super::sync::RangesFor>,
    ) -> Result<Option<u64>, super::sync::ChangeError> {
        let state = self.editor().ok_or(super::sync::ChangeError::Unmounted)?;
        untrack(|| super::sync::apply_host_changes(state, edits, base))
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
        let current = state.to_host(super::sync::Route::UnifiedDiff).ok()?;
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
                        let offset =
                            |position| usize::from(offset_of(text, position, line_breaks_only));
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
        self.path_in_text(|text| {
            (position.line <= lines(text).count())
                .then(|| usize::from(offset_of(text, position, !text.contains('\r'))))
        })
    }

    /// [`path_at`](Self::path_at) the UTF-16 offset `offset` of the text on
    /// screen (for the browser bindings, whose offsets are string indices).
    pub(crate) fn path_at_offset(&self, offset: usize) -> Option<FieldPath> {
        self.path_in_text(|text| {
            if text.contains('\r') {
                // As a position: a place inside a `\r\n` is the line's end.
                let position = TextPosition::from_offset(text, offset);
                return (offset <= api_units(text, text.len()))
                    .then(|| usize::from(offset_of(text, position, false)));
            }
            (offset <= api_units(text, text.len())).then_some(offset)
        })
    }

    /// The path at the offset `place` finds in the text on screen. What the
    /// language reads of the whole text (its lines, its walk from the top)
    /// is kept for the next lookup on the same text.
    fn path_in_text(&self, place: impl FnOnce(&str) -> Option<usize>) -> Option<FieldPath> {
        let state = self.editor()?;
        let behavior = state.behavior();
        state
            .doc()
            .text
            .try_with_untracked(|text| {
                state.doc().spans.try_update_value(|kept| {
                    kept.with_readings(behavior, text, || path_at(behavior, text, place(text)?))
                })
            })
            .flatten()
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
    /// the key of a block), and focuses the editor. Like every jump, it
    /// centers the field when it is out of view or near an edge and flashes
    /// it for a second. Returns whether the field was found. The editor of
    /// a `ConfigDocuments` document not shown selects its document first.
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
        let (start, end) = state
            .doc()
            .text
            .with_untracked(|text| (byte_of(text, start), byte_of(text, end)));
        self.show();
        state.hide_shown_problem();
        state.jump_to_range(start, end).is_some()
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
        let (start, end) = state
            .doc()
            .text
            .with_untracked(|text| (byte_of(text, start), byte_of(text, end)));
        self.show();
        state.hide_shown_problem();
        state.jump_to_range(start, end).is_some()
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
    /// editor mounts, while the text does not parse, and while an Edit-as
    /// edit cannot convert back (the caret then goes to the problem).
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

    /// Goes to the definition of the name at the caret, as `F12` does (a
    /// field whose schema says it names a key defined elsewhere, see
    /// [`KeyNames`](crate::KeyNames)): its key when the document (or, in
    /// `ConfigDocuments`, another open document) defines it, else the
    /// `on_go_to_definition` prop hears where it is. For a host's button
    /// or menu. Returns whether there was a known name at the caret;
    /// `false` before the editor mounts.
    pub fn go_to_definition(&self) -> bool {
        self.editor()
            .is_some_and(|state| untrack(|| state.go_to_definition()))
    }

    /// Lists the uses of the name at the caret (or of the key defining
    /// it), as `Shift+F12` does: the open documents' at once, the files'
    /// the `on_find_uses` prop answers with when they come. For a host's
    /// button or menu. The focus goes to the text, as for `Shift+F12`, so
    /// the list's keys (arrows, Enter, Escape) work at once. Returns
    /// whether there was a name at the caret; `false` before the editor
    /// mounts.
    pub fn find_uses(&self) -> bool {
        let found = self
            .editor()
            .is_some_and(|state| untrack(|| state.find_uses()));
        if found {
            self.focus();
        }
        found
    }

    /// Hides what the lines of `hits` (uses found in `text`, written in
    /// `language`) quote of a secret, by this editor's schema and
    /// placeholders. Returns `false` before the editor mounts.
    pub(crate) fn hide_use_secrets(
        self,
        text: &str,
        language: crate::Language,
        hits: &mut [super::key_names::UseHit],
    ) -> bool {
        let Some(state) = self.editor() else {
            return false;
        };
        untrack(|| state.hide_use_secrets(text, language.behavior(), hits));
        true
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
