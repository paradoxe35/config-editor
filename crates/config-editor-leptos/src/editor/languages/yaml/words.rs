//! The YAML reader's errors in plain words: what is missing or wrong and
//! how to write it, without the reader's own prefix and place (reported
//! on their own).

use crate::editor::language::Said;
use crate::editor::text::{TextSize, text_size};

/// `error`, a reading of `source`, with its message in plain words and
/// what it quotes of `source`; an error that says its quotes already (one
/// the editor words) is kept.
pub(super) fn told(source: &str, error: super::ParseError) -> super::ParseError {
    if error.quotes.is_some() {
        return error;
    }
    let at = error.line.zip(error.column).map(|(line, column)| text_size(byte_at(source, line, column)));
    worded(&error.message, source, at).at(error.line, error.column)
}

/// The reader's `message` in plain words.
pub(in crate::editor::languages) fn plain(message: &str) -> String {
    worded(message, "", None).text().to_string()
}

/// The reader's `message` in plain words, each part quoting `source`
/// with where: `at` is the byte the reader stopped at.
///
/// noyalib's own sentences name text of the document only in the ones
/// worded here (a key, an anchor, an escape, a tag's value) and in those
/// about a tag's suffix or an escape's code point, hidden whole where a
/// secret may be (see [`Said::naming`]); its others are fixed.
fn worded(message: &str, source: &str, at: Option<TextSize>) -> Said {
    let said = without_place(message);
    let said = said.as_str();
    // The line the reader stopped on: wider than what a sentence quotes
    // there, never narrower.
    let line_start = crate::editor::text::slice::up_to(source, at.unwrap_or_default()).rfind('\n').map_or(0, |end| end + 1);
    let here = || line_start..source[line_start..].find('\n').map_or(source.len(), |end| line_start + end);
    if let Some(payload) = said.strip_prefix("failed to parse number: ") {
        return tag_words(payload, source);
    }
    if let Some(key) = said.split_once("duplicate key \"").and_then(|(_, rest)| rest.split('"').next()) {
        return Said::from("the key \"")
            .quote(key, here())
            .words("\" is written twice in the same map: each key may be written only once");
    }
    if let Some(name) = said.strip_prefix("unknown anchor: ") {
        // A suggestion (`did you mean …`) names an anchor written elsewhere.
        let of = if name.contains(' ') { 0..source.len() } else { here() };
        return Said::from("*")
            .quote(name, of.clone())
            .words(" names an anchor that is not defined above: write &")
            .quote(name, of)
            .words(" on the value it should repeat");
    }
    if said.starts_with("tag suffix must not contain") || said.contains("U+") {
        return Said::naming(source, line_start, said);
    }
    let text = if said == "unterminated double-quoted string" {
        "a string in double quotes is never closed: a closing \" is missing"
    } else if said == "unterminated single-quoted string" {
        "a string in single quotes is never closed: a closing ' is missing"
    } else if said == "expected ',' or ']' in flow sequence" {
        "a comma or ] is missing: the items of a [list] are separated by commas, and the list ends with ]"
    } else if said == "expected ',' or '}' in flow mapping" {
        "a comma or } is missing: the entries of a {map} are separated by commas, and the map ends with }"
    } else if said.starts_with("inconsistent indentation") {
        "this line is indented differently from the lines above it: the lines of one map or list start in the same column"
    } else if said.starts_with("mapping values are not allowed") {
        "a second colon is not allowed here: put the value in quotes if it holds a colon, or write the inner map on its own indented lines"
    } else if said.starts_with("expected block sequence entry") {
        "this line does not continue the list: the items of a list each start with - in the same column"
    } else if said.starts_with("simple key was required") {
        "this line is not a key: value pair: write key: value, or indent it as part of the text above"
    } else if said.starts_with("reserved indicator") {
        "@ and ` cannot start a plain value: put the value in quotes"
    } else if said.starts_with("unexpected ']'") {
        "this ] closes no list: remove it"
    } else if said.starts_with("unexpected '}'") {
        "this } closes no map: remove it"
    } else if said.starts_with("expected block mapping key") {
        "this text does not belong here: nothing may follow a quoted value on its line"
    } else if said.starts_with("block sequence entries are not allowed") {
        "a list item cannot start here: write the list on the next lines, indented under its key"
    } else if let Some(character) = said.strip_prefix("unknown escape character ") {
        return Said::from("unknown escape in a string: ")
            .quote(character, here())
            .words(" (YAML knows \\n \\t \\r \\0 \\\\ \\\" \\/ and \\x, \\u or \\U with hex digits)");
    } else {
        return said.into();
    };
    text.into()
}

/// The byte of `source` at one-based `line` and character `column`.
fn byte_at(source: &str, line: usize, column: usize) -> usize {
    let start: usize = source.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum();
    let text = source[start..].split('\n').next().unwrap_or_default();
    start + text.char_indices().nth(column.saturating_sub(1)).map_or(text.len(), |(at, _)| at)
}

/// `message` without the reader's `YAML parse error at line L, column C: `
/// lead or ` at line L, column C` tail.
fn without_place(message: &str) -> String {
    let message = match message.strip_prefix("YAML parse error at line ") {
        Some(rest) => rest.split_once(": ").map_or(message, |(_, said)| said),
        None => message,
    };
    match message.rsplit_once(" at line ") {
        Some((said, place)) if place.contains(", column ") => said.to_string(),
        _ => message.to_string(),
    }
}

/// A value that does not fit its tag (`!!int x`), told plainly: the tag
/// is one of the reader's four (`!!int`, `!!float`, `!!bool`, `!!null`),
/// the value is quoted from `source`, from the tag to its line's end.
fn tag_words(payload: &str, source: &str) -> Said {
    let (tag, value) = payload.split_once(' ').unwrap_or((payload, ""));
    let wanted = match tag {
        "!!int" => "an integer",
        "!!float" => "a number",
        "!!bool" => "true or false",
        "!!null" => "nothing or null",
        _ => "a value of that kind",
    };
    if value.trim().is_empty() {
        return format!("the {tag} tag has no value: write {wanted} after it, or remove the tag").into();
    }
    let of = tag_place(source, &format!("failed to parse number: {payload}"))
        .map_or(0..source.len(), |(line, column)| {
            let from = byte_at(source, line, column);
            from..source[from..].find('\n').map_or(source.len(), |end| from + end)
        });
    Said::from("\"")
        .quote(value.trim(), of)
        .words(&format!("\" does not fit the {tag} tag: write {wanted}, or remove the tag"))
}

/// Where in `source` a tag's error is: the `payload`'s tag and value as
/// written (one-based line and character column), else the tag alone.
pub(super) fn tag_place(source: &str, message: &str) -> Option<(usize, usize)> {
    let payload = message.strip_prefix("failed to parse number: ")?;
    let (tag, value) = payload.split_once(' ').unwrap_or((payload, ""));
    for (index, line) in source.split('\n').enumerate() {
        let mut from = 0;
        while let Some(found) = line[from..].find(tag) {
            let start = from + found;
            let after = line[start + tag.len()..].trim_start();
            if value.trim().is_empty() || after.starts_with(value.trim()) {
                return Some((index + 1, line[..start].chars().count() + 1));
            }
            from = start + tag.len();
        }
    }
    None
}
