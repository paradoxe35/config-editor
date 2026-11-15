//! Folding, find and go to line under random folds and edits: closed
//! folds stay folds of the text as it is (each still folds its block),
//! the view hides exactly the closed folds' lines, go to line lands on the
//! line asked for, and the find widget's matches (count, lines) equal a
//! model computed on the text.

use leptos::prelude::*;

use super::actions::{Action, Context, QUERIES, apply, code, random_action};
use super::docs::{LANGUAGES, document, line_of, literal, smaller_documents};
use super::replace::model_matches;
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::commands::offset_at;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

#[derive(Clone, Debug)]
pub(super) struct FoldCase {
    pub(super) language: Language,
    pub(super) text: String,
    pub(super) actions: Vec<Action>,
    /// Find queries checked after each edit: (query, options bits).
    pub(super) query: (&'static str, u8),
}

impl Case for FoldCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn folds_repro() {{\n    use super::actions::Action::*;\n    super::folds::replay(super::folds::FoldCase {{\n        language: Language::{},\n        text: {}.into(),\n        query: ({:?}, {}),\n        actions: vec![\n{}\n        ],\n    }});\n}}",
            self.language.label(),
            literal(&self.text),
            self.query.0,
            self.query.1,
            code(&self.actions)
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        for index in 0..self.actions.len() {
            let mut actions = self.actions.clone();
            actions.remove(index);
            out.push(Self {
                actions,
                ..self.clone()
            });
        }
        for text in smaller_documents(self.language, &self.text) {
            out.push(Self {
                text,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.actions.len() * 10_000 + self.text.len()
    }
}

fn generate(rng: &mut Rng) -> Option<FoldCase> {
    let language = *rng.pick(&LANGUAGES);
    let text = document(rng, language);
    let actions = (0..4 + rng.below(16))
        .map(|_| {
            if rng.chance(45) {
                match rng.below(4) {
                    0 => Action::Fold {
                        line: rng.position(),
                    },
                    1 => Action::Unfold {
                        line: rng.position(),
                    },
                    2 => Action::FoldAll,
                    _ => Action::Line {
                        at: rng.position(),
                        to: rng.position(),
                        command: rng.below(5) as u8,
                    },
                }
            } else {
                random_action(rng, true, false)
            }
        })
        .collect();
    Some(FoldCase {
        language,
        text,
        actions,
        query: (*rng.pick(&QUERIES), rng.below(8) as u8),
    })
}

fn invariants(state: EditorState, case: &FoldCase, after: &str) -> Result<(), Failure> {
    let language = case.language;
    let text = state.doc().text.get_untracked();
    let lines = text.split('\n').count();
    state.settle_readings();
    let foldable = state.fold().fold_ranges.get_untracked();
    let closed = state.fold().folded.get_untracked();
    for fold in &closed {
        if fold.end >= lines {
            return Err(Failure::new(
                format!("a closed fold runs past the text ({language})"),
                format!("after {after}: fold {fold:?} in {lines} lines:\n{text}"),
            ));
        }
        if !foldable.contains(fold) {
            return Err(Failure::new(
                format!("a closed fold no longer folds its block ({language})"),
                format!("after {after}: fold {fold:?}, foldable {foldable:?}:\n{text}"),
            ));
        }
    }
    let fresh = state.behavior().fold_ranges(&text);
    if fresh != foldable {
        return Err(Failure::new(
            format!("the fold ranges are stale ({language})"),
            format!("after {after}: memo {foldable:?}, text {fresh:?}:\n{text}"),
        ));
    }
    // The view hides exactly the closed folds' lines.
    let hidden: Vec<usize> = (0..lines)
        .filter(|line| closed.iter().any(|fold| fold.hides(*line)))
        .collect();
    let rows = state.view().projection.with_untracked(|projection| {
        (0..projection.rows())
            .map(|row| projection.line_of_row(row))
            .collect::<Vec<_>>()
    });
    let shown: Vec<usize> = (0..lines).filter(|line| !hidden.contains(line)).collect();
    if rows != shown {
        return Err(Failure::new(
            format!("the view's rows differ from the closed folds ({language})"),
            format!("after {after}: rows {rows:?}, lines not hidden {shown:?}, folds {closed:?}"),
        ));
    }
    // Go to line N lands on line N, column 1.
    for line in [1, lines.div_ceil(2), lines] {
        let at = offset_at(&text, line, 1);
        if line_of(&text, at) + 1 != line {
            return Err(Failure::new(
                format!("go to line lands on another line ({language})"),
                format!(
                    "line {line} gave offset {at} on line {}",
                    line_of(&text, at) + 1
                ),
            ));
        }
    }
    // Find: the widget's matches equal the model's.
    let (query, bits) = case.query;
    state.search().query.set(query.to_string());
    state.search().options.update(|options| {
        options.case_sensitive = bits & 1 != 0;
        options.whole_word = bits & 2 != 0;
        options.regex = bits & 4 != 0;
    });
    state.search().open.set(true);
    let found: Vec<(usize, usize, usize)> = state.search().results.with_untracked(|results| {
        results
            .matches
            .iter()
            .map(|found| (found.start, found.end, found.line))
            .collect()
    });
    state.search().open.set(false);
    let model: Vec<(usize, usize, usize)> = model_matches(&text, query, bits)
        .into_iter()
        .map(|(start, end)| (start, end, text[..start].matches('\n').count()))
        .collect();
    if found != model {
        return Err(Failure::new(
            format!("find matches differ from the model ({language}, options {bits})"),
            format!("query {query:?}: found {found:?}, model {model:?}"),
        ));
    }
    Ok(())
}

pub(super) fn check(case: &FoldCase) -> Result<(), Failure> {
    let (_owner, state) = TestEditor::new(case.language, &case.text).mount();
    let mut context = Context::default();
    invariants(state, case, "opening")?;
    for (index, action) in case.actions.iter().enumerate() {
        apply(state, action, &mut context);
        invariants(state, case, &format!("edit {index} {action:?}"))?;
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: FoldCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn folds_find_and_go_to_line_follow_random_edits() {
    run("folds", cases(300), &generate, &check);
}
