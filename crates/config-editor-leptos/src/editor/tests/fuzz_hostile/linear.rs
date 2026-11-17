//! Work on hostile text grows no faster than the text: a broken or odd
//! document (thousands of small items after an unclosed string, comment,
//! bracket or heredoc, one very long line, deep nesting, lone CRs) is read
//! at three sizes, each twice the one before, and typed into once in a
//! mounted editor. Every kind of counted work (see `work_count`) may grow
//! by at most [`RATIO`] halves per doubling, as in `linear_work`. Only
//! counts are compared, never times: a count is the same on a busy machine.

use leptos::prelude::*;

use super::runner::{Cap, Case, Failure, Rng, cases, run};
use super::texts::{NAMES, language, many_items, nested};
use crate::editor::completion::completions;
use crate::editor::edit::Edit;
use crate::editor::highlight::spanning::{Spanning, tokens_after};
use crate::editor::line_endings::editor_text;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted_all};

/// The shape a text grows in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// Small items, one a line.
    Items,
    /// One line holding the whole text.
    LongLine,
    /// Values inside each other.
    Deep,
}

/// What is written before the shape, breaking it.
pub(super) const BREAKERS: [&str; 12] = [
    "", "\"", "'", "\"\"\"", "/*", "<!--", "<<EOT\n", "[", "{", "<a ", "\u{feff}", "a: |\n",
];

/// What a long line is made of.
pub(super) const UNITS: [&str; 6] = ["x", "é", "😀", "\"a\", ", "[1, ", "<a>"];

#[derive(Clone, Debug)]
pub(super) struct LinearCase {
    pub(super) language: &'static str,
    pub(super) shape: Shape,
    /// Index into [`BREAKERS`].
    pub(super) breaker: usize,
    /// Index into [`UNITS`] (long lines).
    pub(super) unit: usize,
    /// Whether lines end with lone CRs and CRLFs in turn.
    pub(super) carriage_returns: bool,
}

impl Case for LinearCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn linear_repro() {{\n    super::linear::replay(super::linear::LinearCase {{\n        language: {:?},\n        shape: super::linear::Shape::{:?},\n        breaker: {},\n        unit: {},\n        carriage_returns: {},\n    }});\n}}",
            self.language, self.shape, self.breaker, self.unit, self.carriage_returns,
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.breaker > 0 {
            out.push(Self {
                breaker: 0,
                ..self.clone()
            });
        }
        if self.unit > 0 {
            out.push(Self {
                unit: 0,
                ..self.clone()
            });
        }
        if self.carriage_returns {
            out.push(Self {
                carriage_returns: false,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.breaker + self.unit + usize::from(self.carriage_returns)
    }
}

fn generate(rng: &mut Rng) -> Option<LinearCase> {
    Some(LinearCase {
        language: rng.pick(&NAMES),
        shape: *rng.pick(&[Shape::Items, Shape::Items, Shape::LongLine, Shape::Deep]),
        breaker: rng.below(BREAKERS.len()),
        unit: rng.below(UNITS.len()),
        carriage_returns: rng.chance(20),
    })
}

/// The case's text at `size` (1, 2, 4).
pub(super) fn text(case: &LinearCase, size: usize) -> String {
    let body = match case.shape {
        Shape::Items => many_items(case.language, 250 * size, "\"é😀\""),
        Shape::LongLine => format!("a = {}\n", UNITS[case.unit].repeat(2500 * size)),
        Shape::Deep => nested(case.language, 30 * size),
    };
    let text = format!("{}{body}", BREAKERS[case.breaker]);
    if case.carriage_returns {
        let mut turn = false;
        text.split('\n')
            .collect::<Vec<_>>()
            .join("\u{0}")
            .chars()
            .map(|character| {
                if character == '\0' {
                    turn = !turn;
                    if turn {
                        "\r".to_string()
                    } else {
                        "\r\n".to_string()
                    }
                } else {
                    character.to_string()
                }
            })
            .collect()
    } else {
        text
    }
}

/// The readings of a whole text a keystroke may start, and the ones at a
/// few carets (a fixed count, at the same fractions of the text).
fn read(case: &LinearCase, raw: &str) {
    let language = language(case.language);
    let behavior = language.behavior();
    let text = editor_text(raw.to_string());
    let _ = behavior.parse(&text);
    let _ = crate::editor::diagnostics::check_syntax(language, raw);
    let comments = behavior.comments(&text);
    let constructs = behavior.multiline_constructs(&text);
    let spanning = Spanning::of(&text, &comments, &constructs);
    for (index, line) in text.split('\n').enumerate() {
        let _ = tokens_after(language, line, spanning.at(index));
    }
    let _ = behavior.fold_ranges(&text);
    let _ = behavior.string_body_lines(&text);
    let units = text.encode_utf16().count();
    for part in 1..=3 {
        let at = crate::editor::keyboard::utf16_to_byte_index(&text, units * part / 4);
        let at = text[..at].encode_utf16().count();
        let _ = behavior.completion_context(&text, at);
        let _ = behavior.newline(&text, at);
        let _ = behavior.lexical_state(&text, at);
        let _ = completions(&text, at, true, None, true, behavior);
        let _ = crate::editor::paths::path_at(behavior, &text, at);
    }
}

/// What the view reads after an edit.
fn read_view(state: EditorState) {
    let _ = state.view().lines.get_untracked();
    let _ = state.view().visible_lines.get_untracked();
    let _ = state.view().spanning.get_untracked();
    let _ = state.view().string_bodies.get_untracked();
    let _ = state.fold().fold_ranges.get_untracked();
    state.changes().diff.with_untracked(|_| ());
    let _ = state.protect().protected.get_untracked();
    let text = state.doc().text.get_untracked();
    state.problems().diagnosed_text.set(text);
    let _ = state.problems().diagnostics.get_untracked();
}

/// A letter typed in the middle of the text on screen, the view read.
fn keystroke(case: &LinearCase, raw: &str) -> Vec<usize> {
    let (_owner, state) = TestEditor::new(language(case.language), raw).mount();
    read_view(state);
    let ((), counts) = counted_all(|| {
        let before = state.doc().text.get_untracked();
        let units = before.encode_utf16().count();
        let byte = crate::editor::keyboard::utf16_to_byte_index(&before, units / 2);
        let at = before[..byte].encode_utf16().count();
        let mut text = before.clone();
        text.insert(byte, 'x');
        state.set_caret_units(at);
        state.commit(Edit::input_units(
            before,
            (at, at),
            text,
            (at + 1, at + 1),
            "insertText".into(),
            1000.0,
        ));
        read_view(state);
    });
    counts.to_vec()
}

/// How many halves a kind of work may grow by when the text doubles.
const RATIO: usize = 5;

/// Counts this small are compared as if this large.
const FLOOR: usize = 16;

/// The kinds of work that grew too fast from `small` to `large`, the text
/// growing `times` times (about two; a text nested by indentation grows
/// faster than its depth).
fn grew(small: &[usize], large: &[usize], times: f64) -> Vec<String> {
    Work::ALL
        .iter()
        .zip(small.iter().zip(large))
        .filter(|(_, (small, large))| {
            **large as f64 * 2.0 > times / 2.0 * RATIO as f64 * (**small).max(FLOOR) as f64
        })
        .map(|(work, (small, large))| format!("{work:?} {small} -> {large}"))
        .collect()
}

pub(super) fn check(case: &LinearCase) -> Result<(), Failure> {
    let name = case.language;
    let mut readings = Vec::new();
    let mut typed = Vec::new();
    let mut lengths = Vec::new();
    for size in [1, 2, 4] {
        let raw = text(case, size);
        lengths.push(raw.len() as f64);
        let ((), counts) = counted_all(|| read(case, &raw));
        readings.push(counts);
        typed.push(keystroke(case, &raw));
    }
    for pair in [0, 1] {
        let growth = lengths[pair + 1] / lengths[pair];
        let found = grew(&readings[pair], &readings[pair + 1], growth);
        if !found.is_empty() {
            return Err(Failure::new(
                format!(
                    "reading a {:?} text grows faster than it ({name})",
                    case.shape
                ),
                found.join(", "),
            ));
        }
        let found = grew(&typed[pair], &typed[pair + 1], growth);
        if !found.is_empty() {
            return Err(Failure::new(
                format!(
                    "a keystroke in a {:?} text grows faster than it ({name})",
                    case.shape
                ),
                found.join(", "),
            ));
        }
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: LinearCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

const CAP: Cap = Cap::Thread {
    time: std::time::Duration::from_secs(60),
    stack: 4 << 20,
};

#[test]
fn work_on_hostile_text_grows_linearly() {
    run("linear", cases(24), CAP, generate, check);
}
