//! One popup per place: while the hover card documents a key, the
//! problems on its line show in the card, above its docs (as VS Code's
//! hover shows them), not in the problem tip. The problem tip F8 (or a tap)
//! showed at the caret shows whenever no popup is open.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::Popup;

/// The one-based place whose problems a card on zero-based `card_line`
/// lists: the tip's (under the pointer, or F8's) when on its line, else
/// its own (`own`: the caret's, for a card shown at the caret).
fn card_place(
    card_line: usize,
    own: Option<(usize, usize)>,
    shown: [Option<(usize, usize)>; 2],
) -> Option<(usize, usize)> {
    shown
        .into_iter()
        .flatten()
        .find(|(line, _)| *line == card_line + 1)
        .or(own)
}

impl EditorState {
    /// Where the problem tip points (tracked): the tip open, else F8's (or
    /// a tap's) while no popup is open or the card hides under the
    /// suggestion list.
    pub(crate) fn tip_place(&self) -> Option<(usize, usize)> {
        let current = || self.problems().current.get();
        let hidden_card = || !self.completion().suggestions.with(Vec::is_empty);
        self.popups().open.with(|open| match open {
            Some(Popup::Tip { place }) => Some(*place),
            None => current(),
            Some(Popup::Card { .. }) if hidden_card() => current(),
            _ => None,
        })
    }

    /// The place whose problems the hover card lists, while it shows
    /// (tracked).
    pub(crate) fn card_problem_place(&self) -> Option<(usize, usize)> {
        if !self.completion().suggestions.with(Vec::is_empty) {
            return None;
        }
        self.popups().open.with(|open| match open {
            Some(Popup::Card { card, .. }) => card.problems,
            _ => None,
        })
    }

    /// The place whose problems a card opening on zero-based `line` lists:
    /// those of the tip it replaces or of F8's when on its line, else
    /// `own`.
    pub(super) fn card_problems_for(
        &self,
        line: usize,
        own: Option<(usize, usize)>,
    ) -> Option<(usize, usize)> {
        let hovered = self.popups().hovered_problem();
        let current = self.problems().current.get_untracked();
        card_place(line, own, [hovered, current])
    }
}

#[cfg(test)]
mod tests {
    use super::card_place;

    /// A key with docs and a problem showed the problem tip and the hover
    /// card stacked at one place, the card hiding the problem. The card
    /// lists the problems on its line (under the pointer, F8's, or at the
    /// caret it was shown at); a problem on another line it does not.
    #[test]
    fn the_card_lists_the_problems_on_its_line() {
        // Hovered on the card's line (zero-based 2 is line 3).
        assert_eq!(card_place(2, None, [Some((3, 4)), None]), Some((3, 4)));
        // F8's on its line.
        assert_eq!(card_place(2, None, [None, Some((3, 1))]), Some((3, 1)));
        // On another line: the card lists none of them (or the caret's).
        assert_eq!(card_place(2, None, [Some((5, 1)), None]), None);
        assert_eq!(
            card_place(2, Some((3, 2)), [Some((5, 1)), None]),
            Some((3, 2))
        );
        // Shown at the caret: the caret's problems.
        assert_eq!(card_place(2, Some((3, 2)), [None, None]), Some((3, 2)));
    }
}
