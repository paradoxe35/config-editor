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

use std::ops::Range;

use super::super::keyboard::utf16_to_byte_index;

/// `region` (UTF-16 in `before`; the edit to `after` replaced it) widened
/// to the edges of the tokens among `tokens` (UTF-16 ranges in `before`) it
/// cuts, when the edit took them whole (see the module docs); `None` when
/// it cuts none, or really cuts one.
pub(crate) fn rewritten_region(
    tokens: &[Range<usize>],
    before: &str,
    after: &str,
    region: &Range<usize>,
) -> Option<Range<usize>> {
    let inside = |at: usize| {
        tokens
            .iter()
            .find(|token| token.start < at && at < token.end)
    };
    let (cut_start, cut_end) = (inside(region.start), inside(region.end));
    if cut_start.is_none() && cut_end.is_none() {
        return None;
    }
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let inserted = (new.len() + region.len()).checked_sub(old.len())?;
    let start = cut_start.map_or(region.start, |token| token.start);
    let end = cut_end.map_or(region.end, |token| token.end);
    // Where the wider region's text is in the new text, and the characters
    // it shares with the old one at each end.
    let inserted_end = region.start + inserted;
    let written = start..inserted_end + (end - region.end);
    let texts: Vec<&[u16]> = tokens.iter().map(|token| &old[token.clone()]).collect();
    // Whether an occurrence of `text` in the new text meets `test`.
    let occurs = |text: &[u16], near: &Range<usize>, test: &dyn Fn(usize) -> bool| {
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
        let (Some(from), Some(to)) = (old.get(start..end), new.get(written.clone())) else {
            return false;
        };
        let (mut from, mut to) = (from.to_vec(), to.to_vec());
        from.sort_unstable();
        to.sort_unstable();
        from == to
    };
    let taken_whole = |token: &Range<usize>, shared: Range<usize>| {
        let moved = || {
            let text = &old[token.clone()];
            rearranged()
                && occurs(text, &written, &|at| {
                    at < written.end && at + text.len() > written.start
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
    let start_whole = cut_start.is_none_or(|token| taken_whole(token, start..region.start));
    let end_whole = cut_end.is_none_or(|token| taken_whole(token, inserted_end..written.end));
    (start_whole && end_whole).then_some(start..end)
}

/// `region` (UTF-16 in `before`; the edit to `after` replaced it) widened
/// over the characters shared at its ends so that the text it wrote holds
/// whole every token of `texts` the new text has across its ends: a line
/// moved below another ending as it does (`a»` above `b: «secret 3f2a»`)
/// writes its token's last `»` in the shared part, and the token is only
/// found again by its text when the written text holds it all.
pub(crate) fn around_written(
    texts: &[&str],
    before: &str,
    after: &str,
    region: &Range<usize>,
) -> Range<usize> {
    let texts: Vec<Vec<u16>> = texts
        .iter()
        .filter(|text| !text.is_empty())
        .map(|text| text.encode_utf16().collect())
        .collect();
    let Some(longest) = texts.iter().map(Vec::len).max() else {
        return region.clone();
    };
    let old_len = before.encode_utf16().count();
    let new_len = after.encode_utf16().count();
    let Some(inserted) = (new_len + region.len()).checked_sub(old_len) else {
        return region.clone();
    };
    // How far past `edge` of the new text an occurrence of a token's text
    // across it reaches: before it (`start`), or after it.
    let across = |edge: usize, start: bool| {
        let from = edge.saturating_sub(longest);
        let to = (edge + longest).min(new_len);
        let window: Vec<u16> = utf16_slice(after, from..to).encode_utf16().collect();
        texts
            .iter()
            .filter_map(|text| {
                window
                    .windows(text.len())
                    .enumerate()
                    .map(|(at, piece)| (from + at, piece))
                    .find(|(at, piece)| *at < edge && at + text.len() > edge && piece == text)
                    .map(|(at, _)| {
                        if start {
                            edge - at
                        } else {
                            at + text.len() - edge
                        }
                    })
            })
            .max()
            .unwrap_or(0)
    };
    let (mut start, mut end) = (region.start, region.end);
    let mut written_end = region.start + inserted;
    loop {
        let before_by = across(start, true);
        let after_by = across(written_end, false).min(old_len - end);
        if before_by == 0 && after_by == 0 {
            return start..end;
        }
        start -= before_by;
        end += after_by;
        written_end += after_by;
    }
}

/// The UTF-16 `range` of `text`.
fn utf16_slice(text: &str, range: Range<usize>) -> &str {
    let start = utf16_to_byte_index(text, range.start);
    let end = utf16_to_byte_index(text, range.end).max(start);
    &text[start..end]
}
