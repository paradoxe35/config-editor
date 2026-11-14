//! The lines pasted since a save (whole lines holding text one edit
//! inserted, no later edit touched), where they are now: an item written
//! in them holding the data of an item followed or cut, under the same
//! key, is a copy of that item or its second paste, and takes its source
//! (QA `E37-5`: a copied TOML datetime was written as a string).

use super::Splice;
use std::ops::Range;

/// Whether `splice` pastes lines: inserts whole lines holding text.
pub(super) fn pastes(splice: &Splice) -> bool {
    splice.inserted.contains('\n') && !splice.inserted.trim().is_empty()
}

/// The ranges of the text the edits `splices` lead to that one of them
/// pasted and no later one touched.
pub(super) fn pasted(splices: &[Splice]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for splice in splices {
        let (at, end) = (splice.at, splice.at + splice.removed.len());
        let grown = splice.inserted.len() as isize - splice.removed.len() as isize;
        out.retain_mut(|range| {
            if end <= range.start && at < range.start || at == end && at == range.start {
                *range =
                    (range.start as isize + grown) as usize..(range.end as isize + grown) as usize;
                true
            } else {
                at >= range.end
            }
        });
        if pastes(splice) {
            out.push(at..at + splice.inserted.len());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splice(at: usize, removed: &str, inserted: &str) -> Splice {
        Splice {
            at,
            removed: removed.into(),
            inserted: inserted.into(),
        }
    }

    #[test]
    fn pasted_lines_follow_later_edits_and_go_once_touched() {
        let paste = splice(10, "", "a: 1\nb: 2\n");
        // Shifted by an edit before them, kept past one after them.
        assert_eq!(
            pasted(&[paste.clone(), splice(0, "x", "xyz"), splice(40, "q", "")]),
            vec![12..22]
        );
        // An insert where they start is before them.
        assert_eq!(
            pasted(&[paste.clone(), splice(10, "", "c: 3\n")]),
            vec![15..25, 10..15]
        );
        // Typed into: gone.
        assert_eq!(
            pasted(&[paste.clone(), splice(13, "1", "2")]),
            Vec::<Range<usize>>::new()
        );
        // Typed text and line breaks paste nothing.
        assert_eq!(
            pasted(&[splice(0, "", "x"), splice(0, "\n", "\n\n")]),
            Vec::<Range<usize>>::new()
        );
    }
}
