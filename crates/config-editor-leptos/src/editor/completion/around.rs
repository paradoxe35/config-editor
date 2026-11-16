//! The document around the caret while the text does not parse as it is:
//! a key being typed there, a dangling item separator before it, or a
//! syntax error elsewhere. Read so the values that decide a `oneOf` option
//! or an `if` (the object's `kind`) still decide the keys offered inside it.

use super::*;

/// The document `source` holds with the entry typed at `cursor_byte` left
/// out (or its line, where it is alone on it), when that parses; else,
/// with an error elsewhere, what the text before the entry holds, each
/// container it leaves open closed ([`Text::container_closers`]).
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
    // the text on screen out of the shared parse. Only the word typed,
    // where entries follow it on its line (`{| nodelay: true}`, a key typed
    // in front of an entry's indentation), so they stay.
    if let Some(text) = typed_word_left_out(behavior, source, line_start, cursor_byte, line_end)
        && let Ok(read) = counted_parse(behavior, &text)
    {
        return Some(read);
    }
    // Only the entry typed, where the line holds more (an item written on
    // one line), so the item stays in its place.
    let left_out = super::typed_entry::typed_entry_left_out(behavior, source, cursor_byte);
    if let Some(read) = left_out
        .as_ref()
        .and_then(|(text, _)| counted_parse(behavior, text).ok())
    {
        return Some(read);
    }
    let line_left_out = || {
        let text = caret_line_left_out(behavior, source, line_start, line_end);
        counted_parse(behavior, &text)
            .ok()
            .or_else(|| without_dangling_separator(before, after, behavior))
            .or_else(|| errors_left_blank(&text, before.matches('\n').count(), behavior))
    };
    match left_out {
        // The line holds more than the entry (an object on one line):
        // leaving the whole line out would read without that object.
        Some((_, start)) => start_closed(&source[..start], behavior).or_else(line_left_out),
        None => line_left_out().or_else(|| start_closed(before, behavior)),
    }
}

/// `source` without the word typed before the caret (back to a blank, a
/// bracket or a separator of the language), when what follows the caret
/// on its line is more text after a blank, or the word is empty and text
/// follows: the entries after the caret on its line are read with the
/// rest. `None` otherwise (the caret inside a word, or nothing after it).
fn typed_word_left_out(
    behavior: &dyn ConfigLanguage,
    source: &str,
    line_start: usize,
    cursor_byte: usize,
    line_end: usize,
) -> Option<String> {
    let rest = &source[cursor_byte..line_end];
    if rest.trim().is_empty() {
        return None;
    }
    let config = behavior.config();
    let ends_word = |character: char| {
        character.is_whitespace()
            || config
                .brackets
                .iter()
                .any(|(open, close)| *open == character || *close == character)
            || config.item_separator == Some(character)
            || config
                .separators
                .iter()
                .any(|separator| separator.contains(character))
    };
    let line = &source[line_start..cursor_byte];
    let word_start = line
        .char_indices()
        .rev()
        .find(|(_, character)| ends_word(*character))
        .map_or(0, |(at, character)| at + character.len_utf8());
    let typed = &line[word_start..];
    if !typed.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let start = line_start + word_start;
    Some(format!("{}{}", &source[..start], &source[cursor_byte..]))
}

/// `text` (the document with the caret's line left out) read with the
/// lines its syntax errors are on left blank, a few at most, never line
/// `caret_line` (0-based): what a text broken elsewhere holds around the
/// caret, after it too.
fn errors_left_blank(
    text: &str,
    caret_line: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<Value> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    for _ in 0..MAX_LINES_LEFT_OUT {
        let blanked = lines.join("\n");
        let error = match read_aside(behavior, &blanked) {
            Ok(read) => return Some(read),
            Err(error) => error,
        };
        // The error's line, or the last line with text before it (an
        // error at the end of the text is that of the line left open).
        let at = error
            .line?
            .checked_sub(1)?
            .min(lines.len().saturating_sub(1));
        let index = (0..=at)
            .rev()
            .find(|index| !lines[*index].trim().is_empty())?;
        if index == caret_line {
            return None;
        }
        lines[index] = "";
    }
    read_aside(behavior, &lines.join("\n")).ok()
}

/// The text with the caret's line (`line_start..line_end`, its line break
/// kept) left out, but for the list items it starts with the language's
/// [`LanguageConfig::item_marker`](crate::editor::language::LanguageConfig::item_marker)
/// (`- na|` keeps `-`): the item stays where it is, holding what is
/// written below it, so the items after it keep their places.
fn caret_line_left_out(
    behavior: &dyn ConfigLanguage,
    source: &str,
    line_start: usize,
    line_end: usize,
) -> String {
    let flow = crate::editor::flow_syntax::Flow::of(behavior);
    let line = &source[line_start..line_end];
    let mut content = line.trim_start_matches([' ', '\t']);
    while let Some(after) = flow.after_item_marker(content) {
        content = after.trim_start_matches([' ', '\t']);
    }
    let markers = line[..line.len() - content.len()].trim_end();
    format!("{}{markers}{}", &source[..line_start], &source[line_end..])
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
    counted_parse(behavior, &format!("{kept}{gap}{after}")).ok()
}

/// The document `start` (the text before the entry at the caret) holds,
/// a separator it ends in left out and the containers it leaves open
/// closed: on its last line, else each on a line of its own (a block
/// closed on its own line). A syntax error on a line above its last (a
/// value left unfinished, an entry the caret is not in) is left out, the
/// line blank, a few times at most; one the parser finds only at the end
/// (a quote left open above) leaves the entry the caret is in, from its
/// last line starting unindented, read alone.
fn start_closed(start: &str, behavior: &dyn ConfigLanguage) -> Option<Value> {
    let kept = start.trim_end();
    let start = behavior
        .config()
        .item_separator
        .and_then(|separator| kept.strip_suffix(separator))
        .unwrap_or(start);
    let mut text = start.to_string();
    let entry = last_unindented_entry(start);
    for _ in 0..=MAX_LINES_LEFT_OUT {
        let error = match closed(&text, behavior) {
            Ok(read) => return Some(read),
            Err(error) => error,
        };
        match error.line.and_then(|line| line_left_blank(&text, line)) {
            Some(blanked) => text = blanked,
            None => break,
        }
    }
    closed(entry?, behavior).ok()
}

/// `text` read with the containers it leaves open closed: on its last
/// line, else each on a line of its own.
fn closed(text: &str, behavior: &dyn ConfigLanguage) -> Result<Value, crate::ParseError> {
    let closers = behavior.container_closers(text);
    let error = match read_aside(behavior, &format!("{text}{}", closers.concat())) {
        Ok(read) => return Ok(read),
        Err(error) => error,
    };
    if closers.is_empty() {
        return Err(error);
    }
    let mut own_lines = text.to_string();
    for closer in &closers {
        own_lines.push('\n');
        own_lines.push_str(closer);
    }
    own_lines.push('\n');
    read_aside(behavior, &own_lines).map_err(|_| error)
}

/// `text` from its last line with text that starts unindented, when that
/// is not its first.
fn last_unindented_entry(text: &str) -> Option<&str> {
    let mut start = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with(|character: char| !character.is_whitespace()) {
            start = Some(at);
        }
        at += line.len();
    }
    start.filter(|start| *start > 0).map(|start| &text[start..])
}

/// Lines of syntax errors above the caret [`start_closed`] leaves out.
const MAX_LINES_LEFT_OUT: usize = 2;

/// `text` with its one-based `line` left blank, when that is a line with
/// text above its last.
fn line_left_blank(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let index = line.checked_sub(1).filter(|index| *index < last)?;
    if lines[index].trim().is_empty() {
        return None;
    }
    let mut kept = lines;
    kept[index] = "";
    Some(kept.join("\n"))
}

/// `text` parsed on the side (not the shared parse of the text on screen).
fn read_aside(behavior: &dyn ConfigLanguage, text: &str) -> Result<Value, crate::ParseError> {
    crate::editor::work_count::note_many(crate::editor::work_count::Work::ParsedBytes, text.len());
    counted_parse(behavior, text)
}

/// `text` (the document with what is typed left out) parsed, counted
/// ([`crate::editor::work_count::Work::AroundParse`]).
fn counted_parse(behavior: &dyn ConfigLanguage, text: &str) -> Result<Value, crate::ParseError> {
    crate::editor::work_count::note(crate::editor::work_count::Work::AroundParse);
    behavior.parse(text)
}
