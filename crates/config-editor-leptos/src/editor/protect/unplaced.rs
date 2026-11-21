//! Tokens of a host text that does not parse whose fields its reading
//! cannot tell: an opener left unclosed before one (a list, a comment, a
//! character data section) carries the reading over it, so the field it
//! names is not where the token is written. Such a token is held by its
//! line while the text is broken (see `whole_guard`), and its field is
//! read where the host's own token, followed through the edits, is once
//! the text on screen parses: never tied to another field.

use std::ops::Range;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::paths::{api_units, field_key, path_at, written_value_span};
use super::super::text::{TextRange, TextSize, slice};
use super::tokens::{Known, field_value};

/// Whether the field at `pointer` of `text` (which does not parse) has its
/// value written where the token at bytes `token` is.
pub(super) fn written_in(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: &Range<usize>,
) -> bool {
    written_value_span(behavior, text, pointer)
        .and_then(|span| scalar_range(behavior, text, span))
        .is_some_and(|value| value.start <= token.start && token.end <= value.end)
}

/// Whether byte `at` of `text` is in a comment left open (it runs over the
/// rest of the text).
pub(super) fn in_open_comment(text: &str, comments: &[Range<usize>], at: TextSize) -> bool {
    let byte = usize::from(at);
    let next = comments.partition_point(|comment| comment.end <= byte);
    comments
        .get(next)
        .is_some_and(|comment| runs_open(text, comment, at))
}

/// Whether `range` of `text`, holding byte `at`, is a construct left open
/// there: it runs over lines after `at` to where the text ends (a closed
/// one is followed by more of the text, or ends on the line of `at`).
pub(super) fn runs_open(text: &str, range: &Range<usize>, at: TextSize) -> bool {
    let at = usize::from(at);
    range.start <= at
        && at < range.end
        && text
            .get(range.end..)
            .is_some_and(|rest| rest.trim().is_empty())
        && text
            .get(at..range.end)
            .is_some_and(|open| open.trim_end().contains('\n'))
}

/// `known` with each token whose field the host's text could not tell
/// (see `Known::placed`) placed in the field the parsed `document` (of
/// `text`, in `behavior`) holds it in where `tokens` (the ranges of the
/// real ones, the host's own followed through the edits) has it, whole
/// when it is alone there. `None` when every one is placed.
pub(crate) fn placed_where_written(
    behavior: &dyn ConfigLanguage,
    text: &str,
    document: &serde_json::Value,
    known: &[Known],
    tokens: &[TextRange],
) -> Option<Vec<Known>> {
    if known.iter().all(|known| known.placed) {
        return None;
    }
    // The places the placed ones hold, never taken again.
    let mut taken: std::collections::HashSet<(String, String)> = known
        .iter()
        .filter(|known| known.placed)
        .filter_map(|known| {
            Some((
                known.token.clone(),
                field_key(behavior, known.field.as_deref()?),
            ))
        })
        .collect();
    let mut out: Vec<Known> = known.iter().filter(|known| known.placed).cloned().collect();
    let mut printed = std::collections::HashMap::new();
    for unplaced in known.iter().filter(|known| !known.placed) {
        let mut left = unplaced.count;
        for range in tokens {
            if left == 0 {
                break;
            }
            if slice::slice(text, *range) != Some(unplaced.token.as_str()) {
                continue;
            }
            // The public language API takes UTF-16 carets.
            let caret = api_units(text, usize::from(range.start()));
            let Some(pointer) = path_at(behavior, text, caret).map(|path| path.to_pointer()) else {
                continue;
            };
            let Some(value) =
                field_value(behavior, document, &pointer).and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if !value.contains(&unplaced.token)
                || !taken.insert((unplaced.token.clone(), field_key(behavior, &pointer)))
            {
                continue;
            }
            left -= 1;
            out.push(Known {
                token: unplaced.token.clone(),
                field: Some(pointer.clone()),
                count: 1,
                whole: (value.trim() == unplaced.token).then(|| value.to_string()),
                items: super::follow_items::item_prints_in(document, &pointer, &mut printed),
                placed: true,
            });
        }
    }
    Some(out)
}
