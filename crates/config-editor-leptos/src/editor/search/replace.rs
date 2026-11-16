//! Replacing matches: the change every match replaced makes, found in one
//! pass so the editor commits it as one edit (one undo step, one
//! `on_change`).

use std::ops::Range;

use super::super::edit::Change;
use super::matcher::Matcher;

/// The document with every match replaced, how many were, and the byte
/// range the first replacement took in the result; `None` with no match.
#[cfg(test)]
pub(in crate::editor) fn replace_all(
    text: &str,
    matcher: &Matcher,
    replacement: &str,
) -> Option<(String, usize, Range<usize>)> {
    replace_all_except(text, matcher, replacement, &|_| false)
        .and_then(|replaced| Some((replaced.change.apply(text)?, replaced.count, replaced.first)))
}

/// What replacing every match did.
pub(in crate::editor) struct Replaced {
    /// Each match replaced, as one change.
    pub(in crate::editor) change: Change,
    pub(in crate::editor) count: usize,
    /// The byte range the first replacement took in the text after.
    pub(in crate::editor) first: Range<usize>,
    /// Matches left alone because `skip` refused them.
    pub(in crate::editor) skipped: usize,
}

/// Every match of `text` replaced except those `skip` refuses (byte ranges
/// of `text`, such as matches in read-only values); `None` when no match
/// was replaced.
pub(in crate::editor) fn replace_all_except(
    text: &str,
    matcher: &Matcher,
    replacement: &str,
    skip: &dyn Fn(&Range<usize>) -> bool,
) -> Option<Replaced> {
    let mut edits = Vec::new();
    let mut skipped = 0;
    let mut first = None;
    let mut from = 0;
    while let Some(found) = matcher.find_from(text, from) {
        from = Matcher::after(text, &found);
        if skip(&found) {
            skipped += 1;
            continue;
        }
        let inserted = matcher.replacement_for(text, &found, replacement);
        if first.is_none() {
            first = Some(found.start..found.start + inserted.len());
        }
        edits.push((found, inserted));
    }
    let first = first?;
    let count = edits.len();
    Some(Replaced {
        change: Change::replacing(text.len(), edits)?,
        count,
        first,
        skipped,
    })
}

/// The match at `range` of `text` replaced, and the byte range the
/// replacement takes in the text after.
pub(in crate::editor) fn replace_one(
    text: &str,
    matcher: &Matcher,
    range: Range<usize>,
    replacement: &str,
) -> (Change, Range<usize>) {
    let inserted = matcher.replacement_for(text, &range, replacement);
    let taken = range.start..range.start + inserted.len();
    (Change::replace_bytes(text, range, &inserted), taken)
}
