//! Backspace asks for the suggestions again only when the list showed
//! options since it was asked for: taking back what the filter emptied it
//! with brings them back (`D13.a11y-1`), while a list that showed none has
//! none to bring back, and working it out again cost each Backspace in a
//! large HCL or XML text a read of the whole text (QA `L30-P2`).

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::input::apply_input;
use crate::editor::keyboard::utf16_to_byte_index;
use crate::editor::languages::Language;
use crate::editor::save_matrix::ALL;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted};

/// The sample in `language`.
fn sample(language: Language) -> &'static str {
    [
        (
            Language::YAML,
            include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
        ),
    ]
    .into_iter()
    .find(|(known, _)| *known == language)
    .map(|(_, text)| text)
    .unwrap()
}

/// `text` with its middle `udp` value (a protocol, which the schema names
/// `tcp` or `udp`) written `value`, and the UTF-16 offset of its end.
fn with_protocol(text: &str, value: &str) -> (String, usize) {
    let ends: Vec<usize> = text
        .match_indices("udp")
        .map(|(at, _)| at + "udp".len())
        .filter(|&end| {
            let after = &text[end..];
            ["\"\n", "\",", "\"\r", "<", "\n"]
                .iter()
                .any(|next| after.starts_with(next))
        })
        .collect();
    let end = ends[ends.len() / 2];
    let start = end - "udp".len();
    let written = format!("{}{value}{}", &text[..start], &text[end..]);
    let caret = written[..start + value.len()].encode_utf16().count();
    (written, caret)
}

fn schema() -> std::sync::Arc<crate::editor::diagnostics::SchemaBundle> {
    std::sync::Arc::new(
        crate::editor::diagnostics::build_schema(include_str!(
            "../../../tests/fixtures/tunnel.schema.json"
        ))
        .unwrap(),
    )
}

/// Types `typed` at UTF-16 `at`, or deletes the character before it
/// (`None`), as the browser's input does; the list then reads its
/// options, as the shown popup does.
fn input(state: EditorState, at: usize, typed: Option<char>) {
    let before = state.doc().text.get_untracked();
    let byte = utf16_to_byte_index(&before, at);
    let mut text = before.clone();
    let (kind, caret) = match typed {
        Some(character) => {
            text.insert(byte, character);
            ("insertText", at + 1)
        }
        None => {
            text.remove(byte - 1);
            ("deleteContentBackward", at - 1)
        }
    };
    let edit = Edit::input(
        before.clone(),
        (at, at),
        text,
        (caret, caret),
        kind.into(),
        0.0,
    );
    apply_input(state, edit, &before, kind);
    let _ = state.completion().suggestions.get_untracked();
}

#[test]
fn backspace_reads_no_list_that_showed_nothing() {
    let schema = schema();
    crate::editor::parse_cache::hold();
    for language in ALL {
        let label = language.label();
        let (text, at) = with_protocol(sample(language), "udp");
        let (_owner, state) = TestEditor {
            schema: Some(schema.clone()),
            ..TestEditor::new(language, &text)
        }
        .mount();
        // `udpq` matches no option: the list asked for shows none.
        input(state, at, Some('q'));
        assert!(!state.suggestions_showing(), "{label}: udpq");
        let ((), read) = counted(Work::CompletionContext, || input(state, at + 1, None));
        assert_eq!(read, 0, "{label}: the caret's place read for a list");
        assert!(!state.completion().requested.get_untracked(), "{label}");
    }
    crate::editor::parse_cache::release();
}

#[test]
fn backspace_brings_back_the_options_the_filter_emptied() {
    let schema = schema();
    crate::editor::parse_cache::hold();
    for language in ALL {
        let label = language.label();
        let (text, at) = with_protocol(sample(language), "u");
        let (_owner, state) = TestEditor {
            schema: Some(schema.clone()),
            ..TestEditor::new(language, &text)
        }
        .mount();
        input(state, at, Some('d'));
        assert!(state.suggestions_showing(), "{label}: ud");
        input(state, at + 1, Some('q'));
        input(state, at + 2, Some('q'));
        assert!(!state.suggestions_showing(), "{label}: udqq");
        input(state, at + 3, None);
        input(state, at + 2, None);
        assert!(state.suggestions_showing(), "{label}: back to ud");
    }
    crate::editor::parse_cache::release();
}
