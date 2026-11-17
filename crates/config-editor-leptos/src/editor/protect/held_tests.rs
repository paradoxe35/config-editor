//! A locked field held (a second copy of its block read as a list in the
//! languages that repeat a block for one: XML, HCL): the copy is taken,
//! the locked text keeps refusing typing, the save waits with the field's
//! message, and deleting the copy lets it save again. A keystroke far from
//! it while it is held costs the same few readings of the text at any size
//! (the held lock is followed by position, not looked for).

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::paths::FieldPath;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::reach_tests::{services, type_in_middle};

/// The locked port's pointer, and a second copy of the first service's
/// block, written where the text ends inside the services.
fn copy(language: Language) -> (&'static str, &'static str, &'static str) {
    match language.label() {
        "XML" => (
            "/root/services/svc_0/port",
            "  </services>",
            "    <svc_0>\n      <port>9</port>\n    </svc_0>\n",
        ),
        _ => (
            "/services/svc_0/port",
            "}\n",
            "  svc_0 {\n    port = 9\n  }\n",
        ),
    }
}

fn mount(language: Language, blocks: usize) -> (Owner, EditorState, String) {
    let text = services(language, blocks);
    let (pointer, _, _) = copy(language);
    let (owner, state) = TestEditor {
        read_only_paths: vec![FieldPath::from(pointer)],
        on_save: Some(Callback::new(|_: SaveRequest| {})),
        ..TestEditor::new(language, &text)
    }
    .mount();
    (owner, state, text)
}

/// Pastes the copy; the text it made.
fn paste_copy(state: EditorState, language: Language) -> String {
    let (_, before_line, block) = copy(language);
    let text = state.doc().text.get_untracked();
    let byte = text.rfind(before_line).expect("the end of the services");
    let at = text[..byte].encode_utf16().count();
    let after = format!("{}{block}{}", &text[..byte], &text[byte..]);
    let caret = at + block.encode_utf16().count();
    assert!(
        state.commit(Edit::input_units(
            text,
            (at, at),
            after.clone(),
            (caret, caret),
            "insertFromPaste".into(),
            0.0,
        )),
        "{language}: the copy is taken"
    );
    after
}

/// Whether a digit typed into the locked port (`0` of the first service)
/// is refused.
fn port_refuses_typing(state: EditorState, language: Language) -> bool {
    let text = state.doc().text.get_untracked();
    let port = text.find("port").expect("the first port");
    let digit = port + text[port..].find('0').expect("its value");
    let at = text[..digit].encode_utf16().count();
    let mut typed = text.clone();
    typed.insert(digit, '7');
    let _ = language;
    !state.commit(Edit::input_units(
        text,
        (at, at),
        typed,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    ))
}

#[test]
fn a_second_copy_of_a_locked_block_is_taken_and_holds_the_save_until_it_goes() {
    for language in [Language::XML, Language::HCL] {
        let (_owner, state, text) = mount(language, 3);
        assert!(state.save(), "{language}: saves before");
        paste_copy(state, language);
        let held = state.protect().protected.with_untracked(|p| p.held.clone());
        assert_eq!(held, [copy(language).0], "{language}: held");
        assert!(
            port_refuses_typing(state, language),
            "{language}: still locked"
        );
        assert!(!state.save(), "{language}: saved with the copy");
        let message = state.protect().tip().expect("the save says why").message;
        assert!(message.contains(copy(language).0), "{language}: {message}");
        // Taking the copy out frees the save.
        let now = state.doc().text.get_untracked();
        assert!(state.commit(Edit::new_units(now, (0, 0), text.clone(), 0)));
        assert_eq!(state.doc().text.get_untracked(), text);
        assert!(state.save(), "{language}: saves once the copy is gone");
    }
}

#[test]
fn typing_while_a_lock_is_held_reads_the_text_as_often_at_any_size() {
    let mut costly = Vec::new();
    for language in [Language::XML, Language::HCL] {
        let reads: Vec<usize> = [40, 80]
            .into_iter()
            .map(|blocks| {
                let (_owner, state, _) = mount(language, blocks);
                paste_copy(state, language);
                assert!(
                    state
                        .protect()
                        .protected
                        .with_untracked(|p| !p.held.is_empty()),
                    "{language}: held"
                );
                counted(Work::DocumentRead, || type_in_middle(state, blocks)).1
            })
            .collect();
        if reads[1] > reads[0] {
            costly.push(format!(
                "{language}: {reads:?} readings for 40 and 80 blocks"
            ));
        }
    }
    assert!(costly.is_empty(), "{costly:#?}");
}
