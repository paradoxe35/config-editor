//! The TOML parser's errors in plain words: what is missing or wrong and
//! how to write it, without the parser's "expected …" lists.

/// The parser's `message` at `line` and `column` of `source`, in plain
/// words.
pub(super) fn plain(source: &str, line: usize, column: usize, message: &str) -> String {
    let rest = rest_of_line(source, line, column);
    let text = if message.starts_with("string values must be quoted") {
        if rest.is_empty() {
            "a value is missing after the = sign"
        } else {
            "this is not a TOML value: write text in quotes, or a number, true, false, a date, a list [ … ] or a table { … }"
        }
    } else if message.starts_with("invalid multi-line basic string") {
        "a multi-line string is never closed: a closing \"\"\" is missing"
    } else if message.starts_with("invalid multi-line literal string") {
        "a multi-line string is never closed: a closing ''' is missing"
    } else if message.starts_with("invalid basic string") {
        "a string is never closed: a closing \" is missing (a string stays on one line)"
    } else if message.starts_with("invalid literal string") {
        "a string is never closed: a closing ' is missing (a string stays on one line)"
    } else if message.starts_with("unclosed array table") {
        "an [[array table]] header is never closed: ]] is missing"
    } else if message.starts_with("unclosed array") {
        "a list is never closed: a ] is missing"
    } else if message.starts_with("unclosed inline table") {
        "a { table is never closed: a } is missing"
    } else if message.starts_with("unclosed table") {
        "a [table] header is never closed: a ] is missing"
    } else if message.starts_with("key with no value") {
        "a key needs = and a value, as in key = value"
    } else if message.starts_with("duplicate key") {
        "this key or table is already written above: each may be written only once in its table"
    } else if message.starts_with("unexpected leading zero") {
        "a number must not start with a zero: write 7, not 07"
    } else if message.starts_with("invalid boolean") {
        "this word is no TOML value: write true or false, or put text in quotes"
    } else if message.starts_with("unquoted keys cannot be empty") {
        "a key is missing here: a key is made of letters, numbers, - and _, or is written in quotes"
    } else if message.starts_with("missing escaped value") || message.starts_with("invalid escape") {
        "unknown escape in a string: TOML knows \\b \\e \\f \\n \\r \\t \\\\ \\\" and \\x, \\u or \\U with hex digits"
    } else if message.starts_with("extra comma in array") {
        "a value is missing before this comma"
    } else if message.starts_with("unexpected key or value") {
        "this does not belong here: write one key = value per line"
    } else if message.starts_with("`_` may only go between digits") {
        "an underscore in a number must stand between two digits"
    } else if message.starts_with("integer number overflowed") {
        "this integer has no digits or is too large to read"
    } else if let Some(detail) = message.strip_prefix("invalid date, expected ") {
        return format!("this date or time is not valid: {detail} is expected");
    } else {
        return unexpected_free(message);
    };
    text.to_string()
}

/// A message not worded above, without the parser's trailing
/// `, expected …` list.
fn unexpected_free(message: &str) -> String {
    message.split(", expected ").next().unwrap_or(message).to_string()
}

/// The text of `line` from one-based `column` on, without a comment.
fn rest_of_line(source: &str, line: usize, column: usize) -> String {
    let text = source.split('\n').nth(line.saturating_sub(1)).unwrap_or_default();
    let rest: String = text.chars().skip(column.saturating_sub(1)).collect();
    rest.split('#').next().unwrap_or_default().trim().to_string()
}
