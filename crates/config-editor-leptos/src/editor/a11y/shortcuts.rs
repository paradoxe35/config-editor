//! The keys that work in the text, named on the textarea
//! (`aria-keyshortcuts`) so a screen reader can tell them to its user:
//! the single chords of the README's table (`keyboard::KEYS`; the
//! attribute cannot spell the two-step `Ctrl+K Ctrl+I`), only those the
//! editor takes in this build and with these settings: a key it leaves to
//! the browser (find with `search` off, save without `on_save`) is not
//! named. Set here rather than in the view: a tachys element holds at most
//! 26 attributes, and the textarea's are taken.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::keyboard::{When, aria_chords, gated_chords};

/// The settings and state the chords depend on.
#[derive(Clone, Copy, Default)]
pub(super) struct Context {
    pub(super) editable: bool,
    pub(super) suggestions: bool,
    pub(super) find: bool,
    pub(super) go_to_line: bool,
    pub(super) save: bool,
    pub(super) problems: bool,
    pub(super) changes: bool,
    pub(super) folding: bool,
    pub(super) notice: bool,
    pub(super) documents: bool,
}

impl Context {
    pub(super) fn allows(self, when: When) -> bool {
        match when {
            When::Always => true,
            When::Editable => self.editable,
            When::Suggestions => self.editable && self.suggestions,
            When::Find => self.find,
            When::Replace => self.find && self.editable,
            When::GoToLine => self.go_to_line,
            When::Save => self.save,
            When::Problems => self.problems,
            When::Changes => self.changes,
            When::Folding => self.folding,
            When::Notice => self.notice,
            When::Documents => self.documents,
            When::Elsewhere => false,
        }
    }
}

/// The chords in effect in `context`, space-separated as
/// `aria-keyshortcuts` is.
pub(super) fn chords_in(context: Context) -> String {
    let mut out: Vec<String> = Vec::new();
    for (chord, when) in gated_chords() {
        if context.allows(when) {
            for spelled in aria_chords(chord) {
                if !out.contains(&spelled) {
                    out.push(spelled);
                }
            }
        }
    }
    out.join(" ")
}

/// The context now (tracked).
fn context(state: EditorState, in_documents: bool) -> Context {
    let settings = state.config().settings;
    Context {
        editable: !state.config().read_only.get(),
        suggestions: settings.completions.get(),
        find: cfg!(feature = "search") && settings.search.get(),
        go_to_line: cfg!(feature = "search") && settings.go_to_line.get(),
        save: state.editing().on_save.is_some() && settings.save_shortcut.get(),
        problems: settings.diagnostics.get(),
        changes: cfg!(feature = "diff") && settings.diff.get(),
        folding: cfg!(feature = "folding") && settings.folding.get(),
        notice: state.editing().conflict.with(Option::is_some),
        documents: in_documents,
    }
}

/// Whether `chord` (as the keys table writes it, `Ctrl+M`) works in the
/// text now: the gate that names it in `aria-keyshortcuts`.
pub(super) fn works_now(state: EditorState, chord: &str) -> bool {
    let in_documents = state
        .view()
        .textarea_ref
        .get_untracked()
        .is_some_and(|element| {
            element
                .closest(".config-editor-documents")
                .ok()
                .flatten()
                .is_some()
        });
    let context = untrack(|| context(state, in_documents));
    gated_chords()
        .into_iter()
        .any(|(named, when)| named == chord && context.allows(when))
}

/// Keeps the textarea's `aria-keyshortcuts` in step with the settings.
pub(in crate::editor) fn connect_shortcuts(state: EditorState) {
    Effect::new(move |_| {
        let Some(element) = state.view().textarea_ref.get() else {
            return;
        };
        let in_documents = element
            .closest(".config-editor-documents")
            .ok()
            .flatten()
            .is_some();
        let _ = element.set_attribute(
            "aria-keyshortcuts",
            &chords_in(context(state, in_documents)),
        );
    });
}
