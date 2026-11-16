//! The hover card's docs: for the key under the pointer or at the caret
//! (when it shows is the popups', see `popups`).

use leptos::prelude::*;

use super::a11y::{
    caret_position, caret_problem_docs, caret_problems_said, hover_text, problems_at_caret,
};
use super::completion::{KeyHover, key_hover};
use super::components::HoverCard;
use super::paths::split_line;
use super::positioning::hover_card_anchor;
use super::state::EditorState;

/// How many characters before the caret `Ctrl+K Ctrl+I` looks for the key
/// of the value it is on.
const LOOK_BACK: usize = 400;

impl EditorState {
    /// Shows the hover card for the key (or documented value) at the caret,
    /// as the pointer would, and says its docs to screen readers: the
    /// keyboard's way (`Ctrl+K Ctrl+I`) and a tap's to the docs. Returns
    /// whether there were docs to show.
    pub(super) fn show_hover_at_caret(&self, protected: Option<&str>) -> bool {
        if !self.config().settings.hover_docs.get_untracked() {
            return false;
        }
        let Some(element) = self.textarea() else {
            return false;
        };
        let caret = self.doc().caret.get_untracked();
        let found = self.doc().text.with_untracked(|text| {
            let (line, column) = caret_position(text, caret);
            let (line, column) = (line - 1, column - 1);
            self.config().schema.with_value(|schema| {
                // On the key, just past its end, or on the `<`
                // before an XML start tag's name; else on a value, its
                // field's: the nearest key before the caret on its line.
                let near = [
                    Some(column),
                    column.checked_sub(1),
                    past_tag_open(self.behavior(), text, line, column),
                ];
                // Only where the line writes a key (read off the line
                // alone): each look further reads the document.
                let line_text = split_line(text, line).unwrap_or_default();
                let before = (0..column.saturating_sub(1))
                    .rev()
                    .take(LOOK_BACK)
                    .filter(|at| self.behavior().hover_key(line_text, *at).is_some());
                near.into_iter()
                    .flatten()
                    .chain(before)
                    .find_map(|at| key_hover(text, line, at, schema.as_deref(), self.behavior()))
                    .map(|info| (line, info))
            })
        });
        let Some((line, info)) = found else {
            return false;
        };
        // On a protected value its own message takes the place of its key's
        // docs, as under the pointer; on the key both show.
        let column = self
            .doc()
            .text
            .with_untracked(|text| caret_position(text, caret).1 - 1);
        if protected.is_some() && column >= info.start_column + info.key.chars().count() {
            return false;
        }
        self.hide_hover();
        let caret = problems_at_caret(*self);
        // The problems it lists, then its docs.
        let problems = caret_problems_said(*self);
        // Alt+Enter opens a problem's link first.
        let docs_key = caret_problem_docs(*self).is_none();
        let docs = self.strings_untracked(|strings| hover_text(strings, &info, docs_key));
        let said = [
            protected.unwrap_or_default(),
            problems.as_str(),
            docs.as_str(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(". ");
        let anchor = hover_card_anchor(&element, line, self.cell_at(line, info.start_column));
        self.show_card_at_caret(HoverCard {
            info,
            line,
            problems: caret,
            anchor,
        });
        self.a11y().announce(said);
        true
    }

    /// The docs for the key (or documented value) under character
    /// `column` of zero-based `line`; on a value, its field's, as
    /// `Ctrl+K Ctrl+I` and a tap there show them: the nearest key before it
    /// on its line. None over blank space.
    pub(super) fn docs_under_pointer(&self, line: usize, column: usize) -> Option<KeyHover> {
        self.doc().text.with_untracked(|text| {
            let line_text = text.split('\n').nth(line)?;
            let chars: Vec<char> = line_text.chars().collect();
            if chars
                .get(column)
                .is_none_or(|character| character.is_whitespace())
            {
                return None;
            }
            self.config().schema.with_value(|schema| {
                let docs = |at| key_hover(text, line, at, schema.as_deref(), self.behavior());
                // The nearest key before it: looked up only where the line
                // writes one (read off the line alone), as each look reads
                // the document (a pointer over a long comment looked up
                // every word before it: seconds per move).
                let keyed = |at: usize| self.behavior().hover_key(line_text, at).is_some();
                docs(column).or_else(|| {
                    word_starts(&chars, column).take(LOOK_BACK).find_map(|at| {
                        [at, at + 1]
                            .into_iter()
                            .filter(|at| keyed(*at))
                            .find_map(docs)
                    })
                })
            })
        })
    }
}

/// The columns before `column` in `chars` where a word or a run of
/// non-blank characters starts, nearest first: where a key may start.
fn word_starts(chars: &[char], column: usize) -> impl Iterator<Item = usize> + '_ {
    let word = |character: char| character.is_alphanumeric() || character == '_';
    (0..column.min(chars.len())).rev().filter(move |&at| {
        let before = at.checked_sub(1).map(|before| chars[before]);
        let here = chars[at];
        let run = !here.is_whitespace() && before.is_none_or(char::is_whitespace);
        let starts_word = word(here) && !before.is_some_and(word);
        run || starts_word
    })
}

/// The column of the key after the markup opening it at character
/// `column` of zero-based `line` in `text` (an XML start tag's `<`), if
/// the language writes one there.
fn past_tag_open(
    behavior: &dyn super::language::ConfigLanguage,
    text: &str,
    line: usize,
    column: usize,
) -> Option<usize> {
    let line = text.split('\n').nth(line)?;
    let start = line.char_indices().nth(column)?.0;
    let length = behavior.key_markup_len(&line[start..])?;
    Some(column + line[start..start + length].chars().count())
}

#[cfg(all(test, feature = "xml"))]
mod tests {
    use super::past_tag_open;
    use crate::editor::completion::hovered_key;
    use crate::editor::languages::Language;

    /// QA 20: `Ctrl+K Ctrl+I` with the caret on the `<` of an XML tag found
    /// nothing: the name after the `<` is the key there.
    #[test]
    fn the_caret_on_a_tags_open_bracket_finds_its_element() {
        let text = "<config>\n  <listen>7835</listen>\n</config>\n";
        let behavior = Language::XML.behavior();
        for (line, column) in [(1, 2), (0, 0)] {
            let at = past_tag_open(behavior, text, line, column);
            let found = at.and_then(|at| hovered_key(text, line, at, behavior));
            let key = found.map(|(_, key, _)| key);
            let expected = if line == 0 { "config" } else { "listen" };
            assert_eq!(key.as_deref(), Some(expected), "{line}:{column}");
        }
        assert_eq!(past_tag_open(behavior, text, 1, 3), None);
        assert_eq!(past_tag_open(behavior, text, 1, 14), None);
        assert_eq!(past_tag_open(behavior, "a: <b>", 0, 9), None);
    }
}
