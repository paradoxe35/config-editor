//! The XML reader's errors in plain words: what is missing or wrong and
//! how to write it. A message that is plain already passes unchanged.

use std::ops::Range;

use crate::editor::language::Said;
use crate::editor::text::{TextSize, text_size};

/// The reader's `message`, about `source` where it stopped at byte `at`
/// (`open`: the name of the innermost element still open, where it is
/// written), in plain words, each part quoting `source` with where.
///
/// quick-xml's own sentences name text of the document only in the ones
/// worded here (tag names) and in a few rare ones (an entity's or a
/// declaration's name, a character reference), hidden whole where a
/// secret may be (see [`Said::naming`]); its others are fixed.
pub(super) fn told(message: &str, source: &str, at: TextSize, open: Option<Range<usize>>) -> Said {
    let message = message
        .strip_prefix("syntax error: ")
        .or_else(|| message.strip_prefix("ill-formed document: "))
        .unwrap_or(message);
    // `position 4: …`: the place is reported on its own.
    let message = match message.strip_prefix("position ") {
        Some(rest) => rest.split_once(": ").map_or(message, |(_, said)| said),
        None => message,
    };
    let at = usize::from(at).min(source.len());
    let line_start = crate::editor::text::slice::up_to(source, text_size(at))
        .rfind('\n')
        .map_or(0, |end| end + 1);
    let open = open.unwrap_or(0..source.len());
    if message.starts_with("entity or character reference not closed") {
        return "an & starts a reference that no ; ends: write a plain & as &amp;".into();
    }
    if let Some((what, ending)) = unclosed(message) {
        let what = match what {
            "tag" => "a tag",
            "attribute value" => "an attribute value",
            "comment" => "a comment",
            "CDATA" => "a CDATA section",
            "XML declaration" => "the XML declaration",
            "DOCTYPE" => "the DOCTYPE",
            "processing instruction" => "a processing instruction",
            // `start tag not closed: </name> not found`: the open element's.
            other => {
                return Said::from(format!("{other} is never closed: "))
                    .quote(&ending, open)
                    .words(" is missing");
            }
        };
        return format!("{what} is never closed: {ending} is missing").into();
    }
    if let Some(expected) = between(message, "expected `</", ">`, but `</")
        && let Some(found) = message.split("but `</").nth(1).and_then(|rest| rest.split(">`").next())
    {
        return Said::from("</")
            .quote(found, end_tag(source, text_size(at), found))
            .words("> does not match the open <")
            .quote(expected, open.clone())
            .words(">: close <")
            .quote(expected, open)
            .words("> first, or fix the name");
    }
    if let Some(tag) = between(message, "close tag `</", ">` does not match any open tag") {
        return Said::from("</")
            .quote(tag, end_tag(source, text_size(at), tag))
            .words("> closes an element that was never opened");
    }
    if message.starts_with("attribute value must be enclosed") {
        return "an attribute value must be in quotes, as in name=\"value\"".into();
    }
    if message.starts_with("attribute key must be directly followed") {
        return "an attribute needs = and a value in quotes, as in name=\"value\"".into();
    }
    let names_text = ["but in starts with", "unrecognized entity", "invalid character reference", "Cannot find ';'", "not a valid codepoint", "cannot decode"];
    if names_text.iter().any(|words| message.contains(words)) {
        return Said::naming(source, line_start, message);
    }
    message.into()
}

/// Where `source` writes the end tag name `name` the reader stopped on at
/// byte `at` (`</name`, its name read up to the next `>`, line breaks
/// too); the lines around `at` when it is not written there.
fn end_tag(source: &str, at: TextSize, name: &str) -> Range<usize> {
    let at = usize::from(at);
    let from = at + 2;
    match source.get(at..from) == Some("</") && source.get(from..).is_some_and(|rest| rest.starts_with(name)) {
        true => from..from + name.len(),
        false => around(source, text_size(at), name.len() + 3),
    }
}

/// The lines of `source` within `reach` bytes of byte `at` (an end tag's
/// name the reader quotes, which may hold a line break, is written there).
fn around(source: &str, at: TextSize, reach: usize) -> Range<usize> {
    let at = usize::from(at);
    let from = crate::editor::text::slice::char_start(source, text_size(at.saturating_sub(reach)));
    let to = crate::editor::text::slice::char_start(source, text_size((at + reach).min(source.len())));
    let (from, to) = (usize::from(from), usize::from(to));
    let start = source[..from].rfind('\n').map_or(0, |end| end + 1);
    start..source[to..].find('\n').map_or(source.len(), |end| to + end)
}

/// What `X not closed: \`Y\` not found before end of input` names: `X` and
/// `Y`.
fn unclosed(message: &str) -> Option<(&str, String)> {
    let (what, rest) = message.split_once(" not closed: `")?;
    let ending = rest.strip_suffix("` not found before end of input")?;
    Some((what, ending.to_string()))
}

/// The text of `message` between `start` and `end`.
fn between<'a>(message: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = message.split_once(start)?.1;
    rest.split(end).next()
}
