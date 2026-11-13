//! A closed fold moved by `Alt+↑` / `Alt+↓` stays a unit at its own level:
//! it moves past a sibling block whole, and a move that would take other
//! lines into it (a YAML block's last child, a TOML table's last key, a
//! key above the first table) is refused. `Ctrl+Shift+K` on it afterwards
//! deletes its own lines only. In every language.
//!
//! It used to move past one line, which then read as its own: `tls` moved
//! above `  burst: 20` took `burst` in, and deleting `tls` deleted it.

use leptos::prelude::*;

use super::super::fold_ranges::FoldRange;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::line_edit::LineCommand;
use super::lines::{Command, run};

/// A document whose last block (`tls`) follows a block (`limits`) and,
/// before it, a field (`listen`), in `language`; the blocks' header lines.
fn sample(language: Language) -> (&'static str, &'static str, &'static str) {
    match language.label() {
        "YAML" => (
            "id: srv-42\nlisten: 7835\nlimits:\n  max: 10\n  burst: 20\ntls:\n  cert: /etc/edge.pem\n  pin: 5\n",
            "limits:",
            "tls:",
        ),
        "JSON" => (
            "{\n  \"id\": \"srv-42\",\n  \"listen\": 7835,\n  \"limits\": {\n    \"max\": 10,\n    \"burst\": 20\n  },\n  \"tls\": {\n    \"cert\": \"/etc/edge.pem\",\n    \"pin\": 5\n  }\n}\n",
            "  \"limits\": {",
            "  \"tls\": {",
        ),
        "TOML" => (
            "id = \"srv-42\"\nlisten = 7835\n\n[limits]\nmax = 10\nburst = 20\n\n[tls]\ncert = \"/etc/edge.pem\"\npin = 5\n",
            "[limits]",
            "[tls]",
        ),
        "HCL" => (
            "id = \"srv-42\"\nlisten = 7835\n\nlimits {\n  max = 10\n  burst = 20\n}\n\ntls {\n  cert = \"/etc/edge.pem\"\n  pin = 5\n}\n",
            "limits {",
            "tls {",
        ),
        _ => (
            "<id>srv-42</id>\n<listen>7835</listen>\n<limits>\n  <max>10</max>\n  <burst>20</burst>\n</limits>\n<tls>\n  <cert>/etc/edge.pem</cert>\n  <pin>5</pin>\n</tls>\n",
            "<limits>",
            "<tls>",
        ),
    }
}

/// The 0-based line of `text` that is `line`.
fn line_of(text: &str, line: &str) -> usize {
    text.lines()
        .position(|found| found == line)
        .unwrap_or_else(|| panic!("no line {line:?} in {text:?}"))
}

/// The UTF-16 offset of the start of 0-based `line`.
fn start_of(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line)
        .map(|line| line.encode_utf16().count())
        .sum()
}

/// `command` with the caret on the line `line`.
fn press(state: EditorState, line: &str, command: LineCommand) {
    let text = state.doc().text.get_untracked();
    let at = start_of(&text, line_of(&text, line));
    run(state, Command::Lines(command), (at, at), false);
}

/// The lines of `text` without `taken`, in order: what stays once the
/// unit `taken` is deleted.
fn without(text: &str, taken: &[&str]) -> Vec<String> {
    let mut left: Vec<String> = text.lines().map(str::to_string).collect();
    for line in taken {
        let at = left.iter().position(|found| found == line).expect("a line");
        left.remove(at);
    }
    left
}

/// Why `now`, which parses, reads otherwise than `text`: a line taken
/// into the block moved.
fn read_otherwise(language: Language, text: &str, now: &str) -> Option<String> {
    let read = language.behavior().parse(now).ok()?;
    let was = language.behavior().parse(text).ok()?;
    (read != was).then(|| format!("{language}: reads {read} in {now:?}"))
}

#[test]
fn a_closed_fold_moves_as_a_unit_at_its_own_level() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        let (text, limits, tls) = sample(language);
        let (_owner, state) = TestEditor::new(language, text).mount();
        let foldable = state.fold().fold_ranges.get_untracked();
        let fold = *foldable
            .iter()
            .find(|fold| fold.start == line_of(text, tls))
            .expect("tls folds");
        let unit_lines: Vec<&str> = {
            let units = super::line_edit::fold_units(text, &[fold], language.behavior());
            let unit = units[0];
            text.lines()
                .skip(unit.start)
                .take(unit.end - unit.start + 1)
                .collect()
        };
        state.set_folds(vec![fold]);
        let up = LineCommand::Move { down: false };
        // Up: past the blank line (where there is one), then past `limits`
        // whole, then past the blank line above it, then (the key above
        // being no block) refused or past it.
        let mut texts = Vec::new();
        for _ in 0..4 {
            press(state, tls, up);
            texts.push(state.doc().text.get_untracked());
        }
        for now in &texts {
            if let Some(why) = read_otherwise(language, text, now) {
                failures.push(why);
            }
            let at = line_of(now, tls);
            let moved: Vec<&str> = now.lines().skip(at).take(unit_lines.len()).collect();
            if moved != unit_lines {
                failures.push(format!("{language}: the unit split: {now:?}"));
            }
            let limits_at = line_of(now, limits);
            let below_limits: Vec<&str> = now.lines().skip(limits_at).collect();
            if !below_limits.iter().any(|line| line.contains("burst")) {
                failures.push(format!("{language}: limits lost burst: {now:?}"));
            }
        }
        if !texts
            .iter()
            .any(|now| line_of(now, tls) < line_of(now, limits))
        {
            failures.push(format!("{language}: never moved past limits: {texts:?}"));
        }
        if state.with_closed_folds_untracked(|folds| folds.len()) != 1 {
            failures.push(format!("{language}: the fold opened"));
        }
        // Deleted: its own lines only.
        let before = state.doc().text.get_untracked();
        press(state, tls, LineCommand::Delete);
        let left: Vec<String> = state
            .doc()
            .text
            .get_untracked()
            .lines()
            .map(str::to_string)
            .collect();
        if left != without(&before, &unit_lines) {
            failures.push(format!(
                "{language}: deleting it left {left:?} of {before:?}"
            ));
        }
    }
    // Down: `limits` folded moves past `tls` whole.
    for language in Language::ALL.iter().copied() {
        let (text, limits, tls) = sample(language);
        let (_owner, state) = TestEditor::new(language, text).mount();
        let foldable = state.fold().fold_ranges.get_untracked();
        let fold: FoldRange = *foldable
            .iter()
            .find(|fold| fold.start == line_of(text, limits))
            .expect("limits folds");
        state.set_folds(vec![fold]);
        for _ in 0..2 {
            press(state, limits, LineCommand::Move { down: true });
            let now = state.doc().text.get_untracked();
            if let Some(why) = read_otherwise(language, text, &now) {
                failures.push(why);
            }
            let tls_at = line_of(&now, tls);
            let under_tls: Vec<&str> = now.lines().skip(tls_at).collect();
            let limits_at = line_of(&now, limits);
            if limits_at > tls_at
                && under_tls[..limits_at - tls_at]
                    .iter()
                    .filter(|line| line.contains("pin"))
                    .count()
                    != 1
            {
                failures.push(format!("{language}: tls split: {now:?}"));
            }
            if limits_at < tls_at
                && now
                    .lines()
                    .skip(limits_at)
                    .take(tls_at - limits_at)
                    .any(|line| line.contains("cert"))
            {
                failures.push(format!("{language}: limits took tls's lines: {now:?}"));
            }
        }
        let now = state.doc().text.get_untracked();
        if line_of(&now, limits) < line_of(&now, tls) {
            failures.push(format!("{language}: never moved past tls: {now:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
