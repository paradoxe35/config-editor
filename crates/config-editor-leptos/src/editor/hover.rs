//! The hover card's docs: for the key under the pointer or at the caret
//! (when it shows is the popups', see `popups`).

use leptos::prelude::*;

use super::a11y::{
    caret_position, caret_problem_docs, caret_problems_said, hover_text, problems_at_caret,
};
use super::completion::{FieldHit, KeyHover, Part, Values, field_at, field_docs};
use super::components::HoverCard;
use super::positioning::hover_card_anchor;
use super::state::EditorState;

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
            // On the key or the value, just past its end, or on the `<`
            // before an XML start tag's name.
            let field = [
                Some(column),
                column.checked_sub(1),
                past_tag_open(self.behavior(), text, line, column),
            ]
            .into_iter()
            .flatten()
            .find_map(|at| field_at(text, line, at, self.behavior()))?;
            let info = self.field_docs(text, &field)?;
            Some((line, field, info))
        });
        let Some((line, field, info)) = found else {
            return false;
        };
        // On a protected value its own message takes the place of its key's
        // docs, as under the pointer; on the key both show.
        let column = self
            .doc()
            .text
            .with_untracked(|text| caret_position(text, caret).1 - 1);
        if protected.is_some()
            && (field.part == Part::Value || column >= info.start_column + info.key.chars().count())
        {
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
        let anchor = hover_card_anchor(&element, line, info.start_column);
        self.show_card_at_caret(HoverCard {
            info,
            line,
            problems: caret,
            anchor,
        });
        self.popups().note_card_field(line, field);
        self.a11y().announce(said);
        true
    }

    /// The field under character `column` of zero-based `line` (see
    /// [`field_at`]): what the pointer's pause waits on, no schema read.
    /// None over blank space.
    pub(super) fn field_under_pointer(&self, line: usize, column: usize) -> Option<FieldHit> {
        self.doc().text.with_untracked(|text| {
            let line_text = text.split('\n').nth(line)?;
            if line_text
                .chars()
                .nth(column)
                .is_none_or(char::is_whitespace)
            {
                return None;
            }
            field_at(text, line, column, self.behavior())
        })
    }

    /// The card's docs of `field` in `text`: its key's, and over a value
    /// its field's with the value's own where the schema documents it.
    pub(super) fn field_docs(&self, text: &str, field: &FieldHit) -> Option<KeyHover> {
        self.config().schema.with_value(|schema| {
            field_docs(
                text,
                field,
                schema.as_deref()?,
                self.behavior(),
                Values::Any,
            )
        })
    }

    /// The docs the card shows for the field under character `column` of
    /// zero-based `line`, as `Ctrl+K Ctrl+I` and a tap there show them.
    /// None over blank space.
    pub(super) fn docs_under_pointer(&self, line: usize, column: usize) -> Option<KeyHover> {
        let field = self.field_under_pointer(line, column)?;
        self.doc()
            .text
            .with_untracked(|text| self.field_docs(text, &field))
    }

    /// Whether the card shows the docs of `field` on zero-based `line`:
    /// the field it was shown for, or another part of the line with the
    /// same docs (an element's end tag, a key's value), looked up once.
    pub(super) fn card_shows(&self, line: usize, field: &FieldHit) -> bool {
        let popups = self.popups();
        if popups.shows_field(line, field) {
            return popups.with_card(|card| card.is_some());
        }
        let Some(card) = popups.with_card(|card| {
            card.filter(|card| card.line == line)
                .map(|card| card.info.clone())
        }) else {
            return false;
        };
        let same = self
            .doc()
            .text
            .with_untracked(|text| self.field_docs(text, field))
            .is_some_and(|info| card.same_docs(&info));
        if same {
            popups.note_card_field(line, field.clone());
        }
        same
    }
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

    /// `Ctrl+K Ctrl+I` with the caret on the `<` of an XML tag found
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
