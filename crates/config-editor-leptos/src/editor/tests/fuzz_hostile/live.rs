//! The props a host may change on a mounted editor, changed at random
//! while editing: every switch of `settings` and its theme, `read_only` on
//! and off, the `read_only_paths` and `read_only_ranges` given, taken and
//! changed, and the `value` replaced (with the host's own edit, another
//! document, or the same text). The language, the schema and the secrets
//! are fixed at mount (the JavaScript `update` refuses them), so a case
//! mounts with one of each and the fresh editor below gets the same.
//!
//! After every step: nothing panicked; an edit made while read-only
//! changed nothing; the text shown, the locks (the host's ranges only when
//! given since the last edit: they follow edits), the problems and the
//! folds are those of a fresh editor mounted on the same text with the
//! same settings; and after a new `value` there is nothing to redo, and
//! undoing every step and redoing them gives the text back (the host's
//! value is no step of its own: the steps before take it, see
//! `EditHistory::rebase`).

use std::sync::Arc;

use leptos::prelude::*;

use super::runner::{Cap, Case, Failure, Rng, cases, run};
use super::texts::{document, language, literal, shown};
use crate::editor::edit::Edit;
use crate::editor::keyboard::utf16_to_byte_index;
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::line_endings::editor_text;
use crate::editor::paths::{FieldPath, TextPosition, TextRange};
use crate::editor::protect::ProtectInit;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::state::{EditorState, Init};
use crate::editor::sync::take_host_value;
use crate::{EditorSettings, EditorTheme};

/// One step of a session.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Step {
    /// A switch of `settings` flipped (by [`SWITCHES`]).
    Switch {
        which: u8,
    },
    Theme {
        which: u8,
    },
    ReadOnly {
        on: bool,
    },
    /// The read-only paths: the [`POINTERS`] whose bit is set.
    Paths {
        mask: u8,
    },
    /// The read-only ranges: the whole line `line` (of the text now), or
    /// none.
    Ranges {
        line: Option<u32>,
    },
    /// A new `value`: 0 the text with a line added by the host, 1 another
    /// document, 2 the same text.
    Value {
        kind: u8,
        seed: u32,
    },
    Type {
        at: u32,
        text: &'static str,
    },
    Enter {
        at: u32,
    },
    Cut {
        at: u32,
        to: u32,
    },
    Undo,
    Redo,
    /// A second without input: the next typing starts a new undo step.
    Wait,
}

/// The fields a lock may name.
const POINTERS: [&str; 5] = ["/name", "/port", "/a", "/x_1", "/k"];

/// The schema a case mounts with: none, or one marking `port` read-only
/// and checking `name`'s type.
const SCHEMA: &str =
    r#"{"type": "object", "properties": {"port": {"readOnly": true}, "name": {"type": "string"}}}"#;

const TYPED: [&str; 5] = ["x", "1", " ", "é", ": "];

#[derive(Clone, Debug)]
pub(super) struct LiveCase {
    pub(super) language: &'static str,
    pub(super) schema: bool,
    pub(super) text: String,
    pub(super) steps: Vec<Step>,
}

impl Case for LiveCase {
    fn code(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|step| format!("            Step::{step:?},"))
            .collect();
        format!(
            "#[test]\nfn live_repro() {{\n    use super::live::Step;\n    super::live::replay(super::live::LiveCase {{\n        language: {:?},\n        schema: {},\n        text: {}.into(),\n        steps: vec![\n{}\n        ],\n    }});\n}}",
            self.language,
            self.schema,
            literal(&self.text),
            steps.join("\n"),
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        for index in 0..self.steps.len() {
            let mut steps = self.steps.clone();
            steps.remove(index);
            out.push(Self {
                steps,
                ..self.clone()
            });
        }
        if self.schema {
            out.push(Self {
                schema: false,
                ..self.clone()
            });
        }
        let lines: Vec<&str> = self.text.split_inclusive('\n').collect();
        for line in 0..lines.len() {
            let text: String = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != line)
                .map(|(_, line)| *line)
                .collect();
            out.push(Self {
                text,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.text.len() + self.steps.len() * 1000 + usize::from(self.schema)
    }
}

const HOSTS: [&str; 6] = ["YAML", "JSON", "TOML", "HCL", "XML", "ODD"];

fn random_step(rng: &mut Rng) -> Step {
    let (at, to) = (rng.position(), rng.position());
    match rng.below(14) {
        0 | 1 => Step::Switch {
            which: rng.below(SWITCHES) as u8,
        },
        2 => Step::Theme {
            which: rng.below(THEMES.len()) as u8,
        },
        3 => Step::ReadOnly { on: rng.chance(50) },
        4 => Step::Paths {
            mask: rng.below(32) as u8,
        },
        5 => Step::Ranges {
            line: rng.chance(70).then_some(at),
        },
        6 => Step::Value {
            kind: rng.below(3) as u8,
            seed: rng.position(),
        },
        7 | 8 => Step::Type {
            at,
            text: rng.pick(&TYPED),
        },
        9 => Step::Enter { at },
        10 => Step::Cut { at, to },
        11 => Step::Undo,
        12 => Step::Redo,
        _ => Step::Wait,
    }
}

fn generate(rng: &mut Rng) -> Option<LiveCase> {
    let name = *rng.pick(&HOSTS);
    let text = document(rng, language(name));
    let steps = (0..1 + rng.below(14)).map(|_| random_step(rng)).collect();
    Some(LiveCase {
        language: name,
        schema: rng.chance(40),
        text,
        steps,
    })
}

const SWITCHES: usize = 14;

/// `settings` with switch `which` flipped.
fn flip(settings: &mut EditorSettings, which: u8) {
    let switch = match which as usize % SWITCHES {
        0 => &mut settings.show_toolbar,
        1 => &mut settings.show_footer,
        2 => &mut settings.show_diagnostics,
        3 => &mut settings.show_diff,
        4 => &mut settings.completions,
        5 => &mut settings.auto_trigger_completions,
        6 => &mut settings.hover_docs,
        7 => &mut settings.highlighting,
        8 => &mut settings.auto_closing,
        9 => &mut settings.search,
        10 => &mut settings.go_to_line,
        11 => &mut settings.folding,
        12 => &mut settings.save_shortcut,
        _ => &mut settings.styled,
    };
    *switch = !*switch;
}

const THEMES: [EditorTheme; 4] = [
    EditorTheme::VsCodeDark,
    EditorTheme::VsCodeLight,
    EditorTheme::Dracula,
    EditorTheme::Nord,
];

/// What the host gives, each a signal it may change.
#[derive(Clone, Copy)]
struct Props {
    settings: RwSignal<EditorSettings>,
    read_only: RwSignal<bool>,
    paths: RwSignal<Vec<FieldPath>>,
    ranges: RwSignal<Vec<TextRange>>,
}

/// An editor on `text` with `props`, as the component mounts one.
fn mount(
    case: &LiveCase,
    text: &str,
    props: Props,
    schema: Option<Arc<SchemaBundle>>,
) -> EditorState {
    let host = language(case.language);
    EditorState::new(Init {
        host_language: host,
        edit_language: host,
        schema,
        schema_error: None,
        host_text: text.to_string(),
        baseline: text.to_string(),
        opening: None,
        read_only: props.read_only.into(),
        read_only_reason: Signal::stored(None),
        settings: props.settings.into(),
        host_on_change: None,
        host_diagnostics: Signal::default(),
        protect: ProtectInit {
            paths: Some(props.paths.into()),
            ranges: Some(props.ranges.into()),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
    })
    .0
}

/// A problem as compared: its line, columns and message.
type Problem = (Option<usize>, Option<usize>, Option<usize>, String);

/// What is compared with a fresh editor.
#[derive(Debug, PartialEq)]
struct Seen {
    text: String,
    locks: Vec<(Option<String>, Vec<std::ops::Range<usize>>)>,
    problems: Vec<Problem>,
    folds: Vec<(usize, usize)>,
}

fn seen(state: EditorState, with_ranges: bool) -> Seen {
    let text = state.doc().text.get_untracked();
    state.problems().diagnosed_text.set(text.clone());
    let mut locks: Vec<_> = state.protect().protected.with_untracked(|protected| {
        protected
            .locks
            .iter()
            .filter(|lock| with_ranges || lock.pointer.is_some())
            .map(|lock| (lock.pointer.clone(), lock.ranges.clone()))
            .collect()
    });
    locks.sort_by_key(|(pointer, ranges)| {
        (
            pointer.clone(),
            ranges
                .iter()
                .map(|range| (range.start, range.end))
                .collect::<Vec<_>>(),
        )
    });
    let problems = state.problems().diagnostics.with_untracked(|problems| {
        problems
            .iter()
            .map(|problem| {
                (
                    problem.line,
                    problem.column,
                    problem.end_column,
                    problem.message.clone(),
                )
            })
            .collect()
    });
    let folds = state
        .fold()
        .fold_ranges
        .with_untracked(|folds| folds.iter().map(|fold| (fold.start, fold.end)).collect());
    Seen {
        text,
        locks,
        problems,
        folds,
    }
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset a raw position stands for in `text`, on a character.
fn offset(text: &str, at: u32) -> usize {
    let characters = text.chars().count();
    let index = at as usize % (characters + 1);
    text.chars().take(index).map(char::len_utf16).sum()
}

fn replace(state: EditorState, (start, end): (usize, usize), typed: &str, kind: &str, time: f64) {
    let before = state.doc().text.get_untracked();
    let from = utf16_to_byte_index(&before, start);
    let to = utf16_to_byte_index(&before, end);
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    if text == before {
        return;
    }
    let caret = start + units(typed);
    state.doc().caret.set(start);
    state.commit(Edit::input(
        before,
        (start, end),
        text,
        (caret, caret),
        kind.into(),
        time,
    ));
}

/// The whole line `line` of `text` as a range.
fn line_range(text: &str, line: u32) -> TextRange {
    let lines: Vec<&str> = text.split('\n').collect();
    let index = line as usize % lines.len();
    TextRange::new(
        TextPosition::new(index + 1, 1),
        TextPosition::new(index + 1, lines[index].chars().count() + 1),
    )
}

/// Undoes every step, then redoes them all; the text after, and whether
/// each undo found a step to redo.
fn round_trip(state: EditorState) -> String {
    let mut undone = 0;
    while undone < 200 {
        match state.history_edit(false) {
            Some(step) => {
                state.commit(step);
                undone += 1;
            }
            None => break,
        }
    }
    for _ in 0..undone {
        match state.history_edit(true) {
            Some(step) => {
                state.commit(step);
            }
            None => break,
        }
    }
    state.doc().text.get_untracked()
}

pub(super) fn check(case: &LiveCase) -> Result<(), Failure> {
    let schema = case
        .schema
        .then(|| Arc::new(build_schema(SCHEMA).expect("the schema builds")));
    let _owner = Owner::new();
    _owner.set();
    let props = Props {
        settings: RwSignal::new(EditorSettings::default()),
        read_only: RwSignal::new(false),
        paths: RwSignal::new(Vec::new()),
        ranges: RwSignal::new(Vec::new()),
    };
    let state = mount(case, &case.text, props, schema.clone());
    // The host's ranges are compared while no edit has moved them.
    let mut ranges_placed = true;
    let name = case.language;
    for (index, step) in case.steps.iter().enumerate() {
        let time = 2000.0 * (index + 1) as f64;
        let before = state.doc().text.get_untracked();
        let edit = matches!(
            step,
            Step::Type { .. } | Step::Enter { .. } | Step::Cut { .. } | Step::Undo | Step::Redo
        );
        match step {
            Step::Switch { which } => props.settings.update(|settings| flip(settings, *which)),
            Step::Theme { which } => props
                .settings
                .update(|settings| settings.theme = THEMES[*which as usize % THEMES.len()]),
            Step::ReadOnly { on } => props.read_only.set(*on),
            Step::Paths { mask } => props.paths.set(
                POINTERS
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| mask & (1 << bit) != 0)
                    .map(|(_, pointer)| FieldPath::from(*pointer))
                    .collect(),
            ),
            Step::Ranges { line } => {
                props.ranges.set(
                    line.map(|line| vec![line_range(&before, line)])
                        .unwrap_or_default(),
                );
                // As the component's effect does when they change.
                state.place_ranges();
                ranges_placed = true;
            }
            Step::Value { kind, seed } => {
                let value = match kind % 3 {
                    0 => format!("{before}\n"),
                    1 => document(&mut Rng::new(u64::from(*seed)), language(name)),
                    _ => before.clone(),
                };
                state.sync().line_endings.note(&value);
                take_host_value(state, editor_text(value));
                // The host's ranges are placed again in a new text.
                ranges_placed |= state.doc().text.get_untracked() != before;
            }
            Step::Type { at, text } => {
                let at = offset(&before, *at);
                replace(state, (at, at), text, "insertText", time);
            }
            Step::Enter { at } => {
                let at = offset(&before, *at);
                state.doc().caret.set(at);
                enter_at(state, (at, at), Pressed::Enter, Some(time));
            }
            Step::Cut { at, to } => {
                let (a, b) = (offset(&before, *at), offset(&before, *to));
                replace(state, (a.min(b), a.max(b)), "", "deleteByCut", time);
            }
            Step::Undo | Step::Redo => {
                if let Some(edit) = state.history_edit(matches!(step, Step::Redo)) {
                    state.commit(edit);
                }
            }
            Step::Wait => state.doc().history.update(|history| history.seal()),
        }
        let after = state.doc().text.get_untracked();
        let at = || format!("step {index} {step:?}: {}", shown(&after));
        if edit && after != before {
            ranges_placed = false;
            if props.read_only.get_untracked() {
                return Err(Failure::new(
                    format!("an edit goes through while read-only ({name})"),
                    format!("{} after {}", at(), shown(&before)),
                ));
            }
        }
        if matches!(step, Step::Value { .. }) && after != before {
            if state
                .doc()
                .history
                .with_untracked(|history| history.can_redo())
            {
                return Err(Failure::new(
                    format!("a new value leaves steps to redo ({name})"),
                    at(),
                ));
            }
            // Locks may refuse a step redone over the host's change.
            let unlocked = !case.schema
                && props.paths.with_untracked(Vec::is_empty)
                && props.ranges.with_untracked(Vec::is_empty);
            if unlocked && !props.read_only.get_untracked() {
                let back = round_trip(state);
                if back != after {
                    return Err(Failure::new(
                        format!(
                            "after a new value, undoing every step and redoing them does not give the text back ({name})"
                        ),
                        format!("{}: back to {}", at(), shown(&back)),
                    ));
                }
            }
        }
        // A fresh editor on the same text with the same props.
        let fresh_props = Props {
            settings: RwSignal::new(props.settings.get_untracked()),
            read_only: RwSignal::new(props.read_only.get_untracked()),
            paths: RwSignal::new(props.paths.get_untracked()),
            ranges: RwSignal::new(if ranges_placed {
                props.ranges.get_untracked()
            } else {
                Vec::new()
            }),
        };
        let fresh_owner = Owner::new();
        let mut new = fresh_owner.with(|| {
            let fresh = mount(case, &after, fresh_props, schema.clone());
            seen(fresh, ranges_placed)
        });
        fresh_owner.cleanup();
        let mut live = seen(state, ranges_placed);
        // A field held because the text reads it otherwise than the host's
        // text did (a path named after the user renamed its key) is known
        // to the live editor only: a fresh one never saw the host's text.
        let held: Vec<String> = untrack(|| {
            state.strings_untracked(|strings| {
                state
                    .protect()
                    .held_problems(state.behavior(), &after, strings)
                    .into_iter()
                    .map(|problem| problem.message)
                    .collect()
            })
        });
        live.problems.retain(|(.., message)| {
            !held.contains(message) || new.problems.iter().any(|(.., known)| known == message)
        });
        // Such a field's lock is the pinned text followed by position (the
        // text at its path now is the user's, the way back must stay free);
        // every other lock is placed by its path, as in a fresh editor.
        let held_fields = state
            .protect()
            .protected
            .with_untracked(|protected| protected.held.clone());
        let not_held = |(pointer, _): &(Option<String>, _)| {
            pointer
                .as_ref()
                .is_none_or(|pointer| !held_fields.contains(pointer))
        };
        live.locks.retain(not_held);
        new.locks.retain(not_held);
        // Locks are carried through a text that does not parse (within one
        // text from the host); a fresh editor has nothing to carry.
        // The host's ranges followed through edits are not where it gave
        // them (and lock the fields they cover by name too).
        let followed = !ranges_placed && !props.ranges.with_untracked(Vec::is_empty);
        if followed || language(name).parse(&after).is_err() {
            live.locks.clear();
            new.locks.clear();
        }
        if live != new {
            let what = if live.text != new.text {
                "the text shown"
            } else if live.locks != new.locks {
                "the locks"
            } else if live.problems != new.problems {
                "the problems"
            } else {
                "the folds"
            };
            return Err(Failure::new(
                format!(
                    "{what} differ from a fresh editor's after {} ({name})",
                    kind(step)
                ),
                format!(
                    "{}\nlive {live:?}\nfresh {new:?}\nheld {:?}",
                    at(),
                    state
                        .protect()
                        .protected
                        .with_untracked(|protected| protected.held.clone())
                ),
            ));
        }
    }
    Ok(())
}

/// The kind of a step, for a class name.
fn kind(step: &Step) -> &'static str {
    match step {
        Step::Switch { .. } | Step::Theme { .. } => "a setting",
        Step::ReadOnly { .. } => "read-only",
        Step::Paths { .. } => "new paths",
        Step::Ranges { .. } => "new ranges",
        Step::Value { .. } => "a new value",
        _ => "an edit",
    }
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: LiveCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn props_changed_live_agree_with_a_fresh_editor() {
    run("live", cases(120), Cap::Here, generate, check);
}
