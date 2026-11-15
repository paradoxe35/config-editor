//! Undo and redo under random edit sequences, in every language and
//! Edit-as view: undo all the way gives back the text (and the language)
//! the editor opened with exactly, redo all the way the last text; undo
//! and redo interleaved with edits never panic, keep the caret in the
//! text and land only on texts the editor showed; and with no undo in
//! between, the steps are those `history` documents (typing grouped while
//! it goes on where it stopped within the window, every command its own
//! step).

use leptos::prelude::*;

use super::actions::{Action, Context, apply, caret_in_range, code, random_action};
use super::docs::{CUSTOM, LANGUAGES, document, literal, smaller_documents};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::history::UNDO_GROUPING_WINDOW_MS;
use crate::editor::languages::Language;
use crate::editor::save_properties::schema;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

#[derive(Clone, Debug)]
pub(super) struct UndoCase {
    pub(super) host: Language,
    pub(super) view: Option<Language>,
    pub(super) schema: bool,
    pub(super) text: String,
    pub(super) actions: Vec<Action>,
    /// Undo and redo among the edits (else the grouping is checked).
    pub(super) interleaved: bool,
}

fn language_code(language: Language) -> String {
    super::docs::language_code(language)
}

impl Case for UndoCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn undo_repro() {{\n    use super::actions::Action::*;\n    super::undo::replay(super::undo::UndoCase {{\n        host: {},\n        view: {},\n        schema: {},\n        text: {}.into(),\n        interleaved: {},\n        actions: vec![\n{}\n        ],\n    }});\n}}",
            language_code(self.host),
            self.view.map_or("None".into(), |view| format!(
                "Some({})",
                language_code(view)
            )),
            self.schema,
            literal(&self.text),
            self.interleaved,
            code(&self.actions)
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        // Halves first, then one edit at a time.
        let count = self.actions.len();
        if count > 1 {
            for half in [&self.actions[..count / 2], &self.actions[count / 2..]] {
                out.push(Self {
                    actions: half.to_vec(),
                    ..self.clone()
                });
            }
        }
        for index in 0..count {
            let mut actions = self.actions.clone();
            actions.remove(index);
            out.push(Self {
                actions,
                ..self.clone()
            });
        }
        if self.schema {
            out.push(Self {
                schema: false,
                ..self.clone()
            });
        }
        if self.view.is_some() {
            out.push(Self {
                view: None,
                ..self.clone()
            });
        }
        for text in smaller_documents(self.host, &self.text) {
            out.push(Self {
                text,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.actions.len() * 10_000
            + self.text.len()
            + usize::from(self.schema)
            + usize::from(self.view.is_some())
    }
}

/// A case in one of `languages`; Edit-as views and switches only among
/// the built-in ones (`views`).
fn generate(
    rng: &mut Rng,
    languages: &[Language],
    interleaved: bool,
    views: bool,
) -> Option<UndoCase> {
    let host = *rng.pick(languages);
    let view = if views {
        rng.chance(35)
            .then(|| *rng.pick(&LANGUAGES))
            .filter(|view| *view != host)
    } else {
        None
    };
    let text = document(rng, host);
    let count = 4 + rng.below(20);
    let actions = (0..count)
        .map(|_| random_action(rng, interleaved, views))
        .collect();
    Some(UndoCase {
        host,
        view,
        schema: rng.chance(30),
        text,
        actions,
        interleaved,
    })
}

fn mount(case: &UndoCase) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(case.host, &case.text);
    editor.edit_as = case.view;
    if case.schema {
        editor.schema = Some(schema());
    }
    editor.mount()
}

fn shown(state: EditorState) -> (String, Language) {
    (
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    )
}

fn where_(case: &UndoCase) -> String {
    match case.view {
        Some(view) => format!("{} host as {view}", case.host),
        None => case.host.to_string(),
    }
}

/// The kind of an edit, for class names.
fn kind(action: &Action) -> String {
    let debug = format!("{action:?}");
    debug
        .split([' ', '{'])
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The undo group an edit's input continues, as `history::record_input`
/// reads its input type; `None` for a command (always its own step).
#[derive(Clone, Copy, PartialEq)]
enum Group {
    Typing,
    Deleting,
}

fn group(action: &Action) -> Option<Group> {
    match action {
        Action::Type { .. } | Action::Enter { .. } => Some(Group::Typing),
        Action::Backspace { .. } => Some(Group::Deleting),
        _ => None,
    }
}

/// Whether the model can tell the steps `action` makes (a pair typed or
/// a selection wrapped follows rules of its own, see `history`).
fn modelled(action: &Action) -> bool {
    !matches!(action, Action::Pair { .. } | Action::Complete { .. })
}

pub(super) fn check(case: &UndoCase) -> Result<(), Failure> {
    let (_owner, state) = mount(case);
    let at = where_(case);
    let original = shown(state);
    let mut context = Context::default();
    let mut seen = vec![original.clone()];
    // The model of the steps: the texts each step returns to, newest last.
    let mut expected: Vec<(String, Language)> = Vec::new();
    let mut open: Option<(Group, f64, usize)> = None;
    let mut modelling = !case.interleaved;
    let mut tip = original.clone();
    for (index, action) in case.actions.iter().enumerate() {
        let before = shown(state);
        apply(state, action, &mut context);
        let after = shown(state);
        if !caret_in_range(state) {
            return Err(Failure::new(
                format!("caret past the text's end after {} ({at})", kind(action)),
                format!(
                    "after edit {index} {action:?} the caret is {} in {:?}",
                    state.doc().caret.get_untracked(),
                    after.0
                ),
            ));
        }
        seen.push(after.clone());
        // The newest step's text: what redo all comes back to.
        if !matches!(action, Action::Undo | Action::Redo) && after != before {
            tip = after.clone();
        }
        if !modelling {
            continue;
        }
        if !modelled(action) {
            modelling = false;
            continue;
        }
        if matches!(action, Action::Wait) {
            open = None;
        }
        if before == after {
            continue;
        }
        let caret_after = state.doc().caret.get_untracked();
        let this = group(action);
        // The edit's own selection: the caret where it was placed.
        let selection_before = super::docs::offset(&before.0, position(action));
        let continues = match (this, open) {
            (Some(group), Some((open_group, time, step_after))) => {
                group == open_group
                    && context.time - time <= UNDO_GROUPING_WINDOW_MS
                    && step_after == selection_before
            }
            _ => false,
        };
        if !continues {
            expected.push(before.clone());
        } else if expected.last() == Some(&after) {
            // The step grew back to the text it started from: no step.
            expected.pop();
            open = None;
            continue;
        }
        open = this.map(|group| (group, context.time, caret_after));
    }
    let last = tip;
    // Undo all the way.
    let mut landings = Vec::new();
    for _ in 0..5000 {
        let Some(step) = state.history_edit(false) else {
            break;
        };
        let from = shown(state);
        state.commit(step);
        let now = shown(state);
        if now == from {
            return Err(Failure::new(
                format!("an undo step changes nothing ({at})"),
                format!("undo on {}:\n{}", now.1, now.0),
            ));
        }
        if !caret_in_range(state) {
            return Err(Failure::new(
                format!("caret past the text's end after undo ({at})"),
                format!("caret {} in {:?}", state.doc().caret.get_untracked(), now.0),
            ));
        }
        if !seen.contains(&now) {
            return Err(Failure::new(
                format!("undo lands on a text never shown ({at})"),
                format!("landed on {}:\n{}", now.1, now.0),
            ));
        }
        landings.push(now);
    }
    let undone = shown(state);
    if undone != original {
        return Err(Failure::new(
            format!("undo all does not give back the opened text ({at})"),
            format!(
                "opened ({}):\n{}\nafter undo all ({}):\n{}\nlast ({}):\n{}",
                original.1, original.0, undone.1, undone.0, last.1, last.0
            ),
        ));
    }
    if modelling {
        let mut wanted = expected.clone();
        wanted.reverse();
        if landings != wanted {
            let show = |texts: &[(String, Language)]| {
                texts
                    .iter()
                    .map(|(text, language)| format!("[{language}] {text:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return Err(Failure::new(
                format!("undo steps differ from the documented grouping ({at})"),
                format!(
                    "undo landed on:\n{}\nthe grouping gives:\n{}",
                    show(&landings),
                    show(&wanted)
                ),
            ));
        }
    }
    // Redo all the way.
    for _ in 0..5000 {
        let Some(step) = state.history_edit(true) else {
            break;
        };
        state.commit(step);
        if !caret_in_range(state) {
            return Err(Failure::new(
                format!("caret past the text's end after redo ({at})"),
                String::new(),
            ));
        }
    }
    let redone = shown(state);
    if redone != last {
        return Err(Failure::new(
            format!("redo all does not give back the last text ({at})"),
            format!(
                "last ({}):\n{}\nafter redo all ({}):\n{}",
                last.1, last.0, redone.1, redone.0
            ),
        ));
    }
    Ok(())
}

/// The raw position an edit was made at (for the grouping model).
fn position(action: &Action) -> u32 {
    match *action {
        Action::Type { at, .. } | Action::Backspace { at, .. } | Action::Enter { at } => at,
        _ => 0,
    }
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: UndoCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn undo_all_and_redo_all_give_back_the_texts_under_random_edits() {
    run(
        "undo-interleaved",
        cases(300),
        &|rng| generate(rng, &LANGUAGES, true, true),
        &check,
    );
}

#[test]
fn undo_steps_follow_the_documented_grouping_under_random_edits() {
    run(
        "undo-grouping",
        cases(300),
        &|rng| generate(rng, &LANGUAGES, false, true),
        &check,
    );
}

#[test]
fn undo_all_and_redo_all_give_back_the_texts_in_custom_languages() {
    run(
        "undo-interleaved-custom",
        cases(300),
        &|rng| generate(rng, &CUSTOM, true, false),
        &check,
    );
}

#[test]
fn undo_steps_follow_the_documented_grouping_in_custom_languages() {
    run(
        "undo-grouping-custom",
        cases(300),
        &|rng| generate(rng, &CUSTOM, false, false),
        &check,
    );
}
