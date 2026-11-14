//! A schema text that is empty, or only white space (a byte order mark
//! too), says so, in the `emptySchema` text, not with a JSON reader's
//! words ("EOF while parsing a value"): thrown by `Schema::compile` and
//! shown inline (QA 33, FW33-2).

use crate::EditorStrings;
use crate::editor::Language;
use crate::editor::diagnostics::{Schema, build_schema, validate_with};

const EMPTY: [&str; 6] = ["", " ", "\n", " \t\r\n ", "\u{feff}", "\u{feff}\n  "];

#[test]
fn an_empty_schema_text_says_it_is_empty() {
    let expected = EditorStrings::default().empty_schema;
    assert_eq!(expected, "Invalid JSON Schema: the schema text is empty");
    let mut failures = Vec::new();
    for text in EMPTY {
        let message = Schema::compile(text).expect_err("refused").0;
        if message != expected {
            failures.push(format!("{text:?}: {message}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What the editor shows for a schema that did not compile, in `strings`.
fn shown(error: &str, strings: &EditorStrings) -> Vec<String> {
    validate_with(
        "a: 1\n",
        None,
        Some(error),
        true,
        Language::YAML.behavior(),
        &[],
        strings,
    )
    .into_iter()
    .map(|problem| problem.message)
    .collect()
}

#[test]
fn an_empty_schema_text_is_worded_by_the_strings_given() {
    let strings = EditorStrings {
        empty_schema: "kein Schema".to_string(),
        invalid_schema: "Schema kaputt: {error}".to_string(),
        ..EditorStrings::default()
    };
    for text in EMPTY {
        let error = build_schema(text).err().expect("refused");
        assert_eq!(shown(&error, &strings), ["kein Schema"], "{text:?}");
    }
    // A text that is there but no schema keeps the `invalidSchema` text.
    let error = build_schema("{").err().expect("refused");
    let messages = shown(&error, &strings);
    assert!(messages[0].starts_with("Schema kaputt: "), "{messages:?}");
}
