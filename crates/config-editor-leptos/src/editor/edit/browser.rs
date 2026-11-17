//! What browser input did, from what the browser said of it: the selection
//! before (`beforeinput`), the selection after, and the input's type. Text
//! an input writes ends where the caret is after it; what follows the
//! caret is what followed the replaced range; a backward deletion starts
//! at the caret. The two texts only confirm it.
//!
//! The browser says nothing usable for input without a `beforeinput`
//! (`execCommand`, a synthetic event: [`UNKNOWN_SELECTION`]), and for types
//! that replace text elsewhere than the selection (`insertReplacementText`
//! from a spelling fix, `historyUndo`, formatting): those are read off the
//! two texts by the caret instead (`Change::diffed_at`), as is input whose
//! facts the texts do not confirm.

use super::super::history::Selection;
use super::super::input::UNKNOWN_SELECTION;
use super::Change;
use crate::editor::text::{TextRange, TextSize, slice};

/// The change input of `kind` made of `before` to give `after`: made on
/// `selection` (bytes of `before`, either direction), leaving `typed`
/// (bytes of `after`); `None` when the browser did not say (see the
/// module).
pub(super) fn said(
    before: &str,
    after: &str,
    selection: Selection,
    typed: Selection,
    kind: &str,
) -> Option<Change> {
    if selection == UNKNOWN_SELECTION {
        return None;
    }
    // A drop inserts where it lands, not at the selection the drag left.
    let drop = kind == "insertFromDrop";
    let says = drop
        || matches!(
            kind,
            "insertText"
                | "insertLineBreak"
                | "insertParagraph"
                | "insertFromPaste"
                | "insertFromPasteAsQuotation"
                | "insertCompositionText"
                | "insertFromComposition"
                | "deleteCompositionText"
                | "deleteByComposition"
                | "deleteByCut"
                | "deleteByDrag"
                | "deleteContent"
                | "deleteContentBackward"
                | "deleteContentForward"
                | "deleteWordBackward"
                | "deleteWordForward"
                | "deleteSoftLineBackward"
                | "deleteSoftLineForward"
                | "deleteHardLineBackward"
                | "deleteHardLineForward"
                | "deleteEntireSoftLine"
        );
    if !says {
        return None;
    }
    let start = selection.0.min(selection.1);
    // Where what it wrote ends: the caret, the end of dropped text left
    // selected.
    let written = typed.0.max(typed.1);
    let kept = TextSize::of(after).checked_sub(written)?;
    let to = TextSize::of(before).checked_sub(kept)?;
    let from = if drop { to } else { start.min(written) };
    if from > to || from > written {
        return None;
    }
    let agree = slice::before(before, from)? == slice::before(after, from)?
        && slice::after(before, to)? == slice::after(after, written)?;
    agree.then(|| {
        Change::replace(
            before,
            TextRange::new(from, to),
            slice::slice(after, TextRange::new(from, written)).unwrap_or_default(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::history::Selection;
    use super::super::super::input::UNKNOWN_SELECTION;
    use super::super::super::keyboard::utf16_to_byte_index;
    use super::super::super::text::text_size;
    use super::said;

    /// The UTF-16 selection `(anchor, caret)` of `text` (as the browser
    /// counts it) in bytes.
    fn at(text: &str, (anchor, caret): (usize, usize)) -> Selection {
        let byte = |units| text_size(utf16_to_byte_index(text, units));
        (byte(anchor), byte(caret))
    }

    fn edits(change: Option<super::Change>) -> Option<Vec<(std::ops::Range<usize>, String)>> {
        change.map(|change| {
            change
                .edits()
                .map(|(range, text)| (range.into(), text.to_string()))
                .collect()
        })
    }

    /// Of two alike places, the one the browser typed at: the texts alone
    /// cannot tell `aa|a` + `a` from `a|aa` + `a`.
    #[test]
    fn typing_is_placed_where_the_browser_typed() {
        let change = said(
            "aaa",
            "aaaa",
            at("aaa", (1, 1)),
            at("aaaa", (2, 2)),
            "insertText",
        );
        assert_eq!(edits(change), Some(vec![(1..1, "a".to_string())]));
        // Backspace after the second `a` of `aa`.
        let change = said(
            "aa",
            "a",
            at("aa", (2, 2)),
            at("a", (1, 1)),
            "deleteContentBackward",
        );
        assert_eq!(edits(change), Some(vec![(1..2, String::new())]));
        // Delete before it.
        let change = said(
            "aa",
            "a",
            at("aa", (0, 0)),
            at("a", (0, 0)),
            "deleteContentForward",
        );
        assert_eq!(edits(change), Some(vec![(0..1, String::new())]));
        // A selection typed over, made backward.
        let change = said(
            "a: one",
            "a: 1",
            at("a: one", (6, 3)),
            at("a: 1", (4, 4)),
            "insertText",
        );
        assert_eq!(edits(change), Some(vec![(3..6, "1".to_string())]));
        // A word deleted backward, past characters of two units.
        let change = said(
            "é😀 ab",
            "é😀 ",
            at("é😀 ab", (6, 6)),
            at("é😀 ", (4, 4)),
            "deleteWordBackward",
        );
        assert_eq!(edits(change), Some(vec![(7..9, String::new())]));
    }

    /// A drop inserts where it lands, its text left selected.
    #[test]
    fn a_drop_is_placed_where_it_landed() {
        let change = said(
            "ab\ncd",
            "ab\nabcd",
            at("ab\ncd", (0, 2)),
            at("ab\nabcd", (3, 5)),
            "insertFromDrop",
        );
        assert_eq!(edits(change), Some(vec![(3..3, "ab".to_string())]));
    }

    /// Input that does not say what it did, or whose facts the texts do
    /// not confirm, has no change said.
    #[test]
    fn input_that_does_not_say_has_none() {
        assert!(
            said(
                "ab",
                "ab!",
                UNKNOWN_SELECTION,
                at("ab!", (3, 3)),
                "insertText"
            )
            .is_none()
        );
        assert!(
            said(
                "teh",
                "the",
                at("teh", (3, 3)),
                at("the", (3, 3)),
                "insertReplacementText"
            )
            .is_none()
        );
        // The caret said is not where the text was typed.
        assert!(
            said(
                "ab",
                "xab",
                at("ab", (2, 2)),
                at("xab", (3, 3)),
                "insertText"
            )
            .is_none()
        );
    }
}
