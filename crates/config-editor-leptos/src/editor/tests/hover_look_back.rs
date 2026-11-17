//! The docs under the pointer over a long line cost the same however many
//! words the line has: the field is found from where the parsed document
//! places its value, not by looking back over the words for a key (each
//! look read the whole document: seconds per pointer move over a
//! 4,000-character comment). A comment is no field's value: it shows none.

use std::sync::Arc;

use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted};

const SCHEMA: &str =
    r#"{"type": "object", "properties": {"note": {"type": "string", "description": "A note."}}}"#;

/// The key of the docs under `column` of a line `words` comment words
/// long below 300 other lines (`None` for the comment's last word), and
/// the docs looked up for them.
fn reads(language: Language, words: usize, on_value: bool) -> (Option<String>, usize) {
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
    let column = if on_value {
        line.find('x').expect("the value")
    } else {
        line.trim_end().chars().count() - 2
    };
    let (found, lookups) = counted(Work::HoverLookup, || state.docs_under_pointer(300, column));
    (found.map(|docs| docs.key), lookups)
}

#[test]
fn docs_under_a_long_line_look_up_one_key() {
    let mut failures = Vec::new();
    for language in [Language::YAML, Language::TOML] {
        for on_value in [true, false] {
            let wanted = on_value.then(|| "note".to_string());
            let (few_key, few) = reads(language, 20, on_value);
            let (many_key, many) = reads(language, 300, on_value);
            if few_key != wanted || many_key != wanted {
                failures.push(format!(
                    "{language} on the value {on_value}: {few_key:?}, {many_key:?}"
                ));
            }
            if many > few || few > 1 {
                failures.push(format!(
                    "{language}: {few} docs looked up at 20 words, {many} at 300"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
