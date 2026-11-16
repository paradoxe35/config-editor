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
use super::super::keyboard::utf16_to_byte_index;
use super::super::paths::utf16_len;
use super::Change;
use crate::editor::text::byte_range;

/// The change input of `kind` made of `before` to give `after`: made on
/// `selection` (UTF-16 of `before`, either direction), leaving `typed`
/// (UTF-16 of `after`); `None` when the browser did not say (see the
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
    let kept = utf16_len(after).checked_sub(written)?;
    let to = utf16_len(before).checked_sub(kept)?;
    let from = if drop { to } else { start.min(written) };
    if from > to || from > written {
        return None;
    }
    let (from_before, to_before) = (
        utf16_to_byte_index(before, from),
        utf16_to_byte_index(before, to),
    );
    let (from_after, written_after) = (
        utf16_to_byte_index(after, from),
        utf16_to_byte_index(after, written),
    );
    let agree = before.get(..from_before)? == after.get(..from_after)?
        && before.get(to_before..)? == after.get(written_after..)?;
    agree.then(|| {
        Change::replace(
            before,
            byte_range(from_before..to_before),
            &after[from_after..written_after],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::input::UNKNOWN_SELECTION;
    use super::said;

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
        let change = said("aaa", "aaaa", (1, 1), (2, 2), "insertText");
        assert_eq!(edits(change), Some(vec![(1..1, "a".to_string())]));
        // Backspace after the second `a` of `aa`.
        let change = said("aa", "a", (2, 2), (1, 1), "deleteContentBackward");
        assert_eq!(edits(change), Some(vec![(1..2, String::new())]));
        // Delete before it.
        let change = said("aa", "a", (0, 0), (0, 0), "deleteContentForward");
        assert_eq!(edits(change), Some(vec![(0..1, String::new())]));
        // A selection typed over, made backward.
        let change = said("a: one", "a: 1", (6, 3), (4, 4), "insertText");
        assert_eq!(edits(change), Some(vec![(3..6, "1".to_string())]));
        // A word deleted backward, past characters of two units.
        let change = said("é😀 ab", "é😀 ", (6, 6), (4, 4), "deleteWordBackward");
        assert_eq!(edits(change), Some(vec![(7..9, String::new())]));
    }

    /// A drop inserts where it lands, its text left selected.
    #[test]
    fn a_drop_is_placed_where_it_landed() {
        let change = said("ab\ncd", "ab\nabcd", (0, 2), (3, 5), "insertFromDrop");
        assert_eq!(edits(change), Some(vec![(3..3, "ab".to_string())]));
    }

    /// Input that does not say what it did, or whose facts the texts do
    /// not confirm, has no change said.
    #[test]
    fn input_that_does_not_say_has_none() {
        assert!(said("ab", "ab!", UNKNOWN_SELECTION, (3, 3), "insertText").is_none());
        assert!(said("teh", "the", (3, 3), (3, 3), "insertReplacementText").is_none());
        // The caret said is not where the text was typed.
        assert!(said("ab", "xab", (2, 2), (3, 3), "insertText").is_none());
    }
}
