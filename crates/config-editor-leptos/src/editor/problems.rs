//! The problems shown: the editor's own (syntax, schema, conversion), the
//! host's (see `host_diagnostics`), checked on the text after each pause in
//! typing and moved with the lines edits shift meanwhile; and the problem
//! F8 moved to.

use std::sync::Arc;

use leptos::prelude::*;

use super::diagnostics::{Diagnostic, SchemaBundle, shift_diagnostics, validate_with};
use super::host_diagnostics::{HostDiagnostic, merge as merge_host, place as place_host};
use super::languages::Language;
use super::protect::{ProtectState, unknown_placeholders};
use super::state::EditorState;
use super::sync::SyncState;

/// The problems part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct ProblemsState {
    /// The document after the typing pause; validation and the Edit-as
    /// conversion read it.
    pub(super) diagnosed_text: RwSignal<String>,
    pub(super) diagnostics: Memo<Vec<Diagnostic>>,
    /// The problem F8 moved to, `(line, column)`: its message shows until the
    /// next key, click or edit.
    pub(super) current: RwSignal<Option<(usize, usize)>>,
}

/// What the problems are found from.
pub(super) struct Sources {
    pub(super) shown: String,
    pub(super) text: RwSignal<String>,
    pub(super) host_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) schema_error: Option<String>,
    /// The `show_diagnostics` prop.
    pub(super) enabled: bool,
    pub(super) host_diagnostics: Signal<Vec<HostDiagnostic>>,
}

impl ProblemsState {
    pub(super) fn new(sources: Sources, sync: &SyncState, protect: ProtectState) -> Self {
        let Sources {
            shown,
            text,
            host_language,
            schema,
            schema_error,
            enabled,
            host_diagnostics,
        } = sources;
        let diagnosed_text = RwSignal::new(shown);
        let edit_language = sync.edit_language;
        let (load_problems, conversion_problems) = (sync.load_problems, sync.conversion_problems);
        let validated = Memo::new(move |_| {
            // The host's placeholders stand for the current secrets.
            let accepted = protect.secrets.known_tokens();
            let behavior = edit_language.get().behavior();
            let mut found = diagnosed_text.with(|text| {
                validate_with(
                    text,
                    schema.as_deref(),
                    schema_error.as_deref(),
                    enabled,
                    behavior,
                    &accepted,
                )
            });
            if enabled {
                found.extend(load_problems.get());
                found.extend(conversion_problems.get());
                if let Some(bundle) = schema.as_deref().filter(|bundle| bundle.has_marks) {
                    found.extend(diagnosed_text.with(|text| {
                        protect.secrets.matcher.with_value(|matcher| {
                            protect.secrets.known.with(|known| {
                                unknown_placeholders(behavior, text, matcher, known, &bundle.schema)
                            })
                        })
                    }));
                }
            }
            found
        });
        // The host's problems, placed on the text validation saw; a new list
        // from the host re-places only these, not the editor's own.
        let host_placed = Memo::new(move |_| {
            if !enabled {
                return Vec::new();
            }
            let edit = edit_language.get();
            host_diagnostics.with(|problems| {
                diagnosed_text
                    .with(|text| place_host(edit.behavior(), text, problems, edit == host_language))
            })
        });
        // Until validation catches up with an edit, markers move with the
        // lines the edit shifted, so they never sit on the wrong text.
        let diagnostics = Memo::new(move |_| {
            let checked = diagnosed_text.get();
            let found = validated.with(|own| host_placed.with(|host| merge_host(own, host)));
            text.with(|current| shift_diagnostics(&checked, current, &found))
        });
        Self {
            diagnosed_text,
            diagnostics,
            current: RwSignal::new(None),
        }
    }
}

impl EditorState {
    /// Moves to diagnostic `index`, which must have a position.
    pub(super) fn go_to_diagnostic(&self, index: usize) {
        let position = self.problems.diagnostics.with_untracked(|problems| {
            problems
                .get(index)
                .and_then(|problem| Some((problem.line?, problem.column?)))
        });
        if let Some((line, column)) = position {
            self.focus_at(line, column);
        }
        self.problems.current.set(position);
    }

    /// Hides the message F8 showed.
    pub(super) fn hide_shown_problem(&self) {
        if self.problems.current.get_untracked().is_some() {
            self.problems.current.set(None);
        }
    }
}
