//! The history carried over a change the host made (a new controlled
//! `value`): each step's change is rebased over the host's (`Change::map`,
//! as operational transform does), so undo takes back only the user's own
//! edits and keeps the host's, as VS Code does after a file changed outside
//! the editor. A step whose edit touches the host's change cannot be
//! carried over: it and every older one go.

use super::super::edit::{Assoc, Change};
use super::Selection;
use crate::editor::text::TextSize;

/// `undo` (popping a step) and the host's change `host`, both made on the
/// text the host changed, carried past each other: `undo` made on the
/// host's text instead, and `host` made on the text `undo` gives back.
/// `None` when what `undo` rewrites touches what the host changed: an
/// insertion at the host's boundary, or an edit ending where the host's
/// starts, may be the same text the host wrote (a host putting back what
/// the user deleted), and carrying the step over would write it twice.
pub(super) fn carried(undo: &Change, host: &Change) -> Option<(Change, Change)> {
    if let (Some((mine, _)), Some((theirs, _))) = (undo.hull(), host.hull())
        && !(mine.end() < theirs.start() || theirs.end() < mine.start())
    {
        return None;
    }
    Some((undo.map(host, false)?, host.map(undo, true)?))
}

/// `selection` (bytes of the text `host` is made on) where the host's
/// change takes it: a position inside what it replaced goes to its start.
pub(super) fn mapped(host: &Change, (anchor, caret): Selection) -> Selection {
    let map = |at: TextSize| host.map_pos(at, Assoc::Before).pos;
    (map(anchor), map(caret))
}

#[cfg(test)]
mod tests {
    use super::super::super::edit::Change;
    use super::carried;

    /// `step` (a text before an edit that gave `old`) under the host's
    /// change of `old` into `new`: the step's text with the host's change,
    /// when it is carried over.
    fn carry(step: &str, old: &str, new: &str) -> Option<String> {
        let undo = Change::diffed(old, step);
        let host = Change::diffed(old, new);
        let (undo, _) = carried(&undo, &host)?;
        undo.apply(new)
    }

    #[test]
    fn a_state_takes_the_hosts_change_apart_from_its_own() {
        assert_eq!(
            carry(
                "a = \"x\"\nb = 2\n",
                "a = \"xZ\"\nb = 2\n",
                "a = \"xZ\"\nb = 3\n"
            )
            .as_deref(),
            Some("a = \"x\"\nb = 3\n")
        );
        assert_eq!(
            carry(
                "a = 1\nb = \"x\"\n",
                "a = 1\nb = \"xZ\"\n",
                "a = 9\nb = \"xZ\"\n"
            )
            .as_deref(),
            Some("a = 9\nb = \"x\"\n")
        );
        // Over the user's own edit: not carried over.
        assert_eq!(carry("a = \"x\"\n", "a = \"xZ\"\n", "a = \"host\"\n"), None);
    }

    /// A host change that touches the step's edit (the host putting back
    /// what the user deleted, or replacing the text around what the user
    /// typed) is no change apart from it: the step goes, so undo cannot
    /// write the host's text twice or take part of it away.
    #[test]
    fn a_step_touching_the_hosts_change_is_not_carried_over() {
        // (step's text, the text the host replaced, the host's text)
        let cases = [
            // Backspace at the end, the host puts the character back.
            (
                "a: 1\nb: 2\nc: 3\n",
                "a: 1\nb: 2\nc: \n",
                "a: 1\nb: 2\nc: 3\n",
            ),
            // A character typed, the host drops the block above and it.
            (
                "a:\n  x: 1\nb: 2\nc: 3\n",
                "a:\n  x: 1\nb: 2\nc: 33\n",
                "b: 2\nc: 3\n",
            ),
            // A line duplicated, the host takes the copy away.
            ("a: 1\nc: 3\n", "a: 1\nc: 3\nc: 3\n", "a: 1\nc: 3\n"),
            // A line deleted, the host puts it back.
            ("a: 1\nb: 2\nc: 3\n", "a: 1\nc: 3\n", "a: 1\nb: 2\nc: 3\n"),
            // Typed at the start, the host writes over it.
            ("x: 1\n", "yx: 1\n", "x: 1\n"),
        ];
        let mut failures = Vec::new();
        for (step, old, new) in cases {
            let carried = carry(step, old, new);
            if carried.is_some() {
                failures.push(format!(
                    "{step:?} under {old:?} -> {new:?}: carried as {carried:?}"
                ));
            }
        }
        // Apart from it: still carried.
        let apart = carry(
            "a: 1\nb: 2\nc: 3\n",
            "a: 1\nb: 2\nc: 33\n",
            "a: 5\nb: 2\nc: 33\n",
        );
        if apart.as_deref() != Some("a: 5\nb: 2\nc: 3\n") {
            failures.push(format!("apart: {apart:?}"));
        }
        // No change at all: the step as it was.
        if carry("a: \n", "a: 1\n", "a: 1\n").as_deref() != Some("a: \n") {
            failures.push("no change".to_string());
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
