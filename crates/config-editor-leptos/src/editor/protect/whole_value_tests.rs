//! P1: a locked value written over lines is locked whole, as its language's
//! parse writes it (`Text::value_ranges`): from its key through its last
//! closer (a string's closing quote, a block scalar's last line, a list's
//! `]`, a block's `}`, a heredoc's terminator, an end tag), and typing on
//! any of its lines is refused, in every built-in language.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::test_state::TestEditor;

/// Documents locking `/v`, what its lock covers, and a place on one of the
/// value's lines past the first (the text before it) where typing is
/// refused.
const CASES: &[(Language, &str, &str, &str)] = &[
    (
        Language::YAML,
        "v: |\n  a\n  b\nn: 1\n",
        "v: |\n  a\n  b",
        "v: |\n  a\n  ",
    ),
    (
        Language::YAML,
        "v: [1,\n  2]\nn: 1\n",
        "v: [1,\n  2]",
        "v: [1,\n  ",
    ),
    (Language::YAML, "v: a\n  b\nn: 1\n", "v: a\n  b", "v: a\n  "),
    (
        Language::YAML,
        "v: \"a\n  b\"\nn: 1\n",
        "v: \"a\n  b\"",
        "v: \"a\n  ",
    ),
    (
        Language::JSON,
        "{\n  \"v\": [\n    1,\n    2\n  ],\n  \"n\": 1\n}\n",
        "\"v\": [\n    1,\n    2\n  ]",
        "{\n  \"v\": [\n    1,\n    ",
    ),
    (
        Language::JSON,
        "{\n  \"v\": {\n    \"a\": 1\n  },\n  \"n\": 1\n}\n",
        "\"v\": {\n    \"a\": 1\n  }",
        "{\n  \"v\": {\n    \"a\": ",
    ),
    (
        Language::TOML,
        "v = \"\"\"\na\nb\"\"\"\nn = 1\n",
        "v = \"\"\"\na\nb\"\"\"",
        "v = \"\"\"\na\n",
    ),
    (
        Language::TOML,
        "v = [\n  1,\n  2,\n]\nn = 1\n",
        "v = [\n  1,\n  2,\n]",
        "v = [\n  1,\n  ",
    ),
    (
        Language::TOML,
        "n = 1\n\n[v]\na = 1\nb = 2\n\n[m]\nc = 3\n",
        "[v]\na = 1\nb = 2",
        "n = 1\n\n[v]\na = 1\nb = ",
    ),
    (
        Language::HCL,
        "v = <<EOT\na\nb\nEOT\nn = 1\n",
        "v = <<EOT\na\nb\nEOT",
        "v = <<EOT\na\n",
    ),
    (
        Language::HCL,
        "v {\n  a = 1\n}\nn = 1\n",
        "v {\n  a = 1\n}",
        "v {\n  a = ",
    ),
    (
        Language::XML,
        "<v>\n  <a>1</a>\n</v>\n<n>1</n>\n",
        "<v>\n  <a>1</a>\n</v>",
        "<v>\n  <a>",
    ),
];

#[test]
fn a_value_written_over_lines_is_locked_whole() {
    let mut failures = Vec::new();
    for &(language, text, covered, before) in CASES {
        let mut editor = TestEditor::new(language, text);
        editor.read_only_paths = vec![FieldPath::from("/v")];
        let (_owner, state) = editor.mount();
        let units: Vec<u16> = text.encode_utf16().collect();
        let locked: Vec<String> = state.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .flat_map(|lock| lock.ranges.iter())
                .map(|range| String::from_utf16_lossy(&units[range.clone()]))
                .collect()
        });
        if locked != [covered] {
            failures.push(format!(
                "{language} {text:?}: locked {locked:?}, read {:?}",
                language.parse(text)
            ));
        }
        let at = before.len();
        let mut typed = text.to_string();
        typed.insert(at, '9');
        let taken = state.commit(Edit::input(
            text.to_string(),
            (at, at),
            typed,
            (at + 1, at + 1),
            "insertText".into(),
            0.0,
        ));
        if taken || state.doc().text.get_untracked() != text {
            failures.push(format!("{language} {text:?}: typing at {at} taken"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
