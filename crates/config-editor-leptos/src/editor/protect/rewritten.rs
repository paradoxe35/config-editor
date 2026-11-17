//! Telling an edit that cuts a placeholder from one that only looks like
//! it. A change is found as the region between what the texts share before
//! and after it, and that region may start or end inside a token the edit
//! did not cut: two lines swapped (`a: «secret 3f2a»` and
//! `b: «secret 0b1c»`) share their last `»`, a line deleted or copied
//! next to one like it shares its first characters. Read as cutting the
//! token, the edit would take it whole, and the shared characters with it
//! (`«secret 3f2a`).
//!
//! The same change is also the region moved out to the edges of the tokens
//! it cuts (the characters it takes in are shared, so the texts are the
//! same): it then takes those tokens whole. That is what the edit did when,
//! for each token cut, the edit only moved text and the new text holds the
//! token whole where it wrote (lines swapped), or the characters shared
//! with it are part of a whole token's text in the new text (another token
//! in its place). A character typed or deleted inside a token, or a token
//! pasted over part of one, gives neither: what is left there is a piece
//! of a token.

use std::collections::{HashMap, HashSet};

use super::super::text::{TextRange, TextSize, slice, text_size};
use super::super::work_count::{Work, note_many};

/// `region` (of `before`; the edit to `after` replaced it) widened to the
/// edges of the tokens among `tokens` (ranges of `before`) it cuts, when
/// the edit took them whole (see the module docs); `None` when it cuts
/// none, or really cuts one.
pub(crate) fn rewritten_region(
    tokens: &[TextRange],
    before: &str,
    after: &str,
    region: &TextRange,
) -> Option<TextRange> {
    let inside = |at: TextSize| {
        tokens
            .iter()
            .find(|token| token.start() < at && at < token.end())
    };
    let (cut_start, cut_end) = (inside(region.start()), inside(region.end()));
    if cut_start.is_none() && cut_end.is_none() {
        return None;
    }
    let (old, new) = (before.as_bytes(), after.as_bytes());
    let inserted = (new.len() + usize::from(region.len())).checked_sub(old.len())?;
    let (region_start, region_end) = (usize::from(region.start()), usize::from(region.end()));
    let start = cut_start.map_or(region_start, |token| usize::from(token.start()));
    let end = cut_end.map_or(region_end, |token| usize::from(token.end()));
    // Where the wider region's text is in the new text, and the characters
    // it shares with the old one at each end.
    let inserted_end = region_start + inserted;
    let written = start..inserted_end + (end - region_end);
    let texts: Vec<&[u8]> = tokens
        .iter()
        .filter_map(|token| old.get(std::ops::Range::<usize>::from(*token)))
        .collect();
    // Whether an occurrence of `text` in the new text meets `test` (a
    // whole token's bytes start on a character).
    let occurs = |text: &[u8], near: &std::ops::Range<usize>, test: &dyn Fn(usize) -> bool| {
        let from = near.start.saturating_sub(text.len());
        let to = (near.end + text.len()).min(new.len());
        from + text.len() <= to
            && new[from..to]
                .windows(text.len())
                .enumerate()
                .any(|(at, window)| window == text && test(from + at))
    };
    // Whether the edit only put the old text's characters in another
    // order (lines moved): a paste writing a token next to a piece of one
    // adds characters.
    let rearranged = || {
        let (Some(from), Some(to)) = (
            slice::slice(before, TextRange::new(text_size(start), text_size(end))),
            slice::slice(
                after,
                TextRange::new(text_size(written.start), text_size(written.end)),
            ),
        ) else {
            return false;
        };
        let (mut from, mut to): (Vec<char>, Vec<char>) =
            (from.chars().collect(), to.chars().collect());
        from.sort_unstable();
        to.sort_unstable();
        from == to
    };
    let taken_whole = |token: &TextRange, shared: std::ops::Range<usize>| {
        let moved = || {
            old.get(std::ops::Range::<usize>::from(*token))
                .is_some_and(|text| {
                    rearranged()
                        && occurs(text, &written, &|at| {
                            at < written.end && at + text.len() > written.start
                        })
                })
        };
        let replaced = || {
            texts.iter().any(|text| {
                occurs(text, &shared, &|at| {
                    at <= shared.start && at + text.len() >= shared.end
                })
            })
        };
        moved() || replaced()
    };
    let start_whole = cut_start.is_none_or(|token| taken_whole(token, start..region_start));
    let end_whole = cut_end.is_none_or(|token| taken_whole(token, inserted_end..written.end));
    (start_whole && end_whole).then(|| TextRange::new(text_size(start), text_size(end)))
}

/// `region` (of `before`; the edit to `after` replaced it) widened over
/// the characters shared at its ends so that the text it wrote holds whole
/// every token of `texts` the new text has across its ends: a line moved
/// below another ending as it does (`a»` above `b: «secret 3f2a»`) writes
/// its token's last `»` in the shared part, and the token is only found
/// again by its text when the written text holds it all.
pub(crate) fn around_written(
    texts: &[&str],
    before: &str,
    after: &str,
    region: &TextRange,
) -> TextRange {
    let texts: Vec<&[u8]> = texts
        .iter()
        .filter(|text| !text.is_empty())
        .map(|text| text.as_bytes())
        .collect();
    let Some(longest) = texts.iter().map(|text| text.len()).max() else {
        return *region;
    };
    let (old_len, new_len) = (before.len(), after.len());
    let Some(inserted) = (new_len + usize::from(region.len())).checked_sub(old_len) else {
        return *region;
    };
    // Looked up by their text at each place, a few lengths: a document
    // holds thousands of placeholders, and each keystroke asks.
    let known: HashSet<&[u8]> = texts.iter().copied().collect();
    let mut lengths: Vec<usize> = texts.iter().map(|text| text.len()).collect();
    lengths.sort_unstable();
    lengths.dedup();
    let new = after.as_bytes();
    // How far past `edge` of the new text an occurrence of a token's text
    // across it reaches: before it (`start`), or after it. Each text's
    // first occurrence counts. A token's bytes start on a character, so
    // byte windows find exactly the whole tokens.
    let across = |edge: usize, start: bool| {
        let from = edge.saturating_sub(longest);
        let to = (edge + longest).min(new_len);
        let window = new.get(from..to).unwrap_or_default();
        let mut first: HashMap<&[u8], usize> = HashMap::new();
        for offset in 0..window.len() {
            let at = from + offset;
            if at >= edge {
                break;
            }
            note_many(Work::TokenScan, lengths.len());
            for &length in &lengths {
                if offset + length > window.len() || at + length <= edge {
                    continue;
                }
                let piece = &window[offset..offset + length];
                if known.contains(piece) {
                    first.entry(piece).or_insert(at);
                }
            }
        }
        first
            .iter()
            .map(|(text, at)| {
                if start {
                    edge - at
                } else {
                    at + text.len() - edge
                }
            })
            .max()
            .unwrap_or(0)
    };
    let (mut start, mut end) = (usize::from(region.start()), usize::from(region.end()));
    let mut written_end = start + inserted;
    loop {
        let before_by = across(start, true);
        let after_by = across(written_end, false).min(old_len - end);
        if before_by == 0 && after_by == 0 {
            return TextRange::new(text_size(start), text_size(end));
        }
        start -= before_by;
        end += after_by;
        written_end += after_by;
    }
}
