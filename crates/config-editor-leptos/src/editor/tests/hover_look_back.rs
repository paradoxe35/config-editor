//! The docs under the pointer over a long comment or value cost the same
//! however many words come before it on the line: the nearest key before
//! it is looked up only where the line writes one (round 9's `D9.perf-1`:
//! every word looked up its own key through the whole document, seconds
//! per pointer move over a 4,000-character comment).

use std::sync::Arc;

use crate::editor::diagnostics::build_schema;
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted};

const SCHEMA: &str =
    r#"{"type": "object", "properties": {"note": {"type": "string", "description": "A note."}}}"#;

/// Docs looked up for the docs under the last word of a line `words` words
/// long, below 300 other lines.
fn reads(language: Language, words: usize) -> usize {
    let filler: String = (0..300)
        .map(|index| {
            format!(
                "k{index}{} 1\n",
                if language == Language::YAML {
                    ":"
                } else {
                    " ="
                }
            )
        })
        .collect();
    let line = match language.label() {
        "YAML" => format!("note: x # {}\n", vec!["word"; words].join(" ")),
        _ => format!("note = \"x\" # {}\n", vec!["word"; words].join(" ")),
    };
    let text = format!("{filler}{line}");
    let mut editor = TestEditor::new(language, &text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema")));
    let (_owner, state) = editor.mount();
    let column = line.trim_end().chars().count() - 2;
    let (found, contexts) = counted(Work::HoverLookup, || state.docs_under_pointer(300, column));
    assert!(
        found.is_some_and(|docs| docs.key == "note"),
        "{language}: the field's docs"
    );
    contexts
}

#[test]
fn docs_under_a_long_line_look_up_one_key() {
    let mut failures = Vec::new();
    for language in [Language::YAML, Language::TOML] {
        let (few, many) = (reads(language, 20), reads(language, 300));
        if many > few {
            failures.push(format!(
                "{language}: {few} docs looked up at 20 words, {many} at 300"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
