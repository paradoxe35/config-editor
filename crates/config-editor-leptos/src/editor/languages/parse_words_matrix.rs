//! Every language's syntax errors in plain words: cut-off and broken
//! snippets per language, each told in the editor's words (what is wrong
//! and how to write it) at the place the mistake is, never in the
//! parser's ("unexpected token", "expected `]`", "integer number
//! overflowed").

use super::Language;

/// Snippets and the message each is told with, and its line and column.
type Case = (&'static str, &'static str, usize, usize);

const HCL: &[Case] = &[
    (
        "a = \"x",
        "a string is never closed: a closing \" is missing (a string stays on one line)",
        1,
        5,
    ),
    (
        "a = foo(",
        "this ( is never closed: a ) is missing to end the function call or group",
        1,
        8,
    ),
    ("a = 1 +", "a value must follow this operator", 1, 7),
    (
        "a = [1, 2",
        "this [ is never closed: a ] is missing to end the list",
        1,
        5,
    ),
    (
        "a = {",
        "this { is never closed: a } is missing to end the map or block",
        1,
        5,
    ),
    (
        "a {",
        "this { is never closed: a } is missing to end the map or block",
        1,
        3,
    ),
    (
        "a = <<EOT\nx\n",
        "the heredoc <<EOT is never closed: a line holding only EOT must end it",
        1,
        5,
    ),
    (
        "a = /* x",
        "a /* comment is never closed: a */ is missing",
        1,
        5,
    ),
    (
        "a = \"${x\"",
        "this ${ is never closed: a } is missing",
        1,
        6,
    ),
    (
        "a",
        "an attribute needs = and a value, as in name = value (a block needs { … } after its name)",
        1,
        1,
    ),
    ("a =", "a value must follow the = sign", 1, 1),
    ("}", "this } closes nothing: remove it", 1, 1),
    (
        "1a = 2",
        "a name must start with a letter or an underscore",
        1,
        1,
    ),
    ("a = )", "a value is missing before this )", 1, 5),
    ("a = [1,,2]", "a value is missing before this ,", 1, 8),
    (
        "a = 1 + * 2",
        "a value is missing here: an operator must be followed by a value",
        1,
        9,
    ),
    ("a = x.", "a name must follow this dot, as in a.b", 1, 6),
    (
        "a = 1 2",
        "this does not belong here: an attribute holds one value, and the next attribute starts on a new line",
        1,
        7,
    ),
    (
        "a b c",
        "a block is written as name \"label\" { … }: a { must follow its name and labels (an attribute is name = value)",
        1,
        6,
    ),
    (
        "a = 1 ? 2",
        "a condition is written as test ? this : that: a : is missing between the two values",
        1,
        10,
    ),
    (
        "a = {b 1}",
        "a map entry needs = or : between its key and its value, as in key = value",
        1,
        9,
    ),
    (
        "a = \"\\q\"",
        "unknown escape in a string: HCL knows \\n \\r \\t \\\" \\\\ and \\u or \\U with hex digits",
        1,
        8,
    ),
    // The first mistake is the one told.
    (
        "a = 1 2\nb = [",
        "this does not belong here: an attribute holds one value, and the next attribute starts on a new line",
        1,
        7,
    ),
    (
        "a = 1\nb = [\n",
        "this [ is never closed: a ] is missing to end the list",
        2,
        5,
    ),
];

const TOML: &[Case] = &[
    (
        "a = \"x",
        "a string is never closed: a closing \" is missing (a string stays on one line)",
        1,
        7,
    ),
    (
        "a = 'x",
        "a string is never closed: a closing ' is missing (a string stays on one line)",
        1,
        7,
    ),
    (
        "a = \"\"\"x",
        "a multi-line string is never closed: a closing \"\"\" is missing",
        1,
        9,
    ),
    ("a = [1, 2", "a list is never closed: a ] is missing", 1, 10),
    (
        "a = {b = 1",
        "a { table is never closed: a } is missing",
        1,
        11,
    ),
    (
        "[a",
        "a [table] header is never closed: a ] is missing",
        1,
        3,
    ),
    (
        "[[a]",
        "an [[array table]] header is never closed: ]] is missing",
        1,
        5,
    ),
    ("a =", "a value is missing after the = sign", 1, 4),
    ("a", "a key needs = and a value, as in key = value", 1, 2),
    (
        "a = 1 2",
        "this is not a TOML value: write text in quotes, or a number, true, false, a date, a list [ … ] or a table { … }",
        1,
        5,
    ),
    (
        "a = @",
        "this is not a TOML value: write text in quotes, or a number, true, false, a date, a list [ … ] or a table { … }",
        1,
        5,
    ),
    (
        "a = 0x",
        "this integer has no digits or is too large to read",
        1,
        5,
    ),
    (
        "a = 01",
        "a number must not start with a zero: write 7, not 07",
        1,
        5,
    ),
    (
        "a = 1_",
        "an underscore in a number must stand between two digits",
        1,
        6,
    ),
    (
        "a = tru",
        "this word is no TOML value: write true or false, or put text in quotes",
        1,
        5,
    ),
    (
        "a = 1979-13-01",
        "this date or time is not valid: month between 01 and 12 is expected",
        1,
        5,
    ),
    (
        "= 1",
        "a key is missing here: a key is made of letters, numbers, - and _, or is written in quotes",
        1,
        1,
    ),
    (
        "[]",
        "a key is missing here: a key is made of letters, numbers, - and _, or is written in quotes",
        1,
        2,
    ),
    ("a = [1,,2]", "a value is missing before this comma", 1, 8),
    (
        "a = 1 = 2",
        "this does not belong here: write one key = value per line",
        1,
        7,
    ),
    (
        "a = 1\na = 2",
        "this key or table is already written above: each may be written only once in its table",
        2,
        1,
    ),
    (
        "a = \"\\q\"",
        "unknown escape in a string: TOML knows \\b \\e \\f \\n \\r \\t \\\\ \\\" and \\x, \\u or \\U with hex digits",
        1,
        7,
    ),
];

const YAML: &[Case] = &[
    (
        "a: !!int x",
        "\"x\" does not fit the !!int tag: write an integer, or remove the tag",
        1,
        4,
    ),
    (
        "a: !!float x",
        "\"x\" does not fit the !!float tag: write a number, or remove the tag",
        1,
        4,
    ),
    (
        "a: !!bool x",
        "\"x\" does not fit the !!bool tag: write true or false, or remove the tag",
        1,
        4,
    ),
    (
        "a: !!int",
        "the !!int tag has no value: write an integer after it, or remove the tag",
        1,
        4,
    ),
    (
        "a: \"x",
        "a string in double quotes is never closed: a closing \" is missing",
        1,
        6,
    ),
    (
        "a: 'x",
        "a string in single quotes is never closed: a closing ' is missing",
        1,
        6,
    ),
    (
        "a: [1, 2",
        "a comma or ] is missing: the items of a [list] are separated by commas, and the list ends with ]",
        1,
        9,
    ),
    (
        "a: {b: 1",
        "a comma or } is missing: the entries of a {map} are separated by commas, and the map ends with }",
        1,
        9,
    ),
    (
        "a: 1\n b: 2",
        "this line is indented differently from the lines above it: the lines of one map or list start in the same column",
        2,
        3,
    ),
    (
        "a: 1\na: 2",
        "the key \"a\" is written twice in the same map: each key may be written only once",
        2,
        1,
    ),
    (
        "a: b: c",
        "a second colon is not allowed here: put the value in quotes if it holds a colon, or write the inner map on its own indented lines",
        1,
        5,
    ),
    (
        "- a\nb: 1",
        "this line does not continue the list: the items of a list each start with - in the same column",
        2,
        1,
    ),
    (
        "a: *x",
        "*x names an anchor that is not defined above: write &x on the value it should repeat",
        1,
        4,
    ),
    (
        "a: @x",
        "@ and ` cannot start a plain value: put the value in quotes",
        1,
        4,
    ),
    ("key: [a, b]]", "this ] closes no list: remove it", 1, 12),
    (
        "a: 'x'y",
        "this text does not belong here: nothing may follow a quoted value on its line",
        1,
        7,
    ),
    (
        "a: - b",
        "a list item cannot start here: write the list on the next lines, indented under its key",
        1,
        4,
    ),
    (
        "a: \"\\q\"",
        "unknown escape in a string: '\\q' (YAML knows \\n \\t \\r \\0 \\\\ \\\" \\/ and \\x, \\u or \\U with hex digits)",
        1,
        6,
    ),
];

const XML: &[Case] = &[
    (
        "<a>",
        "<a> is never closed: its end tag </a> is missing",
        1,
        1,
    ),
    ("<a", "a tag is never closed: > is missing", 1, 1),
    ("<a/><b/", "a tag is never closed: > is missing", 1, 5),
    (
        "<a b=\"x/>",
        "an attribute value is never closed: \" is missing",
        1,
        1,
    ),
    (
        "<a b='x/>",
        "an attribute value is never closed: ' is missing",
        1,
        1,
    ),
    (
        "<a><!-- x</a>",
        "a comment is never closed: --> is missing",
        1,
        4,
    ),
    (
        "<a><![CDATA[x</a>",
        "a CDATA section is never closed: ]]> is missing",
        1,
        4,
    ),
    (
        "<?xml version=\"1.0\"",
        "the XML declaration is never closed: ?> is missing",
        1,
        1,
    ),
    (
        "<!DOCTYPE",
        "the DOCTYPE is never closed: > is missing",
        1,
        1,
    ),
    (
        "<a></b>",
        "</b> does not match the open <a>: close <a> first, or fix the name",
        1,
        4,
    ),
    ("</a>", "</a> closes an element that was never opened", 1, 1),
    (
        "<a b=1/>",
        "an attribute value must be in quotes, as in name=\"value\"",
        1,
        1,
    ),
    (
        "<a b/>",
        "an attribute needs = and a value in quotes, as in name=\"value\"",
        1,
        1,
    ),
    (
        "<a>&</a>",
        "an & starts a reference that no ; ends: write a plain & as &amp;",
        1,
        4,
    ),
];

fn check(language: Language, cases: &[Case]) {
    let mut failures = Vec::new();
    for (text, message, line, column) in cases {
        match language.behavior().parse(text) {
            Ok(_) => failures.push(format!("{text:?} was read without an error")),
            Err(error) => {
                if (error.message.as_str(), error.line, error.column)
                    != (*message, Some(*line), Some(*column))
                {
                    failures.push(format!(
                        "{text:?}\n  said   {:?} at {:?}:{:?}\n  wanted {message:?} at {line}:{column}",
                        error.message, error.line, error.column
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The parsers' own words, which no message may carry.
const JARGON: &[&str] = &[
    "unexpected token",
    "expected `",
    "expected newline",
    "expected literal",
    "expected nothing",
    "overflowed",
    "invalid expression",
    "ill-formed",
    "syntax error:",
    "YAML parse error",
    "failed to parse",
    "eof",
    "EOF",
];

#[test]
fn hcl_errors_read_plainly() {
    check(Language::HCL, HCL);
}

#[test]
fn toml_errors_read_plainly() {
    check(Language::TOML, TOML);
}

#[test]
fn yaml_errors_read_plainly() {
    check(Language::YAML, YAML);
}

#[test]
fn xml_errors_read_plainly() {
    check(Language::XML, XML);
}

#[test]
fn no_language_tells_its_parsers_words() {
    let all = [
        (Language::HCL, HCL),
        (Language::TOML, TOML),
        (Language::YAML, YAML),
        (Language::XML, XML),
    ];
    for (language, cases) in all {
        for (text, ..) in cases {
            let message = language.behavior().parse(text).unwrap_err().message;
            for word in JARGON {
                assert!(
                    !message.contains(word),
                    "{text:?}: {message:?} says {word:?}"
                );
            }
        }
    }
}
