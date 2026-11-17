//! The YAML reader's errors in plain words: what is missing or wrong and
//! how to write it, without the reader's own prefix and place (reported
//! on their own).

/// `error` with its message in plain words.
pub(super) fn told(mut error: super::ParseError) -> super::ParseError {
    error.message = plain(&error.message);
    error
}

/// The reader's `message` in plain words.
pub(in crate::editor::languages) fn plain(message: &str) -> String {
    let said = without_place(message);
    let said = said.as_str();
    if let Some(payload) = said.strip_prefix("failed to parse number: ") {
        return tag_words(payload);
    }
    if let Some(key) = said.split_once("duplicate key \"").and_then(|(_, rest)| rest.split('"').next()) {
        return format!("the key \"{key}\" is written twice in the same map: each key may be written only once");
    }
    if let Some(name) = said.strip_prefix("unknown anchor: ") {
        return format!("*{name} names an anchor that is not defined above: write &{name} on the value it should repeat");
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
        return format!(
            "unknown escape in a string: {character} (YAML knows \\n \\t \\r \\0 \\\\ \\\" \\/ and \\x, \\u or \\U with hex digits)"
        );
    } else {
        return said.to_string();
    };
    text.to_string()
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

/// A value that does not fit its tag (`!!int x`), told plainly.
fn tag_words(payload: &str) -> String {
    let (tag, value) = payload.split_once(' ').unwrap_or((payload, ""));
    let wanted = match tag {
        "!!int" => "an integer",
        "!!float" => "a number",
        "!!bool" => "true or false",
        "!!null" => "nothing or null",
        _ => "a value of that kind",
    };
    if value.trim().is_empty() {
        format!("the {tag} tag has no value: write {wanted} after it, or remove the tag")
    } else {
        format!("\"{}\" does not fit the {tag} tag: write {wanted}, or remove the tag", value.trim())
    }
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
