//! P1: a locked field whose scalar is written on a line below its key
//! (`"port":` then `8080`, `port:` then `8080` in YAML, an HCL expression
//! in parentheses over lines) is locked over that line too: typing into
//! the value is refused, in every language that can write it so.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::test_state::TestEditor;

/// Documents locking `/port`, each with the value's text (its last copy in
/// the document is the one typed into) and what the lock covers.
const CASES: &[(Language, &str, &str, &str)] = &[
    (
        Language::JSON,
        "{\n  \"port\":\n    8080,\n  \"a\": 1\n}\n",
        "8080,",
        "\"port\":\n    8080",
    ),
    (
        Language::JSON,
        "{\n  \"port\":\n  \"line\\nnext\",\n  \"a\": 1\n}\n",
        "\"line",
        "\"port\":\n  \"line\\nnext\"",
    ),
    (
        Language::YAML,
        "port:\n  8080\na: 1\n",
        "8080\n",
        "port:\n  8080",
    ),
    (
        Language::YAML,
        "port:\n  \"q\"\na: 1\n",
        "\"q\"",
        "port:\n  \"q\"",
    ),
    (
        Language::YAML,
        "port: &p\n  8080\na: 1\n",
        "8080\n",
        "port: &p\n  8080",
    ),
    (
        Language::YAML,
        "? port\n: 8080\na: 1\n",
        "8080\n",
        "port\n: 8080",
    ),
    (
        Language::HCL,
        "port = (\n  8080\n)\na = 1\n",
        "8080\n",
        "port = (\n  8080\n)",
    ),
];

#[test]
fn a_scalar_written_below_its_key_is_locked() {
    for &(language, text, value, covered) in CASES {
        let mut editor = TestEditor::new(language, text);
        editor.read_only_paths = vec![FieldPath::from("/port")];
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
        assert_eq!(locked, [covered], "{language:?} {text:?}");
        let at = text.rfind(value).expect("the value is written");
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
        assert!(!taken, "{language:?}: typing into the value is refused");
        assert_eq!(state.doc().text.get_untracked(), text, "{language:?}");
    }
}

/// A scalar on its key's line keeps the lock it had: the line below,
/// another field's, stays free.
#[test]
fn a_scalar_on_its_keys_line_locks_that_line_only() {
    for (language, text, covered) in [
        (
            Language::JSON,
            "{\n  \"port\": 8080,\n  \"a\": 1\n}\n",
            "\"port\": 8080",
        ),
        (Language::YAML, "port: 8080\na: 1\n", "port: 8080"),
        (Language::TOML, "port = 8080\na = 1\n", "port = 8080"),
        (Language::HCL, "port = 8080\na = 1\n", "port = 8080"),
    ] {
        let mut editor = TestEditor::new(language, text);
        editor.read_only_paths = vec![FieldPath::from("/port")];
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
        assert_eq!(locked, [covered], "{language:?}");
    }
}
