//! Every language's text hooks on half-typed text holding characters of
//! several bytes (and two UTF-16 units): at every caret place, the
//! completion context, Enter, typing hooks, suggestions (some accepted one
//! after another) and hover never panic, and an accepted suggestion leaves
//! its selection inside the new text. Byte offsets cut inside a character,
//! and ranges read backwards from half-typed text, are the class this
//! guards.

use super::docs::{CUSTOM, LANGUAGES, document, literal, offset, units};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::TextPosition;
use crate::editor::completion::{schema_hover, schema_suggestions};
use crate::editor::languages::Language;

#[derive(Clone, Debug)]
pub(super) struct HookCase {
    pub(super) language: Language,
    pub(super) text: String,
    /// Where suggestions are accepted one after another (a character).
    pub(super) at: u32,
}

impl Case for HookCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn hook_repro() {{\n    super::hooks::replay(super::hooks::HookCase {{\n        language: {},\n        text: {}.into(),\n        at: {},\n    }});\n}}",
            super::docs::language_code(self.language),
            literal(&self.text),
            self.at,
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let characters: Vec<char> = self.text.chars().collect();
        let moved = |from: usize, to: usize| {
            let at = self.at as usize;
            (if at >= to {
                at - (to - from)
            } else {
                at.min(from)
            }) as u32
        };
        let cut = |from: usize, to: usize| Self {
            text: characters[..from].iter().chain(&characters[to..]).collect(),
            at: moved(from, to),
            ..self.clone()
        };
        let mut out = Vec::new();
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
        self.text.len() * 1000 + self.at as usize
    }
}

/// Characters of several bytes, put where keys, headers and values end.
const WIDE: [&str; 5] = ["é", "😀", "中", "ß", "\u{301}"];

/// Half-typed pieces of every language's syntax.
const HALF: [&str; 34] = [
    "[",
    "[[",
    "]",
    "[a.",
    ".",
    "<",
    "</",
    "<a ",
    "=\"",
    "<!--",
    "-->",
    "<!--->",
    "<![CDATA[",
    "\"",
    "'",
    "\"\"\"",
    "{",
    "}",
    ": ",
    " = ",
    "${",
    "<<EOT\n",
    "#",
    "//",
    "/*",
    "- ",
    "&a ",
    "*a",
    "|\n",
    "\\",
    "\n",
    " ",
    "\t",
    ",",
];

fn generate(rng: &mut Rng, languages: &[Language]) -> Option<HookCase> {
    let language = *rng.pick(languages);
    let mut text = document(rng, language);
    for _ in 0..1 + rng.below(8) {
        let piece = if rng.chance(50) {
            *rng.pick(&WIDE)
        } else {
            *rng.pick(&HALF)
        };
        let at = offset(&text, rng.position());
        let byte = crate::editor::keyboard::utf16_to_byte_index(&text, at);
        text.insert_str(byte, piece);
    }
    // Half-typed to the end: the text cut off at a character.
    if rng.chance(30) {
        let keep = rng.below(text.chars().count() + 1);
        text = text.chars().take(keep).collect();
    }
    let at = rng.position();
    Some(HookCase { language, text, at })
}

/// The caret places of `text` (UTF-16, on characters).
fn places(text: &str) -> Vec<usize> {
    let mut out = vec![0];
    let mut units = 0;
    for character in text.chars() {
        units += character.len_utf16();
        out.push(units);
    }
    out
}

pub(super) fn check(case: &HookCase) -> Result<(), Failure> {
    let (language, text) = (case.language, case.text.as_str());
    let behavior = language.behavior();
    let bundle = crate::editor::save_properties::schema();
    let end = units(text);
    for at in places(text) {
        // What a suggestion replaces, and the other places it renames,
        // run forwards inside the text.
        let context = behavior.completion_context(text, at);
        let ranges = std::iter::once((context.replace_start, context.replace_end))
            .chain(context.paired_names.iter().copied());
        for (start, stop) in ranges {
            if start > stop || stop > end {
                return Err(Failure::new(
                    format!("the completion context names a range outside the text ({language})"),
                    format!("{start}..{stop} at {at} in {text:?}"),
                ));
            }
        }
        let _ = behavior.newline(text, at);
        let _ = behavior.entry_started(text, at);
        let _ = behavior.lexical_state(text, at);
        for typed in ['>', '/', '"', ']', '}', '='] {
            let _ = behavior.auto_close(text, at, typed);
            let _ = behavior.complete_on_type(text, at, typed);
            let _ = behavior.reindent_on_type(text, at, typed);
        }
        let _ = behavior.toggled_text(text, at, at);
        let _ = schema_suggestions(&bundle, behavior, text, at);
    }
    for (index, line) in text.split('\n').enumerate() {
        let _ = behavior.highlight(line);
        for column in 1..=line.chars().count() + 1 {
            let position = TextPosition {
                line: index + 1,
                column,
            };
            let _ = schema_hover(&bundle, behavior, text, position);
        }
    }
    let _ = behavior.fold_ranges(text);
    // Suggestions accepted one after another, each where the last left
    // the caret.
    let mut current = text.to_string();
    let mut caret = offset(text, case.at);
    for round in 0..4 {
        let options = schema_suggestions(&bundle, behavior, &current, caret);
        let Some(option) = options.get(round % options.len().max(1)) else {
            break;
        };
        let (accepted, selection) = option.accept(&current);
        if selection.start > selection.end || selection.end > units(&accepted) {
            return Err(Failure::new(
                format!("an accepted suggestion selects outside the text ({language})"),
                format!(
                    "{:?} accepted at {caret} in {current:?} selects {selection:?} in {accepted:?}",
                    option.label()
                ),
            ));
        }
        current = accepted;
        caret = selection.end;
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: HookCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn language_hooks_never_panic_on_half_typed_text_of_wide_characters() {
    run("hooks", cases(60), &|rng| generate(rng, &LANGUAGES), &check);
}

#[test]
fn language_hooks_never_panic_on_half_typed_text_in_custom_languages() {
    run(
        "hooks-custom",
        cases(60),
        &|rng| generate(rng, &CUSTOM),
        &check,
    );
}
