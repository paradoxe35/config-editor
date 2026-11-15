//! A host file's line endings and byte order mark through editing: a file
//! with LF, CRLF or lone-CR endings (all one kind, or mixed line by line),
//! with a mark or not, in every language (and edited as YAML or JSON),
//! typed into, broken into lines, cut, pasted over, undone, redone, paused
//! and saved at random. After each step what the host hears (`on_change`,
//! `on_save`) keeps the mark exactly when the file had one, reads as the
//! text on screen, keeps the ending of every line the step left alone,
//! and, when the text heard before had one kind of ending, has only that
//! kind (a lone CR its language cannot read aside). As `line_endings`
//! promises.

use leptos::prelude::*;

use super::runner::{Cap, Case, Failure, Rng, cases, run};
use super::texts::{document, language, literal, shown};
use crate::editor::edit::Edit;
use crate::editor::host_editing::SaveRequest;
use crate::editor::keyboard::utf16_to_byte_index;
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::languages::Language;
use crate::editor::line_endings::editor_text;
use crate::editor::state::EditorState;
use crate::editor::sync::convert_now;
use crate::editor::test_state::TestEditor;

/// One step of a session.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Step {
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
    Paste {
        at: u32,
        to: u32,
        text: &'static str,
    },
    Undo,
    Redo,
    Pause,
    Save,
}

/// What a session types and pastes.
const TYPED: [&str; 6] = ["x", "1", " ", "é", "😀", "#"];
const PASTED: [&str; 5] = ["\n", "a\nb", "\n\n", "x\r\ny", "k\n"];

#[derive(Clone, Debug)]
pub(super) struct EndingsCase {
    pub(super) language: &'static str,
    /// The language on screen, when another.
    pub(super) view: Option<&'static str>,
    /// The host's file, its endings and mark as written.
    pub(super) text: String,
    pub(super) steps: Vec<Step>,
}

impl Case for EndingsCase {
    fn code(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|step| format!("            Step::{step:?},"))
            .collect();
        format!(
            "#[test]\nfn endings_repro() {{\n    use super::endings::Step;\n    super::endings::replay(super::endings::EndingsCase {{\n        language: {:?},\n        view: {:?},\n        text: {}.into(),\n        steps: vec![\n{}\n        ],\n    }});\n}}",
            self.language,
            self.view,
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
        if self.view.is_some() {
            out.push(Self {
                view: None,
                ..self.clone()
            });
        }
        // A line of the file left out.
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
        self.text.len() + self.steps.len() * 1000 + usize::from(self.view.is_some())
    }
}

/// The built-in languages and the custom one.
const HOSTS: [&str; 6] = ["YAML", "JSON", "TOML", "HCL", "XML", "ODD"];

fn generate(rng: &mut Rng) -> Option<EndingsCase> {
    let name = *rng.pick(&HOSTS);
    let lf = document(rng, language(name));
    if !lf.contains('\n') {
        return None;
    }
    let style = rng.below(5);
    let mut text = String::new();
    for character in lf.chars() {
        if character == '\n' {
            text.push_str(match style {
                0 => "\n",
                1 => "\r\n",
                2 => "\r",
                _ => rng.pick(&["\n", "\r\n", "\r", "\r\n"]),
            });
        } else {
            text.push(character);
        }
    }
    if rng.chance(40) {
        text.insert(0, '\u{feff}');
    }
    let view = (name != "ODD" && rng.chance(25))
        .then(|| *rng.pick(&["YAML", "JSON"]))
        .filter(|view| *view != name);
    let steps = (0..1 + rng.below(10))
        .map(|_| {
            let (at, to) = (rng.position(), rng.position());
            match rng.below(10) {
                0 | 1 => Step::Type {
                    at,
                    text: rng.pick(&TYPED),
                },
                2 | 3 => Step::Enter { at },
                4 => Step::Cut { at, to },
                5 => Step::Paste {
                    at,
                    to,
                    text: rng.pick(&PASTED),
                },
                6 => Step::Undo,
                7 => Step::Redo,
                8 => Step::Pause,
                _ => Step::Save,
            }
        })
        .collect();
    Some(EndingsCase {
        language: name,
        view,
        text,
        steps,
    })
}

/// `text`'s lines with their endings (LF, CRLF, a lone CR, or none last).
fn split_lines(text: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let mut rest = text;
    loop {
        match rest.find(['\r', '\n']) {
            Some(at) => {
                let ending = if rest[at..].starts_with("\r\n") {
                    "\r\n"
                } else if rest[at..].starts_with('\r') {
                    "\r"
                } else {
                    "\n"
                };
                out.push((rest[..at].to_string(), ending));
                rest = &rest[at + ending.len()..];
            }
            None => {
                out.push((rest.to_string(), ""));
                return out;
            }
        }
    }
}

/// The UTF-16 offset a raw position stands for in `text`, on a character.
fn offset(text: &str, at: u32) -> usize {
    let characters = text.chars().count();
    let index = at as usize % (characters + 1);
    text.chars().take(index).map(char::len_utf16).sum()
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
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
    state.doc().caret.set(caret);
    state.commit(Edit::input(
        before,
        (start, end),
        text,
        (caret, caret),
        kind.into(),
        time,
    ));
}

fn apply(state: EditorState, step: &Step, time: f64) {
    let text = state.doc().text.get_untracked();
    let range = |at: u32, to: u32| {
        let (a, b) = (offset(&text, at), offset(&text, to));
        (a.min(b), a.max(b))
    };
    match *step {
        Step::Type { at, text: typed } => {
            let at = offset(&text, at);
            replace(state, (at, at), typed, "insertText", time);
        }
        Step::Enter { at } => {
            let at = offset(&text, at);
            state.doc().caret.set(at);
            enter_at(state, (at, at), Pressed::Enter, Some(time));
        }
        Step::Cut { at, to } => replace(state, range(at, to), "", "deleteByCut", time),
        Step::Paste {
            at,
            to,
            text: pasted,
        } => {
            // The browser hands the textarea LF text.
            let pasted = pasted.replace("\r\n", "\n");
            replace(state, range(at, to), &pasted, "insertFromPaste", time);
        }
        Step::Undo | Step::Redo => {
            if let Some(step) = state.history_edit(matches!(step, Step::Redo)) {
                state.commit(step);
            }
        }
        Step::Pause => pause(state),
        Step::Save => {
            pause(state);
            let _ = state.save();
        }
    }
}

fn pause(state: EditorState) {
    let text = state.doc().text.get_untracked();
    convert_now(state, text, state.sync().edit_language.get_untracked());
}

/// Whether the language reads a lone CR as a line break.
/// Whether the language reads the lone CRs of `text` as line breaks, as
/// `line_endings` decides it.
fn reads_lone_cr(language: Language, text: &str) -> bool {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lone = text.replace("\r\n", "").contains('\r');
    !lone || language.parse(text).is_ok() || language.parse(&editor_text(text.to_string())).is_err()
}

/// Why `heard` breaks a promise, after `before` (the text heard last).
fn broken(
    case: &EndingsCase,
    before: &str,
    heard: &str,
    marked: bool,
    lone_cr: bool,
) -> Option<(&'static str, String)> {
    if heard.starts_with('\u{feff}') != marked
        || heard.matches('\u{feff}').count() > case.text.matches('\u{feff}').count()
    {
        return Some((
            "the byte order mark is lost, added or doubled",
            format!("{} after {}", shown(heard), shown(before)),
        ));
    }
    let old = split_lines(before.strip_prefix('\u{feff}').unwrap_or(before));
    let new = split_lines(heard.strip_prefix('\u{feff}').unwrap_or(heard));
    // The lines the step left alone, at each end.
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(old, new)| old.0 == new.0)
        .count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(old.len().min(new.len()) - prefix)
        .take_while(|(old, new)| old.0 == new.0)
        .count();
    for index in 0..prefix {
        let last = index + 1 == old.len() || index + 1 == new.len();
        // A lone CR the language does not read is written anew.
        let unread = old[index].1 == "\r" && !lone_cr;
        if !last && !unread && old[index].1 != new[index].1 {
            return Some((
                "a line the edit left alone changes its line ending",
                format!(
                    "line {} in {} after {}",
                    index + 1,
                    shown(heard),
                    shown(before)
                ),
            ));
        }
    }
    for back in 1..suffix {
        // The last line has no ending; the ones above it keep theirs.
        let (old_line, new_line) = (&old[old.len() - 1 - back], &new[new.len() - 1 - back]);
        if old_line.1 != new_line.1 && (old_line.1 != "\r" || lone_cr) {
            return Some((
                "a line the edit left alone changes its line ending",
                format!(
                    "line {} from the end in {} after {}",
                    back + 1,
                    shown(heard),
                    shown(before)
                ),
            ));
        }
    }
    let endings: Vec<&str> = old
        .iter()
        .map(|line| line.1)
        .filter(|ending| !ending.is_empty())
        .collect();
    let uniform = endings
        .first()
        .filter(|first| endings.iter().all(|ending| ending == *first));
    if let Some(&kind) = uniform
        && (kind != "\r" || lone_cr)
        && new.iter().any(|line| !line.1.is_empty() && line.1 != kind)
    {
        return Some((
            "a file of one line ending gets another",
            format!("{kind:?} file, {} after {}", shown(heard), shown(before)),
        ));
    }
    None
}

pub(super) fn check(case: &EndingsCase) -> Result<(), Failure> {
    let host = language(case.language);
    let view = case.view.map(language);
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, &case.text);
    editor.edit_as = view;
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|all| all.push(text))
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        heard.update(|all| all.push(request.text));
    }));
    let (_owner, state) = editor.mount();
    let marked = case.text.starts_with('\u{feff}');
    let lone_cr = reads_lone_cr(host, &case.text);
    let where_ = if view.is_some() { " (Edit-as)" } else { "" };
    let mut before = case.text.clone();
    for (index, step) in case.steps.iter().enumerate() {
        apply(state, step, 1000.0 * (index + 1) as f64);
        let all = heard.get_untracked();
        for text in &all {
            if let Some((class, detail)) = broken(case, &before, text, marked, lone_cr) {
                return Err(Failure::new(
                    format!("{class} ({}){where_}", case.language),
                    format!("after step {index} {step:?}: {detail}"),
                ));
            }
            before = text.clone();
        }
        if let Some(last) = all.last()
            && view.is_none()
            && editor_text(last.clone()) != state.doc().text.get_untracked()
        {
            return Err(Failure::new(
                format!(
                    "what the host hears does not read as the text on screen ({})",
                    case.language
                ),
                format!(
                    "after step {index} {step:?}: {} heard, {} shown",
                    shown(last),
                    shown(&state.doc().text.get_untracked())
                ),
            ));
        }
        heard.set(Vec::new());
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: EndingsCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn line_endings_and_the_byte_order_mark_survive_editing() {
    run("endings", cases(150), Cap::Here, generate, check);
}
