//! Enter in every language on the same kinds of lines, through what the
//! editor writes (`keys::enter_newline`): after an opener with a trailing
//! comment, on a blank line of a multi-line string, and before a bracket's
//! closer after an entry's comma. Each test lists every failing case. What
//! Enter writes rests on each language's syntax (openers, comments,
//! strings over lines), so the cases are written by hand; after a plain
//! entry every language, custom ones too, is checked on what it writes.

use super::super::keys::enter_newline::newline_at;
use super::Language;
use super::harness::{At, every_language, report, write};
use serde_json::json;

/// Where the caret is in a case.
const CARET: &str = "‸";

/// `source` (the caret at [`CARET`]) after Enter, the caret marked again.
fn entered(language: Language, marked: &str) -> String {
    let at = marked.find(CARET).expect("a caret");
    let source = format!("{}{}", &marked[..at], &marked[at + CARET.len()..]);
    let behavior = language.behavior();
    let bodies = behavior.string_body_lines(&source);
    let in_body = |line: usize| bodies.iter().any(|lines| lines.contains(&line));
    let newline = newline_at(
        behavior,
        &source,
        crate::editor::text::text_size(at),
        &in_body,
    );
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

/// After a start tag written over lines (its attributes on lines of their
/// own): the new line indents from the line the tag starts on, not from
/// its last attribute line, with tabs or spaces, its `>` at
/// the end of an attribute line or on a line of its own, and between the
/// tag and its end tag.
#[test]
fn enter_after_a_tag_over_lines_indents_from_its_first_line() {
    check(&[
        (
            Language::XML,
            "<beans>\n\t<bean id=\"a\"\n\t\tclass=\"b\">‸\n\t\t<p/>\n\t</bean>\n</beans>\n",
            "<beans>\n\t<bean id=\"a\"\n\t\tclass=\"b\">\n\t\t‸\n\t\t<p/>\n\t</bean>\n</beans>\n",
        ),
        (
            Language::XML,
            "<project>\n    <plugin\n        groupId=\"g\"\n        artifactId=\"a\">‸\n    </plugin>\n</project>\n",
            "<project>\n    <plugin\n        groupId=\"g\"\n        artifactId=\"a\">\n        ‸\n    </plugin>\n</project>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a\n    x=\"1\"\n  >‸\n    <b/>\n  </a>\n</r>\n",
            "<r>\n  <a\n    x=\"1\"\n  >\n    ‸\n    <b/>\n  </a>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a x=\"1\"\n     y=\"2\">‸</a>\n</r>\n",
            "<r>\n  <a x=\"1\"\n     y=\"2\">\n    ‸\n  </a>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a x=\"1\"\n     y=\"2\"/>‸\n</r>\n",
            "<r>\n  <a x=\"1\"\n     y=\"2\"/>\n  ‸\n</r>\n",
        ),
    ]);
}

/// After a comment or processing instruction written over lines, the new
/// line indents from the line it starts on, a `<` inside it
/// or not. (After CDATA text over lines the element's text may go on: the
/// line's indentation stays, as inside it.)
#[test]
fn enter_after_a_comment_over_lines_indents_from_its_first_line() {
    check(&[
        (
            Language::XML,
            "<r>\n  <!-- a\n       b -->‸\n  <x/>\n</r>\n",
            "<r>\n  <!-- a\n       b -->\n  ‸\n  <x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n\t<!-- a\n\t     <b> -->‸\n\t<x/>\n</r>\n",
            "<r>\n\t<!-- a\n\t     <b> -->\n\t‸\n\t<x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <?pi a\n        b?>‸\n  <x/>\n</r>\n",
            "<r>\n  <?pi a\n        b?>\n  ‸\n  <x/>\n</r>\n",
        ),
    ]);
}

/// After an element on the line where markup written over lines ends
/// (`b --> <y/>`): the new line indents from the line that markup starts
/// on, not its continuation, with spaces or tabs, after a
/// comment, a processing instruction or a tag, two such in a row.
#[test]
fn enter_after_an_element_ending_a_comment_line_indents_from_the_comment() {
    check(&[
        (
            Language::XML,
            "<r>\n  <!-- a\n       b --> <y/>‸\n  <x/>\n</r>\n",
            "<r>\n  <!-- a\n       b --> <y/>\n  ‸\n  <x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n\t<!-- a\n\t     b --> <y>1</y>‸\n\t<x/>\n</r>\n",
            "<r>\n\t<!-- a\n\t     b --> <y>1</y>\n\t‸\n\t<x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <?pi a\n       b?> <y/>‸\n  <x/>\n</r>\n",
            "<r>\n  <?pi a\n       b?> <y/>\n  ‸\n  <x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <!-- a\n     b --> <!-- c\n         d --> <y/>‸\n  <x/>\n</r>\n",
            "<r>\n  <!-- a\n     b --> <!-- c\n         d --> <y/>\n  ‸\n  <x/>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a x=\"1\"\n     y=\"2\"/> <b/>‸\n</r>\n",
            "<r>\n  <a x=\"1\"\n     y=\"2\"/> <b/>\n  ‸\n</r>\n",
        ),
    ]);
}

/// At the end of a line that closes a construct written over lines (a
/// block comment, a heredoc, a multi-line string or list, an element's
/// text): the new line indents from the line the construct starts on, as
/// after XML markup over lines. A line inside the construct
/// keeps its own indentation.
#[test]
fn enter_after_a_construct_over_lines_indents_from_its_first_line() {
    check(&[
        (
            Language::HCL,
            "a {\n  /* a\n     b */‸\n}\n",
            "a {\n  /* a\n     b */\n  ‸\n}\n",
        ),
        (
            Language::HCL,
            "a {\n  x = 1 /* a\n     b */‸\n}\n",
            "a {\n  x = 1 /* a\n     b */\n  ‸\n}\n",
        ),
        (
            Language::HCL,
            "a {\n  x = <<EOT\nhello\nEOT‸\n}\n",
            "a {\n  x = <<EOT\nhello\nEOT\n  ‸\n}\n",
        ),
        (
            Language::HCL,
            "a {\n  x = <<-EOT\n    hello\n    EOT‸\n}\n",
            "a {\n  x = <<-EOT\n    hello\n    EOT\n  ‸\n}\n",
        ),
        (
            Language::HCL,
            "a {\n  x = [\n    1,\n    2]‸\n}\n",
            "a {\n  x = [\n    1,\n    2]\n  ‸\n}\n",
        ),
        (
            Language::HCL,
            "a {\n\tx = [\n\t\t1,\n\t\t2]‸\n}\n",
            "a {\n\tx = [\n\t\t1,\n\t\t2]\n\t‸\n}\n",
        ),
        (
            Language::TOML,
            "[t]\nx = [\n  1,\n  2]‸\n",
            "[t]\nx = [\n  1,\n  2]\n‸\n",
        ),
        (
            Language::TOML,
            "[t]\nx = [\n  [1,\n   2]]‸\n",
            "[t]\nx = [\n  [1,\n   2]]\n‸\n",
        ),
        (
            Language::TOML,
            "[t]\nx = \"\"\"\n  a\n  \"\"\"‸\n",
            "[t]\nx = \"\"\"\n  a\n  \"\"\"\n‸\n",
        ),
        (
            Language::JSON,
            "{\n  \"a\": [\n    1,\n    2],‸\n  \"b\": 1\n}\n",
            "{\n  \"a\": [\n    1,\n    2],\n  ‸\n  \"b\": 1\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"a\": {\n    \"b\": 1},‸\n  \"c\": 1\n}\n",
            "{\n  \"a\": {\n    \"b\": 1},\n  ‸\n  \"c\": 1\n}\n",
        ),
        (
            Language::YAML,
            "a:\n  l: [1,\n    2]‸\n",
            "a:\n  l: [1,\n    2]\n  ‸\n",
        ),
        (
            Language::XML,
            "<r>\n  <x>text\n   more</x> <y/>‸\n</r>\n",
            "<r>\n  <x>text\n   more</x> <y/>\n  ‸\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <x>text\n   more</x>‸\n</r>\n",
            "<r>\n  <x>text\n   more</x>\n  ‸\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <x>a\n   b\n     c</x>‸\n</r>\n",
            "<r>\n  <x>a\n   b\n     c</x>\n  ‸\n</r>\n",
        ),
        // A YAML quoted scalar closed on a later line: the
        // entry it is the value of sets the indentation.
        (
            Language::YAML,
            "a:\n  b: \"x\n    y\"‸\n  c: 1\n",
            "a:\n  b: \"x\n    y\"\n  ‸\n  c: 1\n",
        ),
        (
            Language::YAML,
            "a:\n  b: 'x\n    y'‸\n",
            "a:\n  b: 'x\n    y'\n  ‸\n",
        ),
        (
            Language::YAML,
            "a:\n  b: 'it''s\n    y' # c‸\n",
            "a:\n  b: 'it''s\n    y' # c\n  ‸\n",
        ),
        (
            Language::YAML,
            "a:\n  - \"x\n    y\"‸\n",
            "a:\n  - \"x\n    y\"\n  ‸\n",
        ),
        // Inside the construct the line keeps its indentation.
        (
            Language::YAML,
            "a:\n  b: \"x\n    y‸\"\n",
            "a:\n  b: \"x\n    y\n    ‸\"\n",
        ),
        (
            Language::YAML,
            "a:\n  b: |\n    x\n    y‸\n",
            "a:\n  b: |\n    x\n    y\n    ‸\n",
        ),
        (
            Language::HCL,
            "x = [\n    1,‸\n    2]\n",
            "x = [\n    1,\n    ‸\n    2]\n",
        ),
        (
            Language::JSON,
            "[\n    1,‸\n    2]\n",
            "[\n    1,\n    ‸\n    2]\n",
        ),
        (
            Language::HCL,
            "a {\n  /* a\n     b‸\n  */\n}\n",
            "a {\n  /* a\n     b\n     ‸\n  */\n}\n",
        ),
        (
            Language::XML,
            "<r>\n  <x>text\n   more‸\n  </x>\n</r>\n",
            "<r>\n  <x>text\n   more\n   ‸\n  </x>\n</r>\n",
        ),
        // Closed on its own line: its own indentation.
        (
            Language::HCL,
            "a {\n  x = [1, 2]‸\n}\n",
            "a {\n  x = [1, 2]\n  ‸\n}\n",
        ),
    ]);
}

/// Finding where a construct closed by the caret's line starts reads the
/// text's constructs a fixed number of times per Enter, at any size.
#[test]
fn enter_after_a_construct_reads_the_constructs_a_fixed_number_of_times() {
    use super::super::work_count::{Work, counted};
    let scans = |items: usize| {
        let list: Vec<String> = (0..items).map(|item| format!("    {item}")).collect();
        let marked = format!("{{\n  \"a\": [\n{}],‸\n  \"b\": 1\n}}\n", list.join(",\n"));
        let (found, scans) = counted(Work::ConstructScan, || entered(Language::JSON, &marked));
        assert!(found.contains("],\n  ‸\n"), "{found}");
        scans
    };
    let (small, large) = (scans(10), scans(1000));
    assert_eq!(small, large);
    assert!(small <= 4, "{small} scans");
}

/// Enter in the middle of a line, right after a complete value, before
/// what is a key or an item of its own (`port: 1‸host: x`): the rest goes
/// on at the line's indentation (its item's content column in an item),
/// not as a continued value. After a key with no value, or
/// before more of a plain value, it goes deeper as before.
#[test]
fn enter_between_a_value_and_a_key_keeps_the_lines_indent() {
    check(&[
        (Language::YAML, "port: 1‸host: x\n", "port: 1\n‸host: x\n"),
        (
            Language::YAML,
            "server:\n  port: 1‸listen: 7835\n  host: x\n",
            "server:\n  port: 1\n  ‸listen: 7835\n  host: x\n",
        ),
        (
            Language::YAML,
            "server:\n  port: 1 ‸listen: 7835\n",
            "server:\n  port: 1 \n  ‸listen: 7835\n",
        ),
        (
            Language::YAML,
            "a:\n  b:\n    port: \"1\"‸c: 2\n",
            "a:\n  b:\n    port: \"1\"\n    ‸c: 2\n",
        ),
        (
            Language::YAML,
            "items:\n  - a: 1‸b: 2\n",
            "items:\n  - a: 1\n    ‸b: 2\n",
        ),
        (
            Language::YAML,
            "items:\n  - a‸- b\n",
            "items:\n  - a\n  ‸- b\n",
        ),
        (
            Language::YAML,
            "items:\n  - a: 1‸- b\n",
            "items:\n  - a: 1\n  ‸- b\n",
        ),
        (Language::YAML, "- a‸- b\n", "- a\n‸- b\n"),
        // An item of a list nested in an item: at its own dash.
        (
            Language::YAML,
            "a:\n  - - 1‸- 2\n",
            "a:\n  - - 1\n    ‸- 2\n",
        ),
        (
            Language::YAML,
            "a:\n  - - - x‸- y\n",
            "a:\n  - - - x\n      ‸- y\n",
        ),
        (
            Language::YAML,
            "a:\n  - - k: 1‸- 2\n",
            "a:\n  - - k: 1\n    ‸- 2\n",
        ),
        // After a key with no value: its children.
        (
            Language::YAML,
            "port:‸listen: 7835\n",
            "port:\n  ‸listen: 7835\n",
        ),
        // Inside a plain value: its continuation.
        (Language::YAML, "a: hello ‸world\n", "a: hello \n  ‸world\n"),
        // Other languages keep the line's indentation already.
        (
            Language::TOML,
            "[t]\n  port = 1‸host = \"x\"\n",
            "[t]\n  port = 1\n  ‸host = \"x\"\n",
        ),
        (
            Language::HCL,
            "a {\n  port = 1‸host = \"x\"\n}\n",
            "a {\n  port = 1\n  ‸host = \"x\"\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"port\": 1,‸\"host\": \"x\"\n}\n",
            "{\n  \"port\": 1,\n  ‸\"host\": \"x\"\n}\n",
        ),
        (
            Language::XML,
            "<r>\n  <port>1</port>‸<host>x</host>\n</r>\n",
            "<r>\n  <port>1</port>\n  ‸<host>x</host>\n</r>\n",
        ),
    ]);
}

/// Enter at the end of an entry's line opens a line at the entry's
/// indentation, the next entry of its object: on the documents every
/// language writes itself, custom ones too, at each depth.
#[test]
fn enter_after_an_entry_keeps_its_indentation_in_every_language() {
    let document = json!({"a": {"b": 1, "c": {"d": "x", "e": 2}}, "f": 3});
    let mut failures = Vec::new();
    for language in every_language() {
        for pointer in ["/a/b", "/a/c/d", "/f"] {
            let at = At::LineOfKey { pointer, offset: 0 };
            let written = match write(language, &document, &[], at) {
                Ok(written) => written,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
            let text = &written.text;
            let line_end = text[written.byte..]
                .find('\n')
                .map_or(text.len(), |at| written.byte + at);
            let indent: String = text[written.byte..line_end]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            let marked = format!("{}{CARET}{}", &text[..line_end], &text[line_end..]);
            let expected = format!(
                "{}\n{indent}{CARET}{}",
                &text[..line_end],
                &text[line_end..]
            );
            let found = entered(language, &marked);
            if found != expected {
                failures.push(format!(
                    "{language} {pointer}: {found:?}, expected {expected:?}"
                ));
            }
        }
    }
    report(&failures);
}
