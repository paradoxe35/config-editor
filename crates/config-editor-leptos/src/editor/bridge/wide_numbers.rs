//! Integers past 64 bits typed in an Edit-as view: the view's own
//! language saves them as written, the problem still shown (QA `F30-3`),
//! and so does a view of a host that holds them as written (XML, whose
//! numbers are text): the save writes the digits (QA `E32-7`). A host
//! that refuses them (TOML) still waits.

use super::super::language::{ConfigLanguage, ParseError, with_edits};

/// `text` (in `edit`) with each number literal the editor cannot hold
/// exactly written as a string of the same characters, when those are all
/// that keep it from reading and `host` holds such a number as written;
/// `None` otherwise.
pub(super) fn as_text(
    edit: &dyn ConfigLanguage,
    host: &dyn ConfigLanguage,
    text: &str,
    error: &ParseError,
) -> Option<String> {
    if !host.holds_wide_integers() {
        return None;
    }
    // The language's one-character quote for strings (not a literal one).
    let config = edit.config();
    let quote = config
        .quotes
        .iter()
        .find(|quote| quote.chars().count() == 1 && !config.literal_quotes.contains(quote))?;
    let mut text = text.to_string();
    let mut error = error.clone();
    loop {
        let stand_ins: Vec<_> = edit
            .unheld_numbers(&text, &error)
            .into_iter()
            .filter(|range| !range.is_empty())
            .map(|range| {
                let written = format!("{quote}{}{quote}", &text[range.clone()]);
                (range, written)
            })
            .collect();
        if stand_ins.is_empty() {
            return None;
        }
        text = with_edits(&text, stand_ins);
        match edit.parse(&text) {
            Ok(_) => return Some(text),
            Err(next) => error = next,
        }
    }
}
