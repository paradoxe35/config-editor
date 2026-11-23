//! What `F12` and `Shift+F12` say when they find nothing at the caret.
//! Where the schema names keys they are the editor's keys, so they never
//! fall through to the browser (whose `F12` opens its developer tools):
//! they say so at the caret instead, as VS Code does.

use leptos::prelude::*;

use crate::editor::state::EditorState;
use crate::editor::text::{TextSize, slice};
use crate::strings::fill;

/// The longest word a message quotes whole, in characters.
const MAX_QUOTED: usize = 40;

impl EditorState {
    /// Whether the schema names keys anywhere (`x-config-editor-keys`):
    /// then `F12` and `Shift+F12` are the editor's keys.
    pub(in crate::editor) fn names_keys(&self) -> bool {
        self.config()
            .schema
            .with_value(|schema| {
                schema
                    .as_ref()
                    .map(|schema| !super::pointers_in(&schema.schema).is_empty())
            })
            .unwrap_or(false)
    }

    /// `F12` found no definition at the caret: says so there.
    pub(in crate::editor) fn say_no_definition(&self) {
        let word = self.word_at_caret();
        let message = self.strings_untracked(|strings| match &word {
            Some(word) => fill(&strings.no_definition_for, &[("name", word)]),
            None => strings.no_definition_here.clone(),
        });
        self.note_nothing_found(message);
    }

    /// `Shift+F12` found nothing to list the uses of at the caret: says
    /// so there.
    pub(in crate::editor) fn say_no_uses(&self) {
        let word = self.word_at_caret();
        let message = self.strings_untracked(|strings| match &word {
            Some(word) => fill(&strings.no_uses_of, &[("name", word)]),
            None => strings.no_uses_here.clone(),
        });
        self.note_nothing_found(message);
    }

    fn word_at_caret(&self) -> Option<String> {
        let caret = self.caret_now();
        self.doc()
            .text
            .with_untracked(|text| word_at(text, caret).map(quoted))
    }
}

/// The word the caret at byte `at` is in or just past: letters, digits,
/// `_`, `-` and `.` (a name as config files write them).
fn word_at(text: &str, at: TextSize) -> Option<&str> {
    let is_word = |character: char| character.is_alphanumeric() || "_-.".contains(character);
    let before = slice::before(text, at)?;
    let after = slice::after(text, at)?;
    let start = before.len()
        - before
            .chars()
            .rev()
            .take_while(|c| is_word(*c))
            .map(char::len_utf8)
            .sum::<usize>();
    let end = before.len()
        + after
            .chars()
            .take_while(|c| is_word(*c))
            .map(char::len_utf8)
            .sum::<usize>();
    let range = crate::editor::text::TextRange::new(
        TextSize::try_from(start).ok()?,
        TextSize::try_from(end).ok()?,
    );
    slice::slice(text, range).filter(|word| !word.is_empty())
}

/// `word`, cut to [`MAX_QUOTED`] characters.
fn quoted(word: &str) -> String {
    if word.chars().count() <= MAX_QUOTED {
        word.to_string()
    } else {
        let mut cut: String = word.chars().take(MAX_QUOTED - 1).collect();
        cut.push('…');
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::{quoted, word_at};
    use crate::editor::text::TextSize;

    fn word(text: &str, at: u32) -> Option<&str> {
        word_at(text, TextSize::from(at))
    }

    #[test]
    fn the_word_at_the_caret_is_the_one_it_is_in_or_just_past() {
        let text = "listen: 7835\nname: café-1.x\n";
        assert_eq!(word(text, 0), Some("listen"));
        assert_eq!(word(text, 3), Some("listen"));
        assert_eq!(word(text, 6), Some("listen"));
        assert_eq!(word(text, 7), None);
        assert_eq!(word(text, 8), Some("7835"));
        assert_eq!(word(text, 12), Some("7835"));
        assert_eq!(word(text, 13), Some("name"));
        // Just past the two-byte é.
        assert_eq!(word(text, 24), Some("café-1.x"));
        assert_eq!(word("", 0), None);
        assert_eq!(word("a: b", 99), None);
    }

    #[test]
    fn a_long_word_is_cut() {
        assert_eq!(quoted("short"), "short");
        let long = "x".repeat(60);
        let cut = quoted(&long);
        assert_eq!(cut.chars().count(), 40);
        assert!(cut.ends_with('…'));
    }
}
