//! The HCL parser's errors in plain words: what is missing or wrong and
//! how to write it, as the other languages' problems are told. A cut-off
//! text (a string, heredoc, comment, `${` or bracket never closed) is
//! named and pointed at where it was opened.

use super::unclosed::{Open, first_open};

/// A parse error as shown: its line and column (one-based) and message.
pub(super) type Placed = (usize, usize, String);

/// The parser's `raw` message at `line` and `column` of `source`, in plain
/// words, at the place the words are about.
pub(super) fn plain(source: &str, line: usize, column: usize, raw: &str) -> Placed {
    let readable = raw_fallback(raw);
    let sentence = readable.as_str();
    let vague = sentence == "unexpected token"
        || sentence.starts_with("expected ")
        || sentence.starts_with("invalid expression")
        || sentence.starts_with("invalid block body");
    if vague
        && let Some((offset, open)) = first_open(source)
    {
        let (open_line, open_column) = super::line_column(source, offset);
        // A mistake on an earlier line is the first thing to fix.
        if open_line <= line {
            return (open_line, open_column, unclosed_words(&open));
        }
    }
    let at = char_at(source, line, column);
    let message = worded(sentence, at, source, line).unwrap_or_else(|| readable.clone());
    (line, column, message)
}

/// What a construct left open is told.
fn unclosed_words(open: &Open) -> String {
    match open {
        Open::String => "a string is never closed: a closing \" is missing (a string stays on one line)".to_string(),
        Open::Heredoc(word) => {
            format!("the heredoc <<{word} is never closed: a line holding only {word} must end it")
        }
        Open::Comment => "a /* comment is never closed: a */ is missing".to_string(),
        Open::Interpolation => "this ${ is never closed: a } is missing".to_string(),
        Open::Bracket(bracket) => {
            let (closing, what) = match bracket {
                '{' => ('}', "map or block"),
                '[' => (']', "list"),
                _ => (')', "function call or group"),
            };
            format!("this {bracket} is never closed: a {closing} is missing to end the {what}")
        }
    }
}

/// `sentence` (the parser's first line) in plain words, `at` the character
/// it stops at.
fn worded(sentence: &str, at: Option<char>, source: &str, line: usize) -> Option<String> {
    let operator = |c: Option<char>| c.is_some_and(|c| "+-*/%<>=!&|?:".contains(c));
    let text = if sentence == "unexpected token" {
        return Some(statement_words(source, line));
    } else if sentence == "expected newline or eof" {
        if operator(at) {
            "a value must follow this operator"
        } else if at == Some('.') {
            "a name must follow this dot, as in a.b"
        } else if at.is_none() {
            "something is missing at the end of this line"
        } else {
            "this does not belong here: an attribute holds one value, and the next attribute starts on a new line"
        }
    } else if sentence.starts_with("invalid expression") {
        if at.is_none() || at.is_some_and(char::is_whitespace) {
            "a value is missing here"
        } else if let Some(closer) = at.filter(|c| matches!(c, ')' | ']' | '}' | ',')) {
            return Some(format!("a value is missing before this {closer}"));
        } else if operator(at) {
            "a value is missing here: an operator must be followed by a value"
        } else {
            "no HCL value starts here: write text in double quotes, a number, true, false, null, a list [ … ], a map { … } or a name"
        }
    } else if let Some(closer) = ["]", ")", "}"].into_iter().find(|c| sentence == format!("expected `{c}`")) {
        return Some(match closer {
            "]" => "a ] is missing here: the items of a list are separated by commas and the list ends with ]".to_string(),
            ")" => "a ) is missing here: the arguments of a call are separated by commas and the call ends with )".to_string(),
            _ => "a } is missing here to end the map or block".to_string(),
        });
    } else if sentence == "expected `{`" {
        "a block is written as name \"label\" { … }: a { must follow its name and labels (an attribute is name = value)"
    } else if sentence == "expected `:`" {
        "a condition is written as test ? this : that: a : is missing between the two values"
    } else if sentence.starts_with("invalid block body") {
        "a block holds attributes (name = value) and blocks, one per line: this does not belong here"
    } else if sentence.starts_with("invalid object value assignment") {
        "a map entry needs = or : between its key and its value, as in key = value"
    } else if sentence.starts_with("invalid escape sequence") {
        "unknown escape in a string: HCL knows \\n \\r \\t \\\" \\\\ and \\u or \\U with hex digits"
    } else if sentence == "expected heredoc end delimiter" {
        "the heredoc is never closed: a line holding only its end word must end it"
    } else {
        return None;
    };
    Some(text.to_string())
}

/// Why a statement the parser refused outright (`unexpected token`, at its
/// start) is not one, judged from its line.
fn statement_words(source: &str, line: usize) -> String {
    let text = source.split('\n').nth(line.saturating_sub(1)).unwrap_or_default();
    let code = text.split(['#']).next().unwrap_or_default().split("//").next().unwrap_or_default().trim();
    let first = code.chars().next();
    if first == Some('}') {
        "this } closes nothing: remove it".to_string()
    } else if first.is_some_and(|c| !(c.is_alphabetic() || c == '_')) {
        "a name must start with a letter or an underscore".to_string()
    } else if code.ends_with('=') {
        "a value must follow the = sign".to_string()
    } else if code.ends_with(|c| "+-*/%<>!&|?:,".contains(c)) {
        "a value must follow this operator".to_string()
    } else if !code.contains('=') && !code.contains('{') {
        "an attribute needs = and a value, as in name = value (a block needs { … } after its name)".to_string()
    } else {
        "this line is neither an attribute (name = value) nor a block (name { … })".to_string()
    }
}

/// The parser's sentence without its source frame, for what is not worded.
fn raw_fallback(raw: &str) -> String {
    super::parse::readable_error(raw)
}

fn char_at(source: &str, line: usize, column: usize) -> Option<char> {
    source.split('\n').nth(line.checked_sub(1)?)?.chars().nth(column.checked_sub(1)?)
}
