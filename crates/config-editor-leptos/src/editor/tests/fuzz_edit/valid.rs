//! Edit commands on random parseable documents keep the text valid where
//! they should: a line command and its opposite give the text back (move
//! down then up, copy then delete, indent then outdent, comment twice);
//! indenting lines changes no value in a language where indentation means
//! nothing (outside multi-line strings); and Enter at the end of an entry
//! or a block's opening line indents so that a new entry typed there reads
//! under the parent the line says (beside the entry, inside the block).

use leptos::prelude::*;

use super::docs::{LANGUAGES, document, literal};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::commands::{offset_at, toggled_selection};
use crate::editor::edit::Edit;
use crate::editor::keyboard::replace_selection;
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run as run_command};
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

#[derive(Clone, Debug)]
pub(super) struct ValidCase {
    pub(super) language: Language,
    pub(super) text: String,
    /// Lines (from 1) of the selection's start and end.
    pub(super) first: usize,
    pub(super) last: usize,
}

impl Case for ValidCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn valid_repro() {{\n    super::valid::replay(super::valid::ValidCase {{\n        language: Language::{},\n        text: {}.into(),\n        first: {},\n        last: {},\n    }});\n}}",
            self.language.label(),
            literal(&self.text),
            self.first,
            self.last
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.last > self.first {
            out.push(Self {
                last: self.first,
                ..self.clone()
            });
        }
        // A line (or an opener and its closer) left out, the selected
        // lines following it; the text must still read.
        let lines: Vec<&str> = self.text.split_inclusive('\n').collect();
        let count = lines.len();
        let mut pairs: Vec<(usize, Option<usize>)> = (0..count).map(|line| (line, None)).collect();
        for first in 0..count {
            for last in first + 1..count.min(first + 12) {
                pairs.push((first, Some(last)));
            }
        }
        for (one, two) in pairs {
            let gone = |line: usize| line == one || Some(line) == two;
            // Selected lines (from 1) left out are no smaller case.
            if (self.first - 1..self.last).any(gone) {
                continue;
            }
            let text: String = lines
                .iter()
                .enumerate()
                .filter(|(line, _)| !gone(*line))
                .map(|(_, line)| *line)
                .collect();
            if self.language.parse(&text).is_err() {
                continue;
            }
            let before = (0..self.first - 1).filter(|line| gone(*line)).count();
            out.push(Self {
                text,
                first: self.first - before,
                last: self.last - before,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.text.len() * 4 + (self.last - self.first)
    }
}

fn generate(rng: &mut Rng) -> Option<ValidCase> {
    let language = *rng.pick(&LANGUAGES);
    let mut text = document(rng, language);
    // Some texts end without a newline.
    if rng.chance(30) {
        text = text.trim_end_matches('\n').to_string();
    }
    language.parse(&text).ok()?;
    let lines = text.lines().count().max(1);
    let first = 1 + rng.below(lines);
    let last = if rng.chance(60) {
        first
    } else {
        (first + rng.below(4)).min(lines)
    };
    Some(ValidCase {
        language,
        text,
        first,
        last,
    })
}

fn mount(case: &ValidCase, text: &str) -> (Owner, EditorState) {
    TestEditor::new(case.language, text).mount()
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// The selection over lines `first..=last` (from 1), start to end.
fn lines_selection(text: &str, first: usize, last: usize) -> (usize, usize) {
    let start = offset_at(text, first, 1);
    let end = offset_at(text, last, usize::MAX);
    (start, end)
}

fn command(state: EditorState, command: Command, selection: (usize, usize)) {
    state.doc().caret.set(selection.1);
    run_command(state, command, selection, false);
}

fn inverse(case: &ValidCase) -> Result<(), Failure> {
    let language = case.language;
    let original = &case.text;
    let selection = lines_selection(original, case.first, case.last);
    // A selection ending at a line's start (an empty last line) leaves
    // that line out, as in VS Code.
    let mut last = case.last;
    while last > case.first && selection.1 == offset_at(original, last, 1) {
        last -= 1;
    }
    let pairs: [(&str, Command, Command); 3] = [
        (
            "move down then up",
            Command::Lines(LineCommand::Move { down: true }),
            Command::Lines(LineCommand::Move { down: false }),
        ),
        (
            "copy down then delete",
            Command::Lines(LineCommand::Copy { down: true }),
            Command::Lines(LineCommand::Delete),
        ),
        (
            "indent then outdent",
            Command::Indent { outdent: false },
            Command::Indent { outdent: true },
        ),
    ];
    for (name, there, back) in pairs {
        let (_owner, state) = mount(case, original);
        command(state, there, selection);
        let middle = shown(state);
        if state.doc().caret.get_untracked() > super::docs::units(&middle) {
            return Err(Failure::new(
                format!("caret past the text's end after {name} ({language})"),
                format!(
                    "lines {}..={}: caret {} in {middle:?}",
                    case.first,
                    last,
                    state.doc().caret.get_untracked()
                ),
            ));
        }
        if middle == *original {
            continue;
        }
        // The command leaves its lines selected: the lines it moved,
        // copied or indented, read off the text.
        let moved = match there {
            Command::Lines(LineCommand::Move { .. }) => {
                let count = last - case.first;
                let first = (case.first + 1).min(middle.lines().count().max(1));
                lines_selection(&middle, first, first + count)
            }
            Command::Lines(LineCommand::Copy { .. }) => {
                let count = last - case.first + 1;
                lines_selection(&middle, last + 1, last + count)
            }
            _ => lines_selection(&middle, case.first, last),
        };
        command(state, back, moved);
        let after = shown(state);
        if after != *original {
            return Err(Failure::new(
                format!("{name} does not give the text back ({language})"),
                format!(
                    "lines {}..={}:\nmiddle:\n{middle}\nafter:\n{after}",
                    case.first, last
                ),
            ));
        }
    }
    // Ctrl+/ twice.
    let behavior = language.behavior();
    if let Some((once, from, to)) = behavior.toggled_text(original, selection.0, selection.1)
        && let (from, to) = toggled_selection(original, selection, &once, (from, to))
        && let Some((twice, _, _)) = behavior.toggled_text(&once, from, to)
        && twice != *original
    {
        return Err(Failure::new(
            format!("comment twice does not give the text back ({language})"),
            format!("once:\n{once}\ntwice:\n{twice}"),
        ));
    }
    Ok(())
}

/// Indenting changes no value where indentation means nothing.
fn indent_keeps_values(case: &ValidCase) -> Result<(), Failure> {
    let language = case.language;
    if language == Language::YAML {
        return Ok(());
    }
    let (_owner, state) = mount(case, &case.text);
    let bodies = state.view().string_bodies.get_untracked();
    let in_body = (case.first - 1..case.last).any(|line| {
        bodies
            .iter()
            .any(|body| body.contains(&line) || body.end == line)
    });
    // Lines inside a multi-line string or XML text: their indentation is
    // the value's (as in YAML).
    if in_body || language == Language::XML && multi_line_text(&case.text) {
        return Ok(());
    }
    let before = language.parse(&case.text).ok();
    for outdent in [false, true] {
        let (_owner, state) = mount(case, &case.text);
        command(
            state,
            Command::Indent { outdent },
            lines_selection(&case.text, case.first, case.last),
        );
        let after = shown(state);
        let read = language.parse(&after).ok();
        if read != before {
            return Err(Failure::new(
                format!(
                    "{} changed a value ({language})",
                    if outdent { "outdent" } else { "indent" }
                ),
                format!("after:\n{after}\nread {read:?}\nbefore {before:?}"),
            ));
        }
    }
    Ok(())
}

fn multi_line_text(text: &str) -> bool {
    fn any(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(text) => text.contains('\n'),
            serde_json::Value::Array(items) => items.iter().any(any),
            serde_json::Value::Object(map) => map.values().any(any),
            _ => false,
        }
    }
    Language::XML.parse(text).is_ok_and(|value| any(&value))
}

/// A new entry `zz` with the value 1, written in `language`.
fn new_entry(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "zz: 1",
        "JSON" => "\"zz\": 1,",
        "TOML" | "HCL" => "zz = 1",
        _ => "<zz>1</zz>",
    }
}

/// Whether line `line` (from 1) opens a block the new entry goes into
/// (`Some(true)`), is a whole entry the new one goes beside
/// (`Some(false)`), or neither (`None`).
fn line_kind(language: Language, text: &str, line: usize) -> Option<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let written = lines.get(line - 1)?.trim();
    let next_indent = lines
        .get(line)
        .map(|next| next.len() - next.trim_start().len());
    let indent = lines[line - 1].len() - lines[line - 1].trim_start().len();
    match language.label() {
        "YAML" => {
            if written.starts_with(['-', '#'])
                || written.contains(['{', '[', '|', '>', '&', '*', '!'])
            {
                None
            } else if written.ends_with(':') {
                next_indent
                    .is_some_and(|next| next > indent)
                    .then_some(true)
            } else if written.contains(": ") && next_indent.is_none_or(|next| next <= indent) {
                Some(false)
            } else {
                None
            }
        }
        "JSON" => {
            if written.ends_with('{') && written.contains(':') {
                Some(true)
            } else if written.ends_with(',')
                && written.contains("\": ")
                && !written.contains(['[', '{'])
            {
                Some(false)
            } else {
                None
            }
        }
        "TOML" => {
            if written.starts_with("[[") {
                None
            } else if written.starts_with('[') {
                Some(true)
            } else if written.contains(" = ")
                && !written.ends_with(['[', ','])
                && !written.contains("\"\"\"")
                && !written.contains("'''")
            {
                Some(false)
            } else {
                None
            }
        }
        "HCL" => {
            if written.ends_with('{') && !written.contains('=') {
                Some(true)
            } else if written.contains(" = ")
                && !written.ends_with(['{', '[', ',', '('])
                && !written.contains("<<")
                && !written.contains('}')
            {
                Some(false)
            } else {
                None
            }
        }
        _ => {
            if written.starts_with("<!--") || written.starts_with("</") {
                None
            } else if written.starts_with('<')
                && written.ends_with('>')
                && !written.contains("</")
                && !written.ends_with("/>")
            {
                Some(true)
            } else if written.starts_with('<') && written.contains("</") && written.ends_with('>') {
                Some(false)
            } else {
                None
            }
        }
    }
}

/// Enter at the end of line `first`, then a new entry typed: it reads
/// where the line says.
fn enter_indents(case: &ValidCase) -> Result<(), Failure> {
    let language = case.language;
    let text = &case.text;
    let Some(opens) = line_kind(language, text, case.first) else {
        return Ok(());
    };
    if text.contains("zz") {
        return Ok(());
    }
    // The text a user means: the new entry on a line of its own after
    // the line, at the column of the block's entries (its children's, or
    // one indent in for an empty block; a TOML table's at its header's).
    let lines: Vec<&str> = text.lines().collect();
    let indent_of = |line: &str| line.len() - line.trim_start().len();
    let line_indent = indent_of(lines[case.first - 1]);
    let next = lines[case.first..].iter().find(|line| {
        let line = line.trim_start();
        !line.is_empty() && !line.starts_with(['#', '/']) && !line.starts_with("<!--")
    });
    let column = if !opens {
        line_indent
    } else if language == Language::TOML {
        0
    } else {
        match next.map(|line| indent_of(line)) {
            Some(next) if next > line_indent => next,
            _ => line_indent + 2,
        }
    };
    if opens
        && language == Language::YAML
        && next.is_some_and(|line| line.trim_start().starts_with('-'))
    {
        return Ok(());
    }
    let mut meant: Vec<String> = text.lines().map(str::to_string).collect();
    meant.insert(
        case.first,
        format!("{}{}", " ".repeat(column), new_entry(language)),
    );
    let meant = meant.join("\n") + if text.ends_with('\n') { "\n" } else { "" };
    let Ok(wanted) = language.parse(&meant) else {
        return Ok(());
    };
    let (_owner, state) = mount(case, text);
    let end = offset_at(text, case.first, usize::MAX);
    state.doc().caret.set(end);
    enter_at(state, (end, end), Pressed::Enter, Some(0.0));
    let before = shown(state);
    let caret = state.doc().caret.get_untracked();
    let (typed, after_caret) = replace_selection(&before, caret, caret, new_entry(language));
    state.commit(Edit::input(
        before,
        (caret, caret),
        typed,
        (after_caret, after_caret),
        "insertText".into(),
        50.0,
    ));
    let after = shown(state);
    let at = if opens {
        "a block's opening line"
    } else {
        "an entry"
    };
    let read = match language.parse(&after) {
        Ok(read) => read,
        Err(error) => {
            return Err(Failure::new(
                format!("Enter after {at}, then a new entry, does not read ({language})"),
                format!("{error}\n{after}"),
            ));
        }
    };
    if read != wanted {
        return Err(Failure::new(
            format!("Enter after {at} puts the new entry elsewhere than meant ({language})"),
            format!("typed:\n{after}\nmeant:\n{meant}"),
        ));
    }
    Ok(())
}

pub(super) fn check(case: &ValidCase) -> Result<(), Failure> {
    if case.language.parse(&case.text).is_err() {
        return Ok(());
    }
    inverse(case)?;
    indent_keeps_values(case)?;
    enter_indents(case)
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: ValidCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn edit_commands_keep_documents_valid_under_random_selections() {
    run("valid", cases(600), &generate, &check);
}
