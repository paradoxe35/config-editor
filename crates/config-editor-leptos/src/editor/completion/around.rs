//! The document around the caret while the text does not parse as it is:
//! a key being typed there, a dangling item separator before it, or a
//! syntax error elsewhere. Read so the values that decide a `oneOf` option
//! or an `if` (the object's `kind`) still decide the keys offered inside it.

use super::*;

/// The document `source` holds with the entry typed at `cursor_byte` left
/// out (or its line, where it is alone on it), when that parses; else,
/// with an error elsewhere, what the text before the entry holds, each
/// container it leaves open closed ([`ConfigLanguage::container_closers`]).
pub(crate) fn document_around(
    source: &str,
    cursor_byte: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<Value> {
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |at| cursor_byte + at);
    let (before, after) = (&source[..line_start], &source[line_end..]);
    // Read aside: the text with the entry or line left out must not push
    // the text on screen out of the shared parse. Only the entry typed,
    // where the line holds more (an item written on one line), so the
    // item stays in its place.
    let left_out = super::typed_entry::typed_entry_left_out(behavior, source, cursor_byte);
    if let Some(read) = left_out
        .as_ref()
        .and_then(|(text, _)| behavior.parse(text).ok())
    {
        return Some(read);
    }
    let line_left_out = || {
        behavior
            .parse(&behavior.caret_line_left_out(source, line_start, line_end))
            .ok()
            .or_else(|| without_dangling_separator(before, after, behavior))
    };
    match left_out {
        // The line holds more than the entry (an object on one line):
        // leaving the whole line out would read without that object.
        Some((_, start)) => start_closed(&source[..start], behavior).or_else(line_left_out),
        None => line_left_out().or_else(|| start_closed(before, behavior)),
    }
}

/// The document `before` and `after` the caret's line make when the
/// entry before that line ended in the item separator for the entry being
/// typed (JSON's `"kind": "a",` then a new line): read without it, as the
/// container closes right after.
fn without_dangling_separator(
    before: &str,
    after: &str,
    behavior: &dyn ConfigLanguage,
) -> Option<Value> {
    let config = behavior.config();
    let separator = config.item_separator?;
    let kept = before.trim_end();
    let closes = after
        .trim_start()
        .chars()
        .next()
        .is_some_and(|next| config.brackets.iter().any(|(_, close)| *close == next));
    let kept = kept.strip_suffix(separator).filter(|_| closes)?;
    let gap = &before[kept.len() + separator.len_utf8()..];
    behavior.parse(&format!("{kept}{gap}{after}")).ok()
}

/// The document `start` (the text before the entry at the caret) holds,
/// a separator it ends in left out and the containers it leaves open
/// closed: on its last line, else each on a line of its own (a block
/// closed on its own line).
fn start_closed(start: &str, behavior: &dyn ConfigLanguage) -> Option<Value> {
    let kept = start.trim_end();
    let start = behavior
        .config()
        .item_separator
        .and_then(|separator| kept.strip_suffix(separator))
        .unwrap_or(start);
    let closers = behavior.container_closers(start);
    let on_line = format!("{start}{}", closers.concat());
    read_aside(behavior, &on_line).or_else(|| {
        let mut own_lines = start.to_string();
        for closer in &closers {
            own_lines.push('\n');
            own_lines.push_str(closer);
        }
        own_lines.push('\n');
        (!closers.is_empty())
            .then(|| read_aside(behavior, &own_lines))
            .flatten()
    })
}

/// `text` parsed on the side (not the shared parse of the text on screen).
fn read_aside(behavior: &dyn ConfigLanguage, text: &str) -> Option<Value> {
    crate::editor::work_count::note_many(crate::editor::work_count::Work::ParsedBytes, text.len());
    behavior.parse(text).ok()
}
