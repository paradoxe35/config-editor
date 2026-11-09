//! `path_at` anywhere on the lines of a string written over several lines
//! past its key's (a YAML block, quoted or plain scalar, a TOML `"""` or
//! `'''` string, an HCL heredoc, XML text; a map value and a list item)
//! names that string's field, not the map holding it, its closing line
//! included.

use super::super::Language;
use super::TextPosition;

/// (language, text, the string's lines past its key's, its field)
const CASES: [(Language, &str, &str, &str); 10] = [
    (
        Language::YAML,
        "a:\n  b: |\n    one\n    two\n  c: 1\n",
        "    one\n    two",
        "/a/b",
    ),
    (
        Language::YAML,
        "a:\n  b: >-\n    one\n\n    two\n  c: 1\n",
        "    one\n\n    two",
        "/a/b",
    ),
    (
        Language::YAML,
        "a:\n  b: \"one\n    two\"\n  c: 1\n",
        "    two\"",
        "/a/b",
    ),
    (
        Language::YAML,
        "a:\n  b: zero\n    one\n  c: 1\n",
        "    one",
        "/a/b",
    ),
    (
        Language::YAML,
        "l:\n  - |\n    one\n    two\n  - x\n",
        "    one\n    two",
        "/l/0",
    ),
    (
        Language::TOML,
        "[a]\nb = \"\"\"\none\ntwo\"\"\"\nc = 1\n",
        "one\ntwo\"\"\"",
        "/a/b",
    ),
    (
        Language::TOML,
        "[a]\nb = '''\none\ntwo\n'''\nc = 1\n",
        "one\ntwo\n'''",
        "/a/b",
    ),
    (
        Language::HCL,
        "a {\n  b = <<EOT\none\ntwo\nEOT\n  c = 1\n}\n",
        "one\ntwo\nEOT",
        "/a/b",
    ),
    (
        Language::HCL,
        "a {\n  b = <<-EOT\n    one\n    two\n    EOT\n  c = 1\n}\n",
        "    one\n    two\n    EOT",
        "/a/b",
    ),
    (
        Language::XML,
        "<a>\n  <b>one\ntwo</b>\n  <c>1</c>\n</a>\n",
        "two</b>",
        "/a/b",
    ),
];

#[test]
fn path_at_inside_a_string_over_lines_names_its_field() {
    let mut failures = Vec::new();
    for (language, text, lines, wanted) in CASES {
        let start = text.find(&format!("\n{lines}\n")).unwrap() + 1;
        for offset in start..=start + lines.len() {
            let at = TextPosition::from_offset(text, offset);
            let found = language.path_at(text, at).map(|path| path.to_pointer());
            if found.as_deref() != Some(wanted) {
                failures.push(format!(
                    "{language} {text:?} at {}:{}: {found:?}",
                    at.line, at.column
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
