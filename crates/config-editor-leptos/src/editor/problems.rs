//! The problems shown: the editor's own (syntax, schema, conversion), the
//! host's (see `host_diagnostics`), checked on the text after each pause in
//! typing and moved with the lines edits shift meanwhile; and the problem
//! F8 moved to.

use std::sync::Arc;

use leptos::prelude::*;

use super::diagnostics::{Diagnostic, SchemaBundle, capped, shift_diagnostics, validate_with};
use super::edit_as::EditAsProblem;
use super::host_diagnostics::{
    DiagnosticTarget, HostDiagnostic, HostTargets, RangeText, merge as merge_host,
    place as place_host,
};
use super::languages::Language;
use super::problem_hit::{column_at_cell, covering};
use super::problem_walk::{Stop, stops};
use super::protect::ProtectState;
use super::state::EditorState;
use super::sync::SyncState;
use crate::EditorStrings;
use crate::reactive::later;

/// How long the problem tip stays after the pointer leaves its underline,
/// in milliseconds: time to reach it.
const TIP_GRACE_MS: i32 = 200;

/// The problems part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct ProblemsState {
    /// The document after the typing pause; validation and the Edit-as
    /// conversion read it.
    pub(super) diagnosed_text: RwSignal<String>,
    pub(super) diagnostics: Memo<Vec<Diagnostic>>,
    /// The editor's own problems (syntax, schema, secret placeholders,
    /// conversion), without the host's: what `on_problems` reports.
    pub(super) own: Memo<Vec<Diagnostic>>,
    /// The order F8 walks `diagnostics` in, found when F8 first needs it
    /// after the text or the problems change (not on every press).
    pub(super) walk: Memo<Vec<Stop>>,
    /// The problem F8 moved to, `(line, column)`: its message shows until the
    /// next key, click or edit.
    pub(super) current: RwSignal<Option<(usize, usize)>>,
    /// The problem F8 went to last and the caret it left: from there the
    /// next press goes on (several problems at one place each get a turn).
    pub(super) walked: RwSignal<Option<(Diagnostic, usize)>>,
    /// The place under the pointer (or the focused underline's start),
    /// `(line, column)`: the messages of every problem covering it show in
    /// the problem tip (see `components::diagnostics`).
    pub(super) hovered: RwSignal<Option<(usize, usize)>>,
    /// The pointer is over the problem tip (so leaving the underline for it
    /// keeps it open).
    pub(super) tip_entered: StoredValue<bool>,
    /// Bumped on each hover change, so a hide scheduled before it never lands.
    pub(super) tip_token: StoredValue<u64>,
    /// A hide is scheduled (the pointer left the problems): moving on
    /// does not put it off.
    pub(super) tip_leaving: StoredValue<bool>,
    /// The underline cap left some problems' lines without an underline
    /// (set by the underlines, said in the footer).
    pub(super) underlines_cut: RwSignal<bool>,
    /// The problem tip's element (a second `Ctrl+K Ctrl+I` focuses it).
    pub(super) tip_ref: NodeRef<leptos::html::Div>,
    /// The focus is in the problem tip: it is a dialog then.
    pub(super) tip_focused: RwSignal<bool>,
}

/// What the problems are found from.
pub(super) struct Sources {
    pub(super) shown: String,
    pub(super) text: RwSignal<String>,
    pub(super) host_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) schema_error: Option<super::diagnostics::SchemaFailure>,
    /// The `show_diagnostics` setting.
    pub(super) enabled: Memo<bool>,
    pub(super) host_diagnostics: Signal<Vec<HostDiagnostic>>,
    /// The built-in texts the editor's own problems are worded with.
    pub(super) strings: Memo<EditorStrings>,
}

/// Tells `callback` the editor's own problems on mount and each time they
/// change (the memo holding them notifies only then).
pub(super) fn report_own(state: EditorState, callback: Callback<Vec<Diagnostic>>) {
    let own = state.problems().own;
    Effect::new(move |_| callback.run(own.get()));
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
            strings,
        } = sources;
        let diagnosed_text = RwSignal::new(shown);
        let edit_language = sync.edit_language;
        let host_text = sync.host_text;
        let (load_problems, conversion_problems) = (sync.load_problems, sync.conversion_problems);
        let line_endings = sync.line_endings;
        let validated = Memo::new(move |_| {
            let enabled = enabled.get();
            // The host's placeholders stand for the current secrets.
            let accepted = protect.accepted_tokens();
            let behavior = edit_language.get().behavior();
            strings.with(|strings| {
                let mut found = diagnosed_text.with(|text| {
                    validate_with(
                        text,
                        schema.as_deref(),
                        schema_error.as_ref(),
                        enabled,
                        behavior,
                        &accepted,
                        strings,
                    )
                });
                // A line break the host's language does not read (a lone
                // CR in TOML) fails its reader as the host saves the text.
                if enabled
                    && edit_language.get() == host_language
                    && let Some(problem) =
                        diagnosed_text.with(|text| line_endings.rejected_break(text))
                {
                    return vec![problem];
                }
                if enabled {
                    let worded = |problems: &Vec<EditAsProblem>| -> Vec<Diagnostic> {
                        problems
                            .iter()
                            .map(|problem| problem.worded(strings))
                            .collect()
                    };
                    found.extend(protect.placeholder_pattern_problem(strings));
                    found.extend(load_problems.with(worded));
                    found.extend(conversion_problems.with(worded));
                    let marking = schema
                        .as_deref()
                        .filter(|bundle| bundle.has_marks)
                        .map(|bundle| &bundle.schema);
                    if marking.is_some() || protect.has_placeholders() {
                        found.extend(diagnosed_text.with(|checked| {
                            // The placeholders' places follow the text on
                            // screen; they say which are real only there.
                            let on_screen = text.with_untracked(|shown| shown == checked);
                            // As many as the schema's problems: thousands
                            // of underlines would slow every key.
                            capped(
                                protect.placeholder_problems(
                                    behavior, checked, marking, strings, on_screen,
                                ),
                                strings,
                            )
                        }));
                    }
                }
                found
            })
        });
        // The host's problems, placed on the text validation saw; a new list
        // from the host re-places only these, not the editor's own.
        let host_targets = StoredValue::new(None);
        let host_placed = Memo::new(move |_| {
            if !enabled.get() {
                return Vec::new();
            }
            let edit = edit_language.get();
            host_diagnostics.with(|problems| {
                strings.with(|strings| {
                    diagnosed_text.with(|text| {
                        if edit == host_language {
                            // Nothing kept from a view left.
                            if host_targets.with_value(Option::is_some) {
                                host_targets.set_value(None);
                            }
                            // The placeholders a point on one marks whole.
                            let placeholders = if problems.iter().any(|problem| {
                                matches!(problem.target, DiagnosticTarget::Position(_))
                            }) {
                                protect.placeholder_spans(text)
                            } else {
                                Vec::new()
                            };
                            place_host(
                                edit.behavior(),
                                text,
                                problems,
                                RangeText::OnScreen,
                                &placeholders,
                                strings,
                            )
                        } else {
                            // The host's text matching the text checked (it
                            // follows every conversion back); the fields its
                            // ranges start in kept while it stays the same.
                            let found = host_text.with_value(|host| {
                                HostTargets::cached(
                                    host_targets,
                                    host_language.behavior(),
                                    host,
                                    problems,
                                )
                            });
                            let ranges = RangeText::Host(&found);
                            place_host(edit.behavior(), text, problems, ranges, &[], strings)
                        }
                    })
                })
            })
        });
        // Until validation catches up with an edit, markers move with the
        // lines the edit shifted, so they never sit on the wrong text.
        let diagnostics = Memo::new(move |_| {
            let checked = diagnosed_text.get();
            let found = validated.with(|own| host_placed.with(|host| merge_host(own, host)));
            text.with(|current| shift_diagnostics(&checked, current, &found))
        });
        let walk = Memo::new(move |_| {
            text.with(|current| diagnostics.with(|found| stops(current, found)))
        });
        Self {
            diagnosed_text,
            diagnostics,
            own: validated,
            walk,
            current: RwSignal::new(None),
            walked: RwSignal::new(None),
            hovered: RwSignal::new(None),
            tip_entered: StoredValue::new(false),
            tip_token: StoredValue::new(0),
            tip_leaving: StoredValue::new(false),
            underlines_cut: RwSignal::new(false),
            tip_ref: NodeRef::new(),
            tip_focused: RwSignal::new(false),
        }
    }
}

impl EditorState {
    /// Moves to diagnostic `index`, which must have a position.
    pub(super) fn go_to_diagnostic(&self, index: usize) {
        let position = self.problems().diagnostics.with_untracked(|problems| {
            problems
                .get(index)
                .and_then(|problem| Some((problem.line?, problem.column?)))
        });
        if let Some((line, column)) = position {
            self.focus_at(line, column);
        }
        self.problems().current.set(position);
    }

    /// Hides the message F8 showed, and the one under the pointer.
    pub(super) fn hide_shown_problem(&self) {
        if self.problems().current.get_untracked().is_some() {
            // The tip had the focus: the text takes it back, not the page.
            let focused = self.problems().tip_focused.get_untracked();
            self.problems().current.set(None);
            if focused && let Some(element) = self.textarea() {
                let _ = element.focus_in_place();
            }
        }
        self.hide_problem_tip();
    }

    /// The focus moved from inside the editor (the text, the tip, a
    /// control) `to` another element (or out of the window, `None`): out
    /// of the editor, the tip closes, as the hover card does.
    pub(super) fn tip_follows_focus(&self, to: Option<&web_sys::EventTarget>) {
        use wasm_bindgen::JsCast;
        let inside = self.view().root_ref.get_untracked().is_some_and(|root| {
            to.and_then(|to| to.dyn_ref::<web_sys::Node>())
                .is_some_and(|to| root.contains(Some(to)))
        });
        if !inside {
            self.problems().tip_focused.set(false);
            self.hide_shown_problem();
        }
    }

    /// Shows the problems covering one-based `place` (under the pointer, or
    /// a focused underline's start).
    pub(super) fn hover_problem(&self, place: (usize, usize)) {
        let problems = self.problems();
        problems.tip_token.update_value(|token| *token += 1);
        problems.tip_leaving.set_value(false);
        if problems.hovered.get_untracked() != Some(place) {
            problems.hovered.set(Some(place));
        }
    }

    /// The pointer is over rendered cell `cell` of zero-based `line`: the
    /// tip shows the problems covering the character there, or closes (a
    /// moment later) where none does.
    pub(super) fn point_at_problems(&self, line: usize, cell: usize) {
        match self.problem_place_at(line, cell) {
            Some(place) => self.hover_problem(place),
            None if self.problems().hovered.with_untracked(Option::is_some) => {
                self.leave_problem();
            }
            None => {}
        }
    }

    /// The place (one-based line, column) of rendered cell `cell` of
    /// zero-based `line` when problems cover the character there.
    pub(super) fn problem_place_at(&self, line: usize, cell: usize) -> Option<(usize, usize)> {
        self.doc().text.with_untracked(|text| {
            let line_text = text.split('\n').nth(line).unwrap_or("");
            let place = (line + 1, column_at_cell(line_text, cell));
            self.problems().diagnostics.with_untracked(|problems| {
                covering(problems, place.0, place.1, line_text)
                    .next()
                    .is_some()
                    .then_some(place)
            })
        })
    }

    /// The pointer left an underline: its tip closes a moment later, unless
    /// the pointer reached the tip (to scroll it or click a docs link).
    pub(super) fn leave_problem(&self) {
        let problems = self.problems();
        if problems.tip_leaving.get_value() {
            return;
        }
        problems.tip_leaving.set_value(true);
        problems.tip_token.update_value(|token| *token += 1);
        let token = problems.tip_token.get_value();
        later::after(*self, TIP_GRACE_MS, move |state| {
            let problems = state.problems();
            if problems.tip_token.get_value() != token {
                return;
            }
            problems.tip_leaving.set_value(false);
            if !problems.tip_entered.get_value() {
                state.hide_problem_tip();
            }
        });
    }

    /// Hides the tip of the underline under the pointer.
    pub(super) fn hide_problem_tip(&self) {
        let problems = self.problems();
        problems.tip_token.update_value(|token| *token += 1);
        problems.tip_entered.set_value(false);
        problems.tip_leaving.set_value(false);
        if problems.hovered.with_untracked(Option::is_some) {
            problems.hovered.set(None);
        }
    }
}
