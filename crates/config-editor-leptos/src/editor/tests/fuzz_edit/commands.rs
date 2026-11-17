//! The edit commands on any text, half-typed ones too: Ctrl+/ and the
//! line commands (move, copy, delete, indent) never panic, leave their
//! selection inside the new text; and suggestions asked for (and one
//! accepted) anywhere never panic.
//! Texts are documents with random typing in them, so a failure shrinks
//! character by character to a few lines.

use leptos::prelude::*;

use super::actions::TYPED;
use super::docs::{CUSTOM, LANGUAGES, document, literal, selection, units};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run_units as run_command};
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

#[derive(Clone, Debug)]
pub(super) struct CommandCase {
    pub(super) language: Language,
    pub(super) text: String,
    pub(super) at: u32,
    pub(super) to: u32,
    /// 0 comment, 1 delete, 2 move up, 3 move down, 4 copy up, 5 copy
    /// down, 6 indent, 7 outdent, 8 suggestions asked for (with a
    /// schema) and the first accepted.
    pub(super) command: u8,
}

impl Case for CommandCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn command_repro() {{\n    super::commands::replay(super::commands::CommandCase {{\n        language: {},\n        text: {}.into(),\n        at: {},\n        to: {},\n        command: {},\n    }});\n}}",
            super::docs::language_code(self.language),
            literal(&self.text),
            self.at,
            self.to,
            self.command
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let characters: Vec<char> = self.text.chars().collect();
        // Positions are characters; a cut `from..to` moves them back.
        let moved = |at: u32, from: usize, to: usize| {
            let at = at as usize;
            (if at >= to {
                at - (to - from)
            } else {
                at.min(from)
            }) as u32
        };
        let cut = |from: usize, to: usize| Self {
            text: characters[..from].iter().chain(&characters[to..]).collect(),
            at: moved(self.at, from, to),
            to: moved(self.to, from, to),
            ..self.clone()
        };
        let mut out = Vec::new();
        // Whole lines first, then single characters.
        let mut start = 0;
        for (index, character) in characters.iter().enumerate() {
            if *character == '\n' {
                out.push(cut(start, index + 1));
                start = index + 1;
            }
        }
        if start < characters.len() {
            out.push(cut(start, characters.len()));
        }
        for index in 0..characters.len() {
            out.push(cut(index, index + 1));
        }
        out
    }

    fn size(&self) -> usize {
        self.text.len() * 1000 + self.at as usize + self.to as usize
    }
}

/// Characters the typing puts in: the edits' texts and the comment and
/// markup pieces that make half-written comments.
const BITS: [&str; 10] = [
    "<!--", "-->", "<!-->", "<!--->", "-", "#", "//", "/*", "*/", "\n",
];

fn generate(rng: &mut Rng, languages: &[Language]) -> Option<CommandCase> {
    let language = *rng.pick(languages);
    let mut text = document(rng, language);
    for _ in 0..rng.below(6) {
        let piece = if rng.chance(50) {
            *rng.pick(&TYPED)
        } else {
            *rng.pick(&BITS)
        };
        let at = super::docs::offset(&text, rng.position());
        let byte = crate::editor::keyboard::utf16_to_byte_index(&text, at);
        text.insert_str(byte, piece);
    }
    let characters = text.chars().count() + 1;
    let at = rng.below(characters) as u32;
    let to = if rng.chance(40) {
        at
    } else {
        rng.below(characters) as u32
    };
    Some(CommandCase {
        language,
        text,
        at,
        to,
        command: rng.below(9) as u8,
    })
}

pub(super) fn check(case: &CommandCase) -> Result<(), Failure> {
    let language = case.language;
    let text = &case.text;
    let range = selection(text, case.at, case.to);
    if case.command == 0 {
        let behavior = language.behavior();
        let Some((once, from, to)) = behavior.toggled_text(text, range.0, range.1) else {
            return Ok(());
        };
        let (from, to) = crate::editor::commands::toggled_selection(text, range, &once, (from, to));
        if from > to || to > units(&once) {
            return Err(Failure::new(
                format!("Ctrl+/ leaves its selection outside the text ({language})"),
                format!("selection {from}..{to} in {once:?}"),
            ));
        }
        return Ok(());
    }
    if case.command == 8 {
        let mut editor = TestEditor::new(language, text);
        editor.schema = Some(crate::editor::save_properties::schema());
        let (_owner, state) = editor.mount();
        state.set_caret_units(range.0);
        state.set_list_mode(crate::editor::completion::ListMode::TYPED);
        let options = state.completion().suggestions.get_untracked();
        if let Some(option) = options.first() {
            state.accept(option, false);
        }
        let caret = state.caret_units();
        let after = state.doc().text.get_untracked();
        if caret > units(&after) {
            return Err(Failure::new(
                format!("an accepted suggestion leaves the caret past the text's end ({language})"),
                format!("caret {caret} in {after:?}"),
            ));
        }
        return Ok(());
    }
    let command = match case.command {
        1 => Command::Lines(LineCommand::Delete),
        2 => Command::Lines(LineCommand::Move { down: false }),
        3 => Command::Lines(LineCommand::Move { down: true }),
        4 => Command::Lines(LineCommand::Copy { down: false }),
        5 => Command::Lines(LineCommand::Copy { down: true }),
        6 => Command::Indent { outdent: false },
        _ => Command::Indent { outdent: true },
    };
    let (_owner, state) = TestEditor::new(language, text).mount();
    state.set_caret_units(range.1);
    run_command(state, command, range, false);
    let after = state.doc().text.get_untracked();
    let caret = state.caret_units();
    if caret > units(&after) {
        return Err(Failure::new(
            format!("{command:?} leaves the caret past the text's end ({language})"),
            format!("caret {caret} in {after:?}"),
        ));
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: CommandCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn comment_and_line_commands_never_panic_and_keep_the_selection_in_the_text() {
    run(
        "commands",
        cases(800),
        &|rng| generate(rng, &LANGUAGES),
        &check,
    );
}

#[test]
fn comment_and_line_commands_keep_the_selection_in_the_text_in_custom_languages() {
    run(
        "commands-custom",
        cases(800),
        &|rng| generate(rng, &CUSTOM),
        &check,
    );
}
