//! A text holding each key a few times over (the large sample pasted
//! again and again: it does not parse) checks the host's placeholders in
//! work linear in the text, in every language: each placeholder is judged
//! in the copy of its field it is written in, and the copies of all of
//! them are found in a few readings of the text, never a few for each
//! (QA 34: on the large YAML sample pasted four times, Ctrl+A Tab read the
//! text thousands of times, and its undo hung the page).

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};
use super::glued_tests::paste;

/// The demo's large sample in `language` (about 2,000 lines, a
/// placeholder in each client).
fn sample(language: Language) -> String {
    let text = match language.label() {
        "YAML" => include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
        "JSON" => include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
        "TOML" => include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
        "HCL" => include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
        _ => include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
    };
    if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// The whole text's units.
fn units(state: EditorState) -> usize {
    state
        .doc()
        .text
        .with_untracked(|text| text.encode_utf16().count())
}

/// Undoes the last edit, if any.
fn undo(state: EditorState) {
    if let Some(undo) = state.history_edit(false) {
        state.commit(undo);
    }
}

/// Each kind of work (by [`Work::ALL`]) that the sample written `copies`
/// times costs: Ctrl+A Tab and its undo, the sample pasted once more at
/// the end and that undone, and an Edit-as switch.
fn work(language: Language, copies: usize) -> Vec<usize> {
    let one = sample(language);
    let mut editor = TestEditor::new(language, &one.repeat(copies));
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»"));
    let (_owner, state) = editor.mount();
    assert!(
        state.protect().secrets.tokens.with_untracked(Vec::len) > 100,
        "{language}: the placeholders are real"
    );
    let ((), counts) = counted_all(|| {
        run(
            state,
            Command::Indent { outdent: false },
            (0, units(state)),
            false,
        );
        undo(state);
        let end = state.doc().text.with_untracked(String::len);
        paste(state, end..end, &one);
        undo(state);
        let other = if language == Language::JSON {
            Language::YAML
        } else {
            Language::JSON
        };
        switch_to(state, other);
    });
    counts.to_vec()
}

/// Twice the copies is twice the text: about twice the work of each kind
/// (a pass over the text for each placeholder is four times).
#[test]
fn a_text_holding_its_keys_several_times_checks_placeholders_in_linear_work() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let small = work(language, 2);
        let large = work(language, 4);
        for (index, kind) in Work::ALL.iter().enumerate() {
            if large[index] > 3 * small[index].max(50) {
                costly.push(format!(
                    "{language}: {kind:?} {} for two copies, {} for four",
                    small[index], large[index]
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
