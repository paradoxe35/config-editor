//! Syntax colours of keys and literal values in every language and form:
//! a key is coloured as a key however it is written (quoted, with spaces,
//! dotted, non-ASCII, YAML's merge and complex keys, TOML table headers,
//! HCL object keys, XML names with namespaces), and every number form a
//! language reads as a number (or a boolean, null, date) is coloured as a
//! literal.

use super::super::highlight::{Token, TokenKind};
use super::Language;

/// The kind of the token covering byte `at` of the line, if any.
fn kind_at(tokens: &[Token], at: usize) -> Option<TokenKind> {
    tokens
        .iter()
        .find(|token| token.start <= at && at < token.end)
        .map(|token| token.kind)
}

/// Checks that `part` (found in `line` after byte `from`) is coloured
/// `kind` on every character but the separators in `skip`.
fn check(
    failures: &mut Vec<String>,
    language: Language,
    line: &str,
    part: &str,
    kind: TokenKind,
    skip: &[char],
) {
    let tokens = language.behavior().highlight(line);
    let Some(start) = line.find(part) else {
        failures.push(format!("{language} {line:?}: no {part:?} in it"));
        return;
    };
    let wrong: Vec<String> = part
        .char_indices()
        .filter(|(_, character)| !skip.contains(character))
        .filter_map(|(offset, character)| {
            let found = kind_at(&tokens, start + offset);
            (found != Some(kind)).then(|| format!("{character:?} is {found:?}"))
        })
        .collect();
    if !wrong.is_empty() {
        failures.push(format!(
            "{language} {line:?}: {part:?} should be {kind:?}: {}",
            wrong.join(", ")
        ));
    }
}

/// (language, line, the key in it).
fn keys() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (Language::YAML, "plain_key: 1", "plain_key"),
        (Language::YAML, "\"quoted key\": 1", "\"quoted key\""),
        (Language::YAML, "'single key': two", "'single key'"),
        (Language::YAML, "key with spaces: value", "key with spaces"),
        (Language::YAML, "server.port: 80", "server.port"),
        (Language::YAML, "Content-Type: text/plain", "Content-Type"),
        (Language::YAML, "名前: 太郎", "名前"),
        (Language::YAML, "  <<: *base", "<<"),
        (Language::YAML, "- \"a\": 1", "\"a\""),
        (Language::YAML, "  - - inner: 1", "inner"),
        (Language::YAML, "- ? complex", "complex"),
        (Language::YAML, "? complex key", "complex key"),
        (Language::YAML, "\"a: b\": 1", "\"a: b\""),
        (Language::YAML, "dq: 'not # a comment'", "dq"),
        (Language::YAML, "m: {\"q k\": 1, b: 2}", "\"q k\""),
        (Language::YAML, "m: {\"q k\": 1, b c: 2}", "b c"),
        (Language::JSON, "  \"plain\": 1,", "\"plain\""),
        (
            Language::JSON,
            "  \"key with spaces\": 1,",
            "\"key with spaces\"",
        ),
        (Language::JSON, "  \"a.b\": true,", "\"a.b\""),
        (Language::JSON, "  \"名前\": \"x\",", "\"名前\""),
        (Language::JSON, "  \"esc\\\"q\": 1", "\"esc\\\"q\""),
        (
            Language::JSON,
            "  {\"inline\": 1, \"other\": 2}",
            "\"other\"",
        ),
        (Language::TOML, "bare_key = 1", "bare_key"),
        (Language::TOML, "bare-key = 1", "bare-key"),
        (Language::TOML, "1234 = \"x\"", "1234"),
        (Language::TOML, "\"quoted key\" = 1", "\"quoted key\""),
        (Language::TOML, "'literal key' = 1", "'literal key'"),
        (Language::TOML, "名前 = 1", "名前"),
        (Language::TOML, "\"名前\" = 1", "\"名前\""),
        (
            Language::TOML,
            "tags = { name = \"x\", \"q k\" = 1 }",
            "\"q k\"",
        ),
        (Language::TOML, "tags = { name = \"x\" }", "name"),
        (Language::TOML, "points = [{ x = 1 }]", "x"),
        (Language::HCL, "name = \"web\"", "name"),
        (Language::HCL, "with-dash = 1", "with-dash"),
        (Language::HCL, "名前 = 1", "名前"),
        (
            Language::HCL,
            "tags = { Name = \"web\", Role = \"x\" }",
            "Role",
        ),
        (
            Language::HCL,
            "tags = { \"kebab-key\" = 1 }",
            "\"kebab-key\"",
        ),
        (Language::HCL, "tags = { Name: \"web\" }", "Name"),
        (Language::HCL, "  \"quoted\" = 1", "\"quoted\""),
        (Language::XML, "<server-name>x</server-name>", "server-name"),
        (Language::XML, "<ns:tag>x</ns:tag>", "ns:tag"),
        (Language::XML, "<名前>x</名前>", "名前"),
        (Language::XML, "<a.b>x</a.b>", "a.b"),
        (
            Language::XML,
            "<p data-x=\"1\" xml:lang=\"en\"/>",
            "xml:lang",
        ),
    ]
}

/// The separators inside a key coloured as punctuation (TOML's and
/// YAML's dots are part of the key; TOML dotted keys and table headers
/// are split at their dots).
fn dotted() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (Language::TOML, "a.b.c = 1", "a.b.c"),
        (Language::TOML, "a . b = 1", "a . b"),
        (
            Language::TOML,
            "site.\"google.com\" = true",
            "site.\"google.com\"",
        ),
        (Language::TOML, "[server]", "server"),
        (Language::TOML, "[server.http]", "server.http"),
        (Language::TOML, "[[products]]", "products"),
        (
            Language::TOML,
            "[ \"quoted table\" . x ]",
            "\"quoted table\" . x",
        ),
        (Language::TOML, "[[fruits.variety]] # c", "fruits.variety"),
    ]
}

/// (language, line, the literal in it).
fn literals() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (Language::YAML, "n: 42", "42"),
        (Language::YAML, "hex: 0x1F", "0x1F"),
        (Language::YAML, "oct: 0o17", "0o17"),
        (Language::YAML, "f: -1.5e3", "-1.5e3"),
        (Language::YAML, "inf: .inf", ".inf"),
        (Language::YAML, "ninf: -.Inf", "-.Inf"),
        (Language::YAML, "nan: .NaN", ".NaN"),
        (Language::YAML, "t: True", "True"),
        (Language::YAML, "n: NULL", "NULL"),
        (Language::YAML, "- 0x1F", "0x1F"),
        (Language::TOML, "n = 42", "42"),
        (Language::TOML, "big = 1_000_000", "1_000_000"),
        (Language::TOML, "hex = 0xDEAD_BEEF", "0xDEAD_BEEF"),
        (Language::TOML, "oct = 0o755", "0o755"),
        (Language::TOML, "bin = 0b1101", "0b1101"),
        (Language::TOML, "plus = +99", "+99"),
        (Language::TOML, "f = 6.626e-34", "6.626e-34"),
        (Language::TOML, "f = 224_617.445_991", "224_617.445_991"),
        (Language::TOML, "i = -inf", "-inf"),
        (Language::TOML, "n = nan", "nan"),
        (
            Language::TOML,
            "when = 1979-05-27T07:32:00Z",
            "1979-05-27T07:32:00Z",
        ),
        (Language::TOML, "day = 1979-05-27", "1979-05-27"),
        (Language::TOML, "at = 07:32:00", "07:32:00"),
        (Language::TOML, "a = [0x1F, 1_000]", "1_000"),
        (Language::JSON, "  \"n\": -1.5e3,", "-1.5e3"),
        (Language::HCL, "n = 1e3", "1e3"),
        (Language::HCL, "b = true", "true"),
    ]
}

/// Plain strings that only look like literals: kept as values.
fn not_literals() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (Language::YAML, "s: inf", "inf"),
        (Language::YAML, "s: 0x1G", "0x1G"),
        (Language::YAML, "s: 1.2.3", "1.2.3"),
        (Language::TOML, "s = 1__0", "1__0"),
    ]
}

/// (language, line, the punctuation in it): YAML's directives, document
/// markers and complex key indicators.
fn punctuation() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (Language::YAML, "%YAML 1.2", "%YAML"),
        (Language::YAML, "%TAG ! tag:example.com,2000:", "%TAG"),
        (Language::YAML, "---", "---"),
        (Language::YAML, "--- # first", "---"),
        (Language::YAML, "...", "..."),
        (Language::YAML, "? complex", "?"),
        (Language::YAML, ": value", ":"),
    ]
}

/// QA 21: YAML coloured a key only when it was `[A-Za-z0-9_-]`, numbers
/// only when Rust parsed them as floats, TOML table headers as one run of
/// punctuation and HCL object keys not at all.
#[test]
fn keys_and_literals_are_coloured_in_every_form() {
    let mut failures = Vec::new();
    for (language, line, key) in keys() {
        check(&mut failures, language, line, key, TokenKind::Key, &[]);
    }
    for (language, line, key) in dotted() {
        check(
            &mut failures,
            language,
            line,
            key,
            TokenKind::Key,
            &['.', ' '],
        );
        let tokens = language.behavior().highlight(line);
        for (at, _) in line.match_indices(['[', ']']) {
            if kind_at(&tokens, at) != Some(TokenKind::Punctuation) {
                failures.push(format!(
                    "{language} {line:?}: bracket at {at} not punctuation"
                ));
            }
        }
    }
    for (language, line, literal) in literals() {
        check(
            &mut failures,
            language,
            line,
            literal,
            TokenKind::Literal,
            &[],
        );
    }
    for (language, line, value) in not_literals() {
        check(&mut failures, language, line, value, TokenKind::Value, &[]);
    }
    for (language, line, mark) in punctuation() {
        check(
            &mut failures,
            language,
            line,
            mark,
            TokenKind::Punctuation,
            &[],
        );
    }
    // After a YAML directive or marker, the rest keeps its colours.
    check(
        &mut failures,
        Language::YAML,
        "--- # first",
        "# first",
        TokenKind::Comment,
        &[],
    );
    check(
        &mut failures,
        Language::YAML,
        ": value",
        "value",
        TokenKind::Value,
        &[],
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
