//! The problems shown: the editor's own (syntax, schema, conversion), the
//! host's (see `host_diagnostics`), checked on the text after each pause in
//! typing and following the edits made meanwhile; and the problem
//! F8 moved to (its tip is a popup's, see `popups`).

use std::sync::Arc;

use leptos::prelude::*;

use super::diagnostics::{
    Diagnostic, capped, follow_diagnostics, host_problems, validate_keyed, with_host_problems,
};
use super::edit::Change;
use super::edit_as::{EditAsProblem, EditAsText};
use super::host_diagnostics::{
    DiagnosticTarget, HostDiagnostic, HostTargets, RangeText, merge as merge_host,
    place as place_host,
};
use super::languages::Language;
use super::problem_walk::{Stop, stops};
use super::protect::ProtectState;
use super::schema::SchemaBundle;
use super::state::EditorState;
use super::sync::SyncState;
use crate::EditorStrings;

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
    /// The underline cap left some problems' lines without an underline
    /// (set by the underlines, said in the footer).
    pub(super) underlines_cut: RwSignal<bool>,
    /// The edits made since `diagnosed_text` was checked, composed: what
    /// moves the problems found on it onto the text on screen.
    since_check: StoredValue<Option<SinceCheck>>,
}

/// The change from the text last checked to the text on screen.
struct SinceCheck {
    /// Made of a text this long (the one checked).
    from_len: usize,
    change: Change,
}

/// What the problems are found from.
pub(super) struct Sources {
    pub(super) shown: String,
    pub(super) text: RwSignal<String>,
    pub(super) host_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) schema_error: Option<String>,
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
        let (load_problems, conversion_problems) = (sync.load_problems, sync.conversion_problems);
        let line_endings = sync.line_endings;
        let (synced, session) = (sync.synced, sync.session);
        let host_schema = schema.clone();
        let checked = Memo::new(move |_| {
            let enabled = enabled.get();
            // The host's placeholders stand for the current secrets.
            let accepted = protect.accepted_tokens();
            let behavior = edit_language.get().behavior();
            strings.with(|strings| {
                let (mut found, keys) = diagnosed_text.with(|text| {
                    validate_keyed(
                        text,
                        schema.as_deref(),
                        schema_error.as_deref(),
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
                    return (vec![problem], Vec::new());
                }
                if enabled {
                    let worded = |problems: &Vec<EditAsProblem>| -> Vec<Diagnostic> {
                        problems
                            .iter()
                            .map(|problem| problem.worded(strings))
                            .collect()
                    };
                    found.extend(protect.placeholder_pattern_problem(strings));
                    found.extend(
                        diagnosed_text.with(|text| protect.held_problems(behavior, text, strings)),
                    );
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
                (found, keys)
            })
        });
        // The host's own schema problems while the view cannot show its
        // text exactly (see `diagnostics::host_reading`): checked each time
        // the host's text changes, placed in the view's text it matches.
        let host_checked = Memo::new(move |_| {
            synced.track();
            let edit = edit_language.get();
            let schema = host_schema
                .as_deref()
                .filter(|_| enabled.get() && edit != host_language)?;
            let inexact = load_problems.with(|problems| {
                problems
                    .iter()
                    .any(|problem| matches!(problem.text, EditAsText::EditCannotShow { .. }))
            });
            if !inexact {
                return None;
            }
            let accepted = protect.accepted_tokens();
            let view = session.with_value(|session| session.view.to_string());
            strings.with(|strings| {
                session.with_value(|session| {
                    let host: &str = &session.host;
                    host_problems(
                        host,
                        host_language.behavior(),
                        &view,
                        edit.behavior(),
                        schema,
                        &accepted,
                        strings,
                    )
                })
            })
        });
        let validated = Memo::new(move |_| {
            checked.with(|(own, keys)| {
                host_checked.with(|host| match host {
                    Some(host) => strings.with(|strings| {
                        diagnosed_text
                            .with(|text| with_host_problems(own, keys, host, text, strings))
                    }),
                    None => own.clone(),
                })
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
                            let found = session.with_value(|session| {
                                let host: &str = &session.host;
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
        // Until validation catches up with the edits, markers follow them
        // (see `follow_problems`), so they never sit on other text. Should
        // the edits since not lead to the text on screen, the markers with
        // a place wait for the next check.
        let since_check = StoredValue::new(None);
        let diagnostics = Memo::new(move |_| {
            let found = validated.with(|own| host_placed.with(|host| merge_host(own, host)));
            diagnosed_text.with(|checked| {
                text.with(|current| {
                    if checked == current {
                        return found;
                    }
                    since_check.with_value(|since: &Option<SinceCheck>| match since {
                        Some(since)
                            if since.from_len == checked.len()
                                && since.change.len_after() == current.len() =>
                        {
                            follow_diagnostics(&since.change, checked, current, &found)
                        }
                        _ => found
                            .into_iter()
                            .filter(|problem| problem.line.is_none())
                            .collect(),
                    })
                })
            })
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
            underlines_cut: RwSignal::new(false),
            since_check,
        }
    }
}

impl EditorState {
    /// Notes that `change` (an edit's [`Edit::mapping`](super::edit::Edit))
    /// is about to make `before`, the document, into the text after it, as
    /// the edit pipeline writes it: the problems found on the text last
    /// checked follow it until the next check. Before the text is written,
    /// so the problems read with the new text follow it.
    pub(super) fn follow_problems(&self, change: &Change, before: &str) {
        let problems = self.problems();
        let fresh = problems
            .diagnosed_text
            .with_untracked(|checked| checked == before);
        problems.since_check.update_value(|since| {
            *since = if fresh {
                Some(SinceCheck {
                    from_len: before.len(),
                    change: change.clone(),
                })
            } else {
                since.take().and_then(|since| {
                    Some(SinceCheck {
                        change: since.change.compose(change)?,
                        ..since
                    })
                })
            };
        });
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
}
