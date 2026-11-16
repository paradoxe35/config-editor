//! The hover card: schema docs for the key under the pointer, shown after a
//! short pause (see `input::mouse_move`) and hidden as the pointer leaves.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::a11y::{
    caret_position, caret_problem_docs, caret_problems_said, hover_text, problems_at_caret,
};
use super::completion::{FieldHit, KeyHover, Part, Values, field_at, field_docs};
use super::components::HoverCard;
use super::positioning::hover_card_anchor;
use super::state::EditorState;

/// The hover part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct HoverState {
    pub(super) card: RwSignal<Option<HoverCard>>,
    /// The field waiting out the pause, and its line (zero-based).
    pub(super) pending: StoredValue<Option<(usize, FieldHit)>>,
    /// A field the card shows the docs of, and its line: the one it was
    /// shown for, or another part of its line found to have the same docs.
    pub(super) shown: StoredValue<Option<(usize, FieldHit)>>,
    /// The pointer is over the hover card.
    pub(super) card_entered: StoredValue<bool>,
    /// Bumped on each hide, so a card scheduled before it never shows.
    pub(super) token: StoredValue<u64>,
    /// The card was shown at the caret (`Ctrl+K Ctrl+I`, a tap), not by
    /// the pointer: the pointer leaving does not close it.
    pub(super) at_caret: StoredValue<bool>,
    /// Where the pointer was last over the text, in the window's pixels
    /// (see `hover_path`).
    pub(super) pointer: StoredValue<Option<(f64, f64)>>,
    /// The pointer is over the text (since its last move there, until it
    /// left it).
    pub(super) on_text: StoredValue<bool>,
    /// The pointer's last move over the text was on the way to the popup
    /// it can enter: on what the popup is for, in its gap, or heading for
    /// it from there (see `hover_path`).
    pub(super) on_way: StoredValue<bool>,
    /// Where (the window's x) the pointer was last on what the popup is
    /// for: the gap between the popup and its line reaches over to there
    /// (see `hover_path`).
    pub(super) anchor_x: StoredValue<Option<f64>>,
    /// Where the way to the popup started: the pointer's last place on
    /// what the popup is for, or in its gap (see `hover_path`).
    pub(super) way_start: StoredValue<Option<(f64, f64)>>,
    /// The line (zero-based) the pointer came onto a popup that lets it
    /// through along, while it stays under it (see `popup_under`).
    pub(super) under_line: StoredValue<Option<usize>>,
    /// Bumped on each move under such a popup: where the pointer stops
    /// decides (see `popup_under`).
    pub(super) under_token: StoredValue<u64>,
    /// The card's element while it shows: the keyboard reaches its links
    /// (see [`EditorState::keep_card_for`]).
    pub(super) card_ref: NodeRef<leptos::html::Div>,
    /// The focus is in the card: it is a dialog then, not a tooltip.
    pub(super) card_focused: RwSignal<bool>,
}

impl HoverState {
    pub(super) fn new() -> Self {
        Self {
            card: RwSignal::new(None),
            pending: StoredValue::new(None),
            shown: StoredValue::new(None),
            card_entered: StoredValue::new(false),
            token: StoredValue::new(0),
            at_caret: StoredValue::new(false),
            pointer: StoredValue::new(None),
            on_text: StoredValue::new(false),
            on_way: StoredValue::new(false),
            anchor_x: StoredValue::new(None),
            way_start: StoredValue::new(None),
            under_line: StoredValue::new(None),
            under_token: StoredValue::new(0),
            card_ref: NodeRef::new(),
            card_focused: RwSignal::new(false),
        }
    }
}

impl EditorState {
    /// Hides the card after a moment, unless the pointer gets there (to
    /// select its text, follow a link, scroll it); a waiting one now.
    pub(super) fn leave_hover(&self) {
        if self.hover().at_caret.get_value() {
            return;
        }
        if self.hover().card.with_untracked(Option::is_none) {
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

    /// The pointer is back on the card's key: a hiding it was waiting for
    /// is called off (the card stays, as in VS Code).
    pub(super) fn keep_hover(&self) {
        self.hover().on_way.set_value(true);
        if !self.hover().at_caret.get_value() {
            self.hover().token.update_value(|token| *token += 1);
        }
    }

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
        self.hover().card.set(Some(HoverCard {
            info,
            line,
            caret,
            anchor,
        }));
        self.hover().shown.set_value(Some((line, field)));
        self.hover().at_caret.set_value(true);
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
        let shown = (line, field.clone());
        if self
            .hover()
            .shown
            .with_value(|known| known.as_ref() == Some(&shown))
        {
            return self.hover().card.with_untracked(Option::is_some);
        }
        let Some(card) = self.hover().card.with_untracked(|card| {
            card.as_ref()
                .filter(|card| card.line == line)
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
            self.hover().shown.set_value(Some(shown));
        }
        same
    }

    /// The host replaced the text: a card about what was there goes. One
    /// that had the focus gives it back to the text, as `Escape` does.
    pub(super) fn hide_hover_for_new_text(&self) {
        let focused = self.hover().card_ref.get_untracked().is_some_and(|card| {
            let active = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.active_element());
            active.is_some_and(|active| card.contains(Some(&active)))
        });
        self.hide_hover();
        if focused && let Some(element) = self.textarea() {
            let _ = element.focus_in_place();
        }
    }

    pub(super) fn hide_hover(&self) {
        self.drop_card_problems();
        self.hover().token.update_value(|token| *token += 1);
        self.hover().pending.set_value(None);
        self.hover().shown.set_value(None);
        self.hover().card_entered.set_value(false);
        self.hover().at_caret.set_value(false);
        self.note_card_focus(false);
        if self.hover().card.with_untracked(Option::is_some) {
            // The card or a link in it had the focus (Tab, in `Ctrl+M`
            // mode; a second `Ctrl+K Ctrl+I`): the text takes it back
            // instead of the page.
            let focused = self.card_has_focus();
            self.hover().card.set(None);
            if focused && let Some(element) = self.textarea() {
                let _ = element.focus_in_place();
            }
        }
    }

    /// Records whether the focus is in the card (see
    /// [`HoverState::card_focused`]).
    pub(super) fn note_card_focus(&self, focused: bool) {
        if self.hover().card_focused.get_untracked() != focused {
            self.hover().card_focused.set(focused);
        }
    }

    /// Whether the focus is in the hover card (on it or one of its links).
    pub(super) fn card_has_focus(&self) -> bool {
        let active = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element());
        self.hover()
            .card_ref
            .get_untracked()
            .zip(active)
            .is_some_and(|(card, active)| card.contains(Some(&active)))
    }

    /// Whether the focus moving to `to` keeps the card: into it (Tab from
    /// the text to its links, in `Ctrl+M` mode, as to the problem tip's),
    /// or from it back to the text (Shift+Tab). The text's blur and the
    /// card's focus-out hide it otherwise.
    pub(super) fn keep_card_for(&self, to: Option<&web_sys::EventTarget>) -> bool {
        let Some(to) = to.and_then(|to| to.dyn_ref::<web_sys::Node>()) else {
            return false;
        };
        let into_card = self
            .hover()
            .card_ref
            .get_untracked()
            .is_some_and(|card| card.contains(Some(to)));
        let into_text = self
            .view()
            .textarea_ref
            .get_untracked()
            .is_some_and(|text| text.is_same_node(Some(to)));
        into_card || into_text
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
