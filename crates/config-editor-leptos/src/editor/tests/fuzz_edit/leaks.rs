//! A random edit sequence repeated in cycles keeps no memory (P5): typing,
//! paste, cut, undo and redo, Edit-as switches, Replace all, folds and
//! pauses, then the document put back as it opened (the history cleared,
//! the folds opened), over and over. Measured as `renamed_moves_memory`
//! measures one sequence: the live allocations of the case's own thread
//! over two runs of `cycles` cycles after a warm-up. A leak keeps memory
//! in both runs; a process-wide cache another test fills or frees
//! meanwhile shows in one at most.
//!
//! `FUZZ_LEAK_CYCLES` sets the cycles of each run (default 20; the
//! warm-up is half as many again), `FUZZ_LEAK_CASES` the cases (default 100).

use leptos::prelude::*;

use super::actions::{Action, Context, apply, code, pause, random_action};
use super::docs::{CUSTOM, LANGUAGES, document, language_code, literal, smaller_documents, units};
use super::runner::{Case, Failure, Rng, run};
use crate::editor::edit::{Edit, take_trace};
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::sync::switch_to;
use crate::editor::test_state::TestEditor;
use crate::live_allocations::live;

#[derive(Clone, Debug)]
pub(super) struct LeakCase {
    pub(super) host: Language,
    pub(super) view: Option<Language>,
    pub(super) text: String,
    pub(super) actions: Vec<Action>,
}

impl Case for LeakCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn leak_repro() {{\n    use super::actions::Action::*;\n    super::leaks::replay(super::leaks::LeakCase {{\n        host: {},\n        view: {},\n        text: {}.into(),\n        actions: vec![\n{}\n        ],\n    }});\n}}",
            language_code(self.host),
            self.view.map_or("None".into(), |view| format!(
                "Some({})",
                language_code(view)
            )),
            literal(&self.text),
            code(&self.actions)
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        // Each try runs every cycle: halves and single edits only, then the
        // view and the document.
        let mut out = Vec::new();
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
        if self.view.is_some() {
            out.push(Self {
                view: None,
                ..self.clone()
            });
        }
        out.extend(
            smaller_documents(self.host, &self.text)
                .into_iter()
                .take(4)
                .map(|text| Self {
                    text,
                    ..self.clone()
                }),
        );
        out
    }

    fn size(&self) -> usize {
        self.actions.len() * 10_000 + self.text.len() + usize::from(self.view.is_some())
    }
}

fn generate(rng: &mut Rng) -> Option<LeakCase> {
    let custom = rng.chance(20);
    let host = if custom {
        *rng.pick(&CUSTOM)
    } else {
        *rng.pick(&LANGUAGES)
    };
    let view = (!custom && rng.chance(35))
        .then(|| *rng.pick(&LANGUAGES))
        .filter(|view| *view != host);
    let text = document(rng, host);
    let actions = (0..6 + rng.below(14))
        .map(|_| random_action(rng, true, !custom))
        .collect();
    Some(LeakCase {
        host,
        view,
        text,
        actions,
    })
}

/// The cycles of each measured run.
fn cycles() -> usize {
    std::env::var("FUZZ_LEAK_CYCLES")
        .ok()
        .and_then(|cycles| cycles.parse().ok())
        .unwrap_or(20)
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Replaces the whole text on screen with `text`, as a paste over all.
fn put(state: EditorState, text: &str) {
    let before = shown(state);
    if before != text {
        let all = units(&before);
        state.commit(Edit::new_units(before, (0, all), text.to_string(), 0));
    }
}

/// One cycle: the edits, then the document as it opened (in the language
/// it opened in, the host told, no step to undo, no fold closed).
/// Whether it got back there.
fn cycle(state: EditorState, case: &LeakCase, opened: &(String, Language)) -> bool {
    let mut context = Context::default();
    for action in &case.actions {
        apply(state, action, &mut context);
    }
    pause(state);
    // The host's own text put back in its language: a view of the same
    // values saves nothing, so an edit of the host's layout alone (an
    // indent made while editing as the host's language) stays in the host
    // otherwise, and the document grows each cycle.
    if state.sync().host_text.get_value() != case.text {
        let now = state.sync().edit_language.get_untracked();
        if now != case.host {
            if let Ok(conversion) = case.host.convert(&case.text, now) {
                put(state, &conversion.text);
            }
            switch_to(state, case.host);
        }
        put(state, &case.text);
        pause(state);
    }
    let (text, language) = opened;
    let now = state.sync().edit_language.get_untracked();
    if now != *language {
        if let Ok(conversion) = language.convert(text, now) {
            put(state, &conversion.text);
        }
        switch_to(state, *language);
    }
    put(state, text);
    pause(state);
    state.doc().history.update(|history| history.clear());
    state.unfold_all();
    state.close_suggestions();
    state.search().open.set(false);
    // The stages the edits passed, which only tests record.
    take_trace();
    shown(state) == *text
        && state.sync().edit_language.get_untracked() == *language
        && state.sync().host_text.get_value() == case.text
}

/// What two measured runs of `count` cycles each kept, after a warm-up;
/// `None` when a cycle could not put the document back.
fn kept(case: &LeakCase, count: usize) -> Option<[(isize, isize); 2]> {
    let mut editor = TestEditor::new(case.host, &case.text);
    editor.edit_as = case.view;
    let (_owner, state) = editor.mount();
    let opened = (shown(state), state.sync().edit_language.get_untracked());
    for _ in 0..count + count / 2 {
        if !cycle(state, case, &opened) {
            return None;
        }
    }
    let start = live();
    (0..count).for_each(|_| {
        cycle(state, case, &opened);
    });
    let middle = live();
    (0..count).for_each(|_| {
        cycle(state, case, &opened);
    });
    let end = live();
    Some([
        (middle.0 - start.0, middle.1 - start.1),
        (end.0 - middle.0, end.1 - middle.1),
    ])
}

pub(super) fn check(case: &LeakCase) -> Result<(), Failure> {
    let count = cycles();
    let owned = case.clone();
    // On a thread of its own: what other tests left in this thread's
    // caches neither counts nor fills them meanwhile.
    let runs = std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(move || kept(&owned, count))
        .expect("a thread")
        .join()
        .map_err(|_| Failure::new("a cycle panics", "see the thread's panic above"))?;
    let Some(runs) = runs else {
        return Ok(());
    };
    // Anything kept per cycle is `count` allocations or more in each run.
    if runs
        .iter()
        .all(|(allocations, bytes)| *allocations > 3 || *bytes > 1024)
    {
        let at = match case.view {
            Some(view) => format!("{} host as {view}", case.host),
            None => case.host.to_string(),
        };
        return Err(Failure::new(
            format!("a repeated edit sequence keeps memory ({at})"),
            format!("kept over {count} cycles, twice (allocations, bytes): {runs:?}"),
        ));
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: LeakCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
#[ignore = "measures this thread's memory: run alone (scripts/ci.sh test runs keeps_no_memory with --ignored --test-threads=1)"]
fn a_random_edit_sequence_repeated_keeps_no_memory() {
    let count = std::env::var("FUZZ_LEAK_CASES")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(100);
    run("leaks", count, &generate, &check);
}
