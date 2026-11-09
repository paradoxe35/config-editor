//! Enter in every language on the same kinds of lines, through what the
//! editor writes (`keys::enter_newline`): after an opener with a trailing
//! comment, on a blank line of a multi-line string, and before a bracket's
//! closer after an entry's comma. Each test lists every failing case.

use super::super::keys::enter_newline::newline_at;
use super::Language;

/// Where the caret is in a case.
const CARET: &str = "‸";

/// `source` (the caret at [`CARET`]) after Enter, the caret marked again.
fn entered(language: Language, marked: &str) -> String {
    let at = marked.find(CARET).expect("a caret");
    let source = format!("{}{}", &marked[..at], &marked[at + CARET.len()..]);
    let behavior = language.behavior();
    let bodies = behavior.string_body_lines(&source);
    let in_body = |line: usize| bodies.iter().any(|lines| lines.contains(&line));
    let cursor = source[..at].encode_utf16().count();
    let newline = newline_at(behavior, &source, cursor, &in_body);
    let start = at - newline.replaces_before;
    let end = (at + newline.replaces).min(source.len());
    format!(
        "{}{}{CARET}{}{}",
        &source[..start],
        &newline.text[..newline.caret],
        &newline.text[newline.caret..],
        &source[end..]
    )
}

fn check(cases: &[(Language, &str, &str)]) {
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(language, before, expected)| {
            let found = entered(*language, before);
            (found != *expected)
                .then(|| format!("{language} {before:?}: {found:?}, expected {expected:?}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Enter at the end of a line whose opener has a trailing comment indents
/// as without the comment; inside the comment's text it only keeps the
/// line's indentation.
#[test]
fn enter_after_an_opener_with_a_trailing_comment_indents() {
    check(&[
        (Language::YAML, "a: # c‸\n", "a: # c\n  ‸\n"),
        (Language::YAML, "a:  # c‸", "a:  # c\n  ‸"),
        (
            Language::YAML,
            "l:\n  - a: # c‸\n",
            "l:\n  - a: # c\n      ‸\n",
        ),
        (Language::YAML, "a: 1 # c‸\n", "a: 1 # c\n‸\n"),
        (Language::YAML, "a: # c‸ d\n", "a: # c\n‸ d\n"),
        (Language::YAML, "  # c‸\n", "  # c\n  ‸\n"),
        (Language::HCL, "b { # c‸\n}\n", "b { # c\n  ‸\n}\n"),
        (Language::HCL, "b { // c‸\n}\n", "b { // c\n  ‸\n}\n"),
        (Language::HCL, "l = [ # c‸\n]\n", "l = [ # c\n  ‸\n]\n"),
        (Language::TOML, "l = [ # c‸\n]\n", "l = [ # c\n  ‸\n]\n"),
        (Language::TOML, "[t] # c‸\n", "[t] # c\n‸\n"),
        (
            Language::XML,
            "<a> <!-- c -->‸\n</a>\n",
            "<a> <!-- c -->\n  ‸\n</a>\n",
        ),
    ]);
}

/// A blank line at column 0 inside a multi-line string's body: Enter keeps
/// the body's indentation (column 0 would end a YAML block scalar).
#[test]
fn enter_on_a_blank_line_of_a_string_body_keeps_the_bodys_indent() {
    check(&[
        (
            Language::YAML,
            "a: |\n  one\n‸\n  two\n",
            "a: |\n  one\n\n  ‸\n  two\n",
        ),
        (
            Language::YAML,
            "a: >\n    one\n‸\n    two\nb: 1\n",
            "a: >\n    one\n\n    ‸\n    two\nb: 1\n",
        ),
        (
            Language::HCL,
            "a = <<-EOT\n    one\n‸\n    two\n    EOT\n",
            "a = <<-EOT\n    one\n\n    ‸\n    two\n    EOT\n",
        ),
    ]);
}

/// Before a closer after an entry's comma (`[1, 2, |]`): the entries go on
/// one level deeper with no spacing left at the line's end, and the closer
/// on its own line at the opener's indentation, as for `[|]`.
#[test]
fn enter_before_a_closer_after_a_comma_opens_the_brackets() {
    check(&[
        (
            Language::JSON,
            "{\n  \"a\": [1, 2, ‸]\n}\n",
            "{\n  \"a\": [1, 2,\n    ‸\n  ]\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"a\": [1, 2,‸]\n}\n",
            "{\n  \"a\": [1, 2,\n    ‸\n  ]\n}\n",
        ),
        (Language::JSON, "{\"a\": 1, ‸}\n", "{\"a\": 1,\n  ‸\n}\n"),
        (Language::YAML, "l: [1, 2, ‸]\n", "l: [1, 2,\n  ‸\n]\n"),
        (Language::YAML, "m: {a: 1, ‸}\n", "m: {a: 1,\n  ‸\n}\n"),
        (Language::TOML, "l = [1, 2, ‸]\n", "l = [1, 2,\n  ‸\n]\n"),
        (Language::HCL, "l = [1, 2, ‸]\n", "l = [1, 2,\n  ‸\n]\n"),
        (Language::HCL, "m = { a = 1, ‸}\n", "m = { a = 1,\n  ‸\n}\n"),
        // Between entries the spacing goes too.
        (Language::JSON, "[1, ‸2]\n", "[1,\n  ‸2]\n"),
        (Language::YAML, "l: [1, ‸2]\n", "l: [1,\n  ‸2]\n"),
        // No comma: the closer comes down to the opener's indentation.
        (Language::JSON, "[1, 2‸]\n", "[1, 2\n‸]\n"),
    ]);
}
