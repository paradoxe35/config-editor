//! The editor's shared state: one `Copy` handle, passed to every component
//! and event handler, pointing at the state's parts.
//!
//! Each part is a small group of signals defined in and owned by the module
//! that uses it; this module only composes them:
//!
//! | Field        | Type                | Module          |
//! |--------------|---------------------|-----------------|
//! | `config`     | [`Config`]          | here            |
//! | `doc`        | `DocState`          | `document`      |
//! | `sync`       | `SyncState`         | `sync`          |
//! | `view`       | `ViewState`         | `layout`        |
//! | `completion` | `CompletionState`   | `completion`    |
//! | `typing`     | `TypingState`       | `typing`        |
//! | `hover`      | `HoverState`        | `hover`         |
//! | `problems`   | `ProblemsState`     | `problems`      |
//! | `changes`    | `ChangesState`      | `changes`       |
//! | `search`     | `SearchState`       | `search`        |
//! | `go_to_line` | `GoToLine`          | `go_to_line`    |
//! | `editing`    | `HostEditing`       | `host_editing`  |
//! | `fold`       | `FoldState`         | `folding`       |
//! | `protect`    | `ProtectState`      | `protect`       |
//! | `a11y`       | `A11yState`         | `a11y`          |

use std::sync::Arc;

use leptos::prelude::*;

use super::a11y::A11yState;
use super::changes::ChangesState;
use super::completion::{self, CompletionState};
use super::diagnostics::SchemaBundle;
use super::document::DocState;
use super::edit_as::EditAsState;
use super::folding::{FoldState, Projection, SourceArea};
use super::go_to_line::GoToLine;
use super::history::Selection;
use super::host_diagnostics::HostDiagnostic;
use super::host_editing::{
    self, BaselineConflict, HostEditing, SaveRequest, SentTexts, noting_sent,
};
use super::hover::HoverState;
use super::languages::Language;
use super::layout::{ViewState, string_bodies};
use super::line_rows::rendered_lines;
use super::live_settings::LiveSettings;
use super::problems::{self, ProblemsState};
use super::protect::{Inputs, ProtectInit, ProtectState};
use super::search::SearchState;
use super::sync::SyncState;
use super::typing::TypingState;
use crate::{EditorSettings, EditorStrings};

/// What the editor is mounted with and keeps: the host's language, the
/// schema, the read-only switch and the settings.
pub(super) struct Config {
    pub(super) host_language: Language,
    pub(super) schema: StoredValue<Option<Arc<SchemaBundle>>>,
    /// The `read_only` prop, which may switch live.
    pub(super) read_only: Signal<bool>,
    /// The `settings` prop, one memo per setting (see `live_settings`).
    pub(super) settings: LiveSettings,
}

/// What the component hands over to build the state.
pub(super) struct Init {
    pub(super) host_language: Language,
    pub(super) edit_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) schema_error: Option<String>,
    /// The host's text (controlled value or initial value).
    pub(super) host_text: String,
    /// The host's diff baseline at mount, in the host language (see
    /// `host_editing` for later ones).
    pub(super) baseline: String,
    pub(super) read_only: Signal<bool>,
    pub(super) settings: Signal<EditorSettings>,
    /// The host's `on_change`; edits reach it directly or through the
    /// Edit-as conversion (see `sync`).
    pub(super) host_on_change: Option<Callback<String>>,
    /// The host's own problems (see `host_diagnostics`).
    pub(super) host_diagnostics: Signal<Vec<HostDiagnostic>>,
    /// The host's read-only paths and ranges (see `protect`).
    pub(super) protect: ProtectInit,
    pub(super) on_save: Option<Callback<SaveRequest>>,
    pub(super) on_baseline_conflict: Option<Callback<BaselineConflict>>,
}

/// The editor's state, shared by every handler: one pointer to its
/// [`EditorFields`].
///
/// Handlers and view closures capture the state by copy, and an unoptimized
/// (debug) wasm build keeps each copy on the stack while it builds them, so
/// the fields held inline overflowed the stack once they reached about a
/// kilobyte. A pointer keeps every capture at one word however many fields
/// there are.
///
/// The fields (small signal and stored-value handles, about 1 KB) are
/// allocated once per mounted editor and never freed: timers and other
/// handlers may still read them after the editor unmounts, where the handles
/// inside answer as disposed. Anything larger than a handle (the schema, a
/// text) lives behind one, so it is freed with the editor's reactive owner.
#[derive(Clone, Copy)]
pub(super) struct EditorState(&'static EditorFields);

impl std::ops::Deref for EditorState {
    type Target = EditorFields;

    fn deref(&self) -> &EditorFields {
        self.0
    }
}

/// What an [`EditorState`] points to: its parts (see the module docs).
pub(super) struct EditorFields {
    pub(super) config: Config,
    pub(super) doc: DocState,
    /// The host's text and Edit-as (see `sync`).
    pub(super) sync: SyncState,
    pub(super) view: ViewState,
    pub(super) completion: CompletionState,
    pub(super) typing: TypingState,
    pub(super) hover: HoverState,
    pub(super) problems: ProblemsState,
    /// Changes against the baseline (see `changes`).
    pub(super) changes: ChangesState,
    /// Find and replace (see `search`).
    pub(super) search: SearchState,
    /// The go-to-line field (see `go_to_line`).
    pub(super) go_to_line: GoToLine,
    /// Saving, the dirty flag and baseline conflicts (see `host_editing`).
    pub(super) editing: HostEditing,
    /// Folding (see `folding`).
    pub(super) fold: FoldState,
    /// Read-only fields and secrets (see `protect`).
    pub(super) protect: ProtectState,
    /// Leaving with the keyboard, the caret's note, announcements (see
    /// `a11y`).
    pub(super) a11y: A11yState,
    /// The Edit-as notice (see `edit_as`).
    pub(super) edit_as: EditAsState,
}

impl EditorState {
    /// The state, and an `edit_as` switch to retry once the document can
    /// convert (TOML has no `null`): until then it is shown in the host
    /// language, as a live switch waits.
    pub(super) fn new(init: Init) -> (Self, Option<Language>) {
        let Init {
            host_language,
            edit_language,
            schema,
            schema_error,
            host_text,
            baseline,
            read_only,
            settings,
            host_on_change,
            host_diagnostics,
            protect,
            on_save,
            on_baseline_conflict,
        } = init;
        let settings = LiveSettings::new(settings, schema.is_some());
        let config = Config {
            host_language,
            schema: StoredValue::new(schema.clone()),
            read_only,
            settings,
        };
        // What reaches the host is remembered: a baseline equal to it is the
        // user's own work coming back (see `host_editing`).
        let sent = StoredValue::new(SentTexts::default());
        let host_on_change = host_on_change.map(|callback| noting_sent(sent, callback));
        let (sync, shown, pending) = SyncState::load(
            host_language,
            edit_language,
            config.schema,
            host_text,
            host_on_change,
        );
        let doc = DocState::new(&shown, sync.edit_language);
        let protect = ProtectState::new(
            protect,
            Inputs {
                source: doc.text,
                edit_language: sync.edit_language,
                host_language,
                schema: schema.clone(),
                host_text: sync.host_text,
            },
        );
        let problems = ProblemsState::new(
            problems::Sources {
                shown: shown.clone(),
                text: doc.text,
                host_language,
                schema: schema.clone(),
                schema_error,
                enabled: settings.diagnostics,
                host_diagnostics,
                strings: settings.strings,
            },
            &sync,
            protect,
        );
        let edit_as = EditAsState::new(
            host_language,
            sync.edit_language,
            problems.diagnosed_text,
            sync.host_text,
        );
        let string_bodies = string_bodies(doc.text, sync.edit_language);
        let completion = CompletionState::new(completion::Sources {
            text: doc.text,
            caret: doc.caret,
            edit_language: sync.edit_language,
            schema,
            read_only,
            enabled: settings.completions,
            protect,
            string_bodies,
            strings: settings.strings,
        });
        let editing = HostEditing::new(
            host_editing::Init {
                baseline,
                sent,
                on_save,
                save_shortcut: settings.save_shortcut.into(),
                on_conflict: on_baseline_conflict,
            },
            doc.text,
            host_language,
            config.schema,
            &sync,
        );
        let changes = ChangesState::new(doc.text, editing.shown_baseline, settings.diff);
        let projection = RwSignal::new(Arc::new(Projection::new(&shown, &[])));
        let view = ViewState::new(
            projection,
            rendered_lines(
                doc.text,
                problems.diagnostics,
                changes.diff,
                protect,
                settings.strings,
            ),
            string_bodies,
        );
        let fold = FoldState::new(
            projection,
            view.visible_rows,
            doc.text,
            sync.edit_language,
            settings.folding,
        );
        let state = Self(Box::leak(Box::new(EditorFields {
            search: SearchState::new(doc.text),
            go_to_line: GoToLine::new(),
            typing: TypingState::new(),
            hover: HoverState::new(),
            a11y: A11yState::new(),
            edit_as,
            config,
            doc,
            sync,
            view,
            completion,
            problems,
            changes,
            editing,
            fold,
            protect,
        })));
        (state, pending)
    }

    /// Reads the built-in texts, tracked: a view reading them shows new
    /// texts at once.
    pub(super) fn strings<R>(&self, read: impl FnOnce(&EditorStrings) -> R) -> R {
        self.config.settings.strings.with(read)
    }

    /// The built-in texts as a signal the calling view owns: what a view
    /// that comes and goes reads (see `reactive`). Call it once, while
    /// building the view.
    pub(super) fn strings_signal(&self) -> Signal<Arc<EditorStrings>> {
        self.config.settings.strings_relay.read()
    }

    /// One built-in text as a view reads it: a closure following the
    /// `strings` setting through the view's own signal.
    pub(super) fn text(
        self,
        pick: fn(&EditorStrings) -> &String,
    ) -> impl Fn() -> String + Copy + Send + Sync + 'static {
        let strings = self.strings_signal();
        move || strings.with(|strings| pick(strings).clone())
    }

    /// A built-in text naming the language on screen (`{language}`), as a
    /// view reads it.
    pub(super) fn language_text(
        self,
        pick: fn(&EditorStrings) -> &String,
    ) -> impl Fn() -> String + Copy + Send + Sync + 'static {
        let strings = self.strings_signal();
        move || {
            let language = self.label();
            strings.with(|strings| crate::strings::fill(pick(strings), &[("language", &language)]))
        }
    }

    /// Reads the built-in texts without tracking them, for a message made
    /// once (an announcement).
    pub(super) fn strings_untracked<R>(&self, read: impl FnOnce(&EditorStrings) -> R) -> R {
        self.config.settings.strings.with_untracked(read)
    }

    /// The textarea, in the document's offsets (see `folding`).
    pub(super) fn textarea(&self) -> Option<SourceArea> {
        self.view
            .textarea_ref
            .get_untracked()
            .map(|element| SourceArea::new(element, *self))
    }
}

/// Converts CRLF and CR to LF; the textarea and all line math assume LF.
pub(super) fn normalize_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

/// Lines in `text` (one more than its newlines).
pub(super) fn line_count(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count() + 1
}

pub(super) fn textarea_selection(element: &SourceArea) -> Selection {
    let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
    let end = element
        .selection_end()
        .ok()
        .flatten()
        .map_or(start, |end| end as usize);
    (start, end)
}

/// Selects `start..end`, keeping the caret at the start when `backward`
/// (as a selection made with Shift+Left), so Shift+arrows extend the same end.
pub(super) fn select(element: &SourceArea, start: usize, end: usize, backward: bool) {
    let direction = if backward { "backward" } else { "forward" };
    let _ = element.set_selection_range_with_direction(start as u32, end as u32, direction);
}

/// Whether the selection's caret is at its start.
pub(super) fn is_backward(element: &SourceArea) -> bool {
    element.selection_direction().ok().flatten().as_deref() == Some("backward")
}

/// The selection as (anchor, caret): the caret is the end that moves.
pub(super) fn directed_selection(element: &SourceArea) -> (usize, usize) {
    let (start, end) = textarea_selection(element);
    let backward = element.selection_direction().ok().flatten().as_deref() == Some("backward");
    if backward { (end, start) } else { (start, end) }
}

#[cfg(test)]
mod size_tests {
    use super::EditorState;

    /// Every handler copies the state; a larger copy overflowed the stack of
    /// debug wasm builds (see [`EditorState`]).
    #[test]
    fn the_state_handlers_copy_is_one_pointer() {
        assert_eq!(
            std::mem::size_of::<EditorState>(),
            std::mem::size_of::<usize>()
        );
    }
}
