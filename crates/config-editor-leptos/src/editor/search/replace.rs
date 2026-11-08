//! Replacing matches: the new document text, computed in one pass so the
//! editor commits it as one edit (one undo step, one `on_change`).

use std::ops::Range;

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
        .map(|replaced| (replaced.text, replaced.count, replaced.first))
}

/// What replacing every match did.
pub(in crate::editor) struct Replaced {
    pub(in crate::editor) text: String,
    pub(in crate::editor) count: usize,
    /// The byte range the first replacement took in `text`.
    pub(in crate::editor) first: Range<usize>,
    /// Matches left alone because `skip` refused them.
    pub(in crate::editor) skipped: usize,
}

/// The document with every match replaced except those `skip` refuses
/// (byte ranges of `text`, such as matches in read-only values); `None`
/// when no match was replaced.
pub(in crate::editor) fn replace_all_except(
    text: &str,
    matcher: &Matcher,
    replacement: &str,
    skip: &dyn Fn(&Range<usize>) -> bool,
) -> Option<Replaced> {
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    let mut count = 0;
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
        result.push_str(&text[copied..found.start]);
        if first.is_none() {
            first = Some(result.len()..result.len() + inserted.len());
        }
        result.push_str(&inserted);
        copied = found.end;
        count += 1;
    }
    let first = first?;
    result.push_str(&text[copied..]);
    Some(Replaced {
        text: result,
        count,
        first,
        skipped,
    })
}

/// The document with the match at `range` replaced, and the byte range the
/// replacement took.
pub(in crate::editor) fn replace_one(
    text: &str,
    matcher: &Matcher,
    range: Range<usize>,
    replacement: &str,
) -> (String, Range<usize>) {
    let inserted = matcher.replacement_for(text, &range, replacement);
    let mut result = String::with_capacity(text.len() + inserted.len());
    result.push_str(&text[..range.start]);
    result.push_str(&inserted);
    result.push_str(&text[range.end..]);
    (result, range.start..range.start + inserted.len())
}
