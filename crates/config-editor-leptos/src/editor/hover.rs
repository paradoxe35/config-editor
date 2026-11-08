//! The hover card: schema docs for the key under the pointer, shown after a
//! short pause (see `input::mouse_move`) and hidden as the pointer leaves.

use leptos::prelude::*;

use super::a11y::{caret_position, caret_problems_said, hover_text, problems_at_caret};
use super::completion::key_hover;
use super::components::HoverCard;
use super::positioning::hover_card_style;
use super::state::EditorState;

/// How many characters before the caret `Ctrl+K Ctrl+I` looks for the key
/// of the value it is on.
const LOOK_BACK: usize = 400;

/// The hover part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct HoverState {
    pub(super) card: RwSignal<Option<HoverCard>>,
    /// The key waiting out the pause: (line, start column, key).
    pub(super) pending: StoredValue<Option<(usize, usize, String)>>,
    /// The pointer is over the hover card (one with a link it can reach).
    pub(super) card_entered: StoredValue<bool>,
    /// Bumped on each hide, so a card scheduled before it never shows.
    pub(super) token: StoredValue<u64>,
    /// The card was shown at the caret (`Ctrl+K Ctrl+I`, a tap), not by
    /// the pointer: the pointer leaving does not close it.
    pub(super) at_caret: StoredValue<bool>,
}

impl HoverState {
    pub(super) fn new() -> Self {
        Self {
            card: RwSignal::new(None),
            pending: StoredValue::new(None),
            card_entered: StoredValue::new(false),
            token: StoredValue::new(0),
            at_caret: StoredValue::new(false),
        }
    }
}

impl EditorState {
    /// Hides a card the pointer may be on its way to (one with a link)
    /// after a moment, unless it gets there; hides any other card now.
    pub(super) fn leave_hover(&self) {
        if self.hover().at_caret.get_value() {
            return;
        }
        if !self.card_reachable() {
            self.hide_hover();
            return;
        }
        let token = self.hover().token.get_value();
        super::positioning::schedule_hover(*self, move |state| {
            if state.hover().token.get_value() == token && !state.hover().card_entered.get_value() {
                state.hide_hover();
            }
        });
    }

    /// Shows the hover card for the key (or documented value) at the caret,
    /// as the pointer would, and says its docs to screen readers: the
    /// keyboard's way (`Ctrl+K Ctrl+I`) and a tap's to the docs. Returns
    /// whether there were docs to show.
    pub(super) fn show_hover_at_caret(&self) -> bool {
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
                    past_tag_open(text, line, column),
                ];
                let before = (0..column.saturating_sub(1)).rev().take(LOOK_BACK);
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
        self.hide_hover();
        let caret = problems_at_caret(*self);
        // The problems it lists, then its docs.
        let problems = caret_problems_said(*self);
        let docs = self.strings_untracked(|strings| hover_text(strings, &info));
        let said = if problems.is_empty() {
            docs
        } else {
            format!("{problems}. {docs}")
        };
        let style = hover_card_style(&element, line, info.start_column);
        self.hover().card.set(Some(HoverCard {
            info,
            line,
            caret,
            style,
        }));
        self.hover().at_caret.set_value(true);
        self.a11y().announce(said);
        true
    }

    pub(super) fn hide_hover(&self) {
        self.hover().token.update_value(|token| *token += 1);
        self.hover().pending.set_value(None);
        self.hover().card_entered.set_value(false);
        self.hover().at_caret.set_value(false);
        if self.hover().card.with_untracked(Option::is_some) {
            self.hover().card.set(None);
        }
    }
}

/// The column of the tag name after a `<` at character `column` of
/// zero-based `line` in `text` (an XML start tag), if one is there.
fn past_tag_open(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut rest = text.split('\n').nth(line)?.chars().skip(column);
    if rest.next()? != '<' {
        return None;
    }
    rest.next()
        .filter(|next| next.is_alphabetic() || *next == '_')?;
    Some(column + 1)
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
            let at = past_tag_open(text, line, column);
            let found = at.and_then(|at| hovered_key(text, line, at, behavior));
            let key = found.map(|(_, key, _)| key);
            let expected = if line == 0 { "config" } else { "listen" };
            assert_eq!(key.as_deref(), Some(expected), "{line}:{column}");
        }
        assert_eq!(past_tag_open(text, 1, 3), None);
        assert_eq!(past_tag_open(text, 1, 14), None);
        assert_eq!(past_tag_open("a: <b>", 0, 9), None);
    }
}
