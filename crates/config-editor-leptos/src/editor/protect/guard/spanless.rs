//! Fields the language cannot locate (it gives `parse` and `serialize`, no
//! spans) while the edited text does not parse: no lock says where they
//! are, so the edit's lines are tried on the reference text instead. The
//! line diff from the reference text (the last that parsed) to the edited
//! text is a list of hunks; each hunk the edit touches is applied alone to
//! the reference text. When that parses, every pinned value must still read
//! as pinned (a locked value typed into while another line is broken, a
//! duplicate key added); when it does not (the hunk is the breakage), the
//! reference text without the hunk's lines must still hold them (the line
//! holding a locked value replaced or broken). A hunk that tells neither
//! way is let through: the edit making the text parse again decides
//! (`repair`), and a save waits for a text that parses
//! (`ProtectState::save_waits`).

use std::ops::Range;

use super::super::super::codec;
use super::super::super::diff::{DiffEdit, myers_edits};
use super::super::locate::LineIndex;
use super::edit::Edited;
use super::pins::pins_hold;
use super::{Blocked, pins_still_hold};

/// Refuses `edit`, whose text does not parse, when a hunk it touches
/// changes a pinned value the language cannot locate.
pub(super) fn hunks_keep_pins(edit: &Edited<'_>) -> Result<(), Blocked> {
    let protected = edit.protected;
    if protected.unplaced.is_empty() || protected.pins.is_empty() || edit.parsed().is_some() {
        return Ok(());
    }
    let Some(origin) = protected.origin.as_deref() else {
        return Ok(());
    };
    let old: Vec<&str> = origin.split('\n').collect();
    let new: Vec<&str> = edit.after.split('\n').collect();
    let lines = LineIndex::new(edit.after);
    let touched = lines.line_of(edit.new_region.start)..lines.line_of(edit.new_region.end) + 1;
    for (was, now) in hunks(&old, &new) {
        let touches = if now.is_empty() {
            touched.start <= now.start && now.start <= touched.end
        } else {
            now.start < touched.end && touched.start < now.end
        };
        if !touches {
            continue;
        }
        let rebuilt = |middle: &[&str]| {
            old[..was.start]
                .iter()
                .chain(middle)
                .chain(&old[was.end..])
                .copied()
                .collect::<Vec<&str>>()
                .join("\n")
        };
        let read = |text: String| codec::read(edit.behavior, &text, edit.reading_schema).ok();
        let document = read(rebuilt(&new[now.clone()])).or_else(|| read(rebuilt(&[])));
        if let Some(document) = document
            && pins_hold(&document, protected).is_some()
        {
            return pins_still_hold(&document, protected);
        }
    }
    Ok(())
}

/// The line ranges each change of the line diff from `old` to `new`
/// replaces, in `old` and in `new`; a change replacing as many lines as it
/// writes is a hunk per line (lines changed side by side are tried one by
/// one: a broken line next to a locked one does not take it along). One
/// hunk over everything when the texts differ too much to diff.
fn hunks(old: &[&str], new: &[&str]) -> Vec<(Range<usize>, Range<usize>)> {
    changes(old, new)
        .into_iter()
        .flat_map(|(was, now)| {
            if was.len() == now.len() {
                (0..was.len())
                    .map(|line| {
                        (
                            was.start + line..was.start + line + 1,
                            now.start + line..now.start + line + 1,
                        )
                    })
                    .collect()
            } else {
                vec![(was, now)]
            }
        })
        .collect()
}

/// The runs of changed lines of the line diff from `old` to `new`.
fn changes(old: &[&str], new: &[&str]) -> Vec<(Range<usize>, Range<usize>)> {
    let Some(edits) = myers_edits(old, new) else {
        return vec![(0..old.len(), 0..new.len())];
    };
    let mut found = Vec::new();
    let (mut at_old, mut at_new) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    for step in edits {
        match step {
            DiffEdit::Equal => {
                if let Some((from_old, from_new)) = open.take() {
                    found.push((from_old..at_old, from_new..at_new));
                }
                at_old += 1;
                at_new += 1;
            }
            DiffEdit::Delete => {
                open.get_or_insert((at_old, at_new));
                at_old += 1;
            }
            DiffEdit::Insert => {
                open.get_or_insert((at_old, at_new));
                at_new += 1;
            }
        }
    }
    if let Some((from_old, from_new)) = open {
        found.push((from_old..at_old, from_new..at_new));
    }
    found
}

#[cfg(test)]
mod tests {
    use super::hunks;

    #[test]
    fn hunks_are_the_runs_of_changed_lines() {
        let old = ["a", "b", "c", "d"];
        let new = ["a", "x", "c", "d", "e"];
        assert_eq!(hunks(&old, &new), [(1..2, 1..2), (4..4, 4..5)]);
        assert_eq!(hunks(&old, &["a", "d"]), [(1..3, 1..1)]);
        assert_eq!(
            hunks(&old, &["a", "x", "y", "d"]),
            [(1..2, 1..2), (2..3, 2..3)]
        );
    }
}
