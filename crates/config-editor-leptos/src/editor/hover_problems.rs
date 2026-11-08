//! One popup per place: while the hover card documents a key, the
//! problems on its line show in the card, above its docs (as VS Code's
//! hover shows them), and the problem tip yields to it. Elsewhere (no
//! card, or a card on another line) the tip shows them as before.

use leptos::prelude::*;

use super::problem_hit::covering;
use super::state::EditorState;

/// The one-based place whose problems the card on zero-based `card_line`
/// lists: the one hovered (or F8's) when on its line, else the caret's
/// (`caret`, for a card shown at the caret).
fn card_place(
    card_line: usize,
    caret: Option<(usize, usize)>,
    shown: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    shown.filter(|(line, _)| *line == card_line + 1).or(caret)
}

/// Whether the tip for the problems at one-based `shown` yields to the card
/// on zero-based `card_line`, which lists them.
fn yields(card_line: Option<usize>, shown: (usize, usize)) -> bool {
    card_line.is_some_and(|card_line| shown.0 == card_line + 1)
}

impl EditorState {
    /// The place whose problems the tip would show: the one hovered (or a
    /// focused underline's), else where F8 or a tap went.
    fn shown_problem_place(&self) -> Option<(usize, usize)> {
        let problems = self.problems();
        problems.hovered.get().or_else(|| problems.current.get())
    }

    /// The zero-based line and caret place of the hover card while it is
    /// shown (it hides while suggestions are).
    fn shown_card(&self) -> Option<(usize, Option<(usize, usize)>)> {
        let card = self
            .hover()
            .card
            .with(|card| card.as_ref().map(|card| (card.line, card.caret)))?;
        self.completion()
            .suggestions
            .with(Vec::is_empty)
            .then_some(card)
    }

    /// Where the problem tip points, unless the hover card lists those
    /// problems (tracked).
    pub(super) fn tip_place(&self) -> Option<(usize, usize)> {
        let shown = self.shown_problem_place()?;
        let card_line = self.shown_card().map(|(line, _)| line);
        (!yields(card_line, shown)).then_some(shown)
    }

    /// The place whose problems the hover card lists, while it shows
    /// (tracked).
    pub(super) fn card_problem_place(&self) -> Option<(usize, usize)> {
        let (line, caret) = self.shown_card()?;
        card_place(line, caret, self.shown_problem_place())
    }

    /// The card goes while the pointer has left the problems it lists: they
    /// go with it, not on to the problem tip for the same place.
    pub(super) fn drop_card_problems(&self) {
        let Some(line) = self
            .hover()
            .card
            .with_untracked(|card| card.as_ref().map(|card| card.line))
        else {
            return;
        };
        let problems = self.problems();
        let listed = problems
            .hovered
            .get_untracked()
            .is_some_and(|(hovered, _)| hovered == line + 1);
        if listed && problems.tip_leaving.get_value() && !problems.tip_entered.get_value() {
            self.hide_problem_tip();
        }
    }

    /// Whether the card has a link to reach (its docs', or a problem's it
    /// lists), so the pointer leaving its key gives it a moment.
    pub(super) fn card_reachable(&self) -> bool {
        untrack(|| {
            let docs = self.hover().card.with(|card| {
                card.as_ref()
                    .is_some_and(|card| card.info.docs_url.is_some())
            });
            docs || self.card_problem_place().is_some_and(|(line, column)| {
                self.doc().text.with(|text| {
                    let line_text = text.split('\n').nth(line - 1).unwrap_or("");
                    self.problems().diagnostics.with(|problems| {
                        covering(problems, line, column, line_text)
                            .any(|problem| problem.docs_url.is_some())
                    })
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{card_place, yields};

    /// QA 22: a key with docs and a problem showed the problem tip and the
    /// hover card stacked at one place, the card hiding the problem. The
    /// card lists the problems on its line (hovered, F8's, or at the caret
    /// it was shown at) and the tip yields; a problem on another line keeps
    /// its own tip.
    #[test]
    fn the_card_lists_the_problems_on_its_line_and_the_tip_yields() {
        // Hovered on the card's line (zero-based 2 is line 3).
        assert_eq!(card_place(2, None, Some((3, 4))), Some((3, 4)));
        assert!(yields(Some(2), (3, 4)));
        // On another line: the card lists none of them (or the caret's).
        assert_eq!(card_place(2, None, Some((5, 1))), None);
        assert_eq!(card_place(2, Some((3, 2)), Some((5, 1))), Some((3, 2)));
        assert!(!yields(Some(2), (5, 1)));
        // No card: the tip shows.
        assert!(!yields(None, (3, 4)));
        // Shown at the caret: the caret's problems.
        assert_eq!(card_place(2, Some((3, 2)), None), Some((3, 2)));
    }
}
