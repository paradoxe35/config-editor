//! The handle's host document: `value()` in the host's language, with
//! pending edits applied, and `unified_diff` of the baseline against it.

use std::cell::RefCell;

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::handle::EditorHandle;
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

thread_local! {
    static SENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn sent() -> Vec<String> {
    SENT.with(|sent| std::mem::take(&mut *sent.borrow_mut()))
}

/// An editor on the YAML `host`, shown as `edit_as`, with a handle.
fn editor(
    host: &str,
    edit_as: Option<Language>,
) -> (Owner, crate::editor::state::EditorState, EditorHandle) {
    let (owner, state) = TestEditor {
        edit_as,
        on_change: Some(Callback::new(|text: String| {
            SENT.with(|sent| sent.borrow_mut().push(text));
        })),
        ..TestEditor::new(Language::YAML, host)
    }
    .mount();
    let handle = EditorHandle::new();
    handle.attach(state);
    sent();
    (owner, state, handle)
}

/// Replaces the text on screen with `text`, as a command does.
fn replace(state: crate::editor::state::EditorState, text: &str) {
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    assert!(state.commit(Edit::new(before, (0, end), text.to_string(), 0)));
}

#[test]
fn value_is_the_text_without_edit_as() {
    let (_owner, state, handle) = editor("a: 1\n", None);
    assert_eq!(handle.value().as_deref(), Some("a: 1\n"));
    replace(state, "a: 2\n");
    assert_eq!(handle.value(), handle.text());
    assert_eq!(handle.value().as_deref(), Some("a: 2\n"));
}

/// A CRLF host gets its document back with CRLF, as `on_change` does; the
/// screen text and the diff stay LF.
#[test]
fn value_keeps_the_hosts_line_endings() {
    let (_owner, state, handle) = editor("a: 1\r\nb: x\r\n", None);
    assert_eq!(handle.text().as_deref(), Some("a: 1\nb: x\n"));
    assert_eq!(handle.value().as_deref(), Some("a: 1\r\nb: x\r\n"));
    replace(state, "a: 2\nb: x\n");
    assert_eq!(handle.value().as_deref(), Some("a: 2\r\nb: x\r\n"));
    assert_eq!(sent(), ["a: 2\r\nb: x\r\n"]);
    #[cfg(feature = "diff")]
    assert_eq!(
        handle.unified_diff(0).as_deref(),
        Some("--- baseline\n+++ current\n@@ -1 +1 @@\n-a: 1\n+a: 2\n")
    );
}

#[cfg(feature = "json")]
#[test]
fn value_is_the_host_document_while_editing_as_another_language() {
    let (_owner, state, handle) = editor("a: 1\nb: x\n", Some(Language::JSON));
    let shown = handle.text().unwrap();
    assert!(shown.starts_with('{'), "{shown}");
    // Still in the typing pause: converted when asked, and sent first.
    replace(state, &shown.replacen('1', "2", 1));
    assert_eq!(handle.value().as_deref(), Some("a: 2\nb: x\n"));
    assert!(handle.text().unwrap().starts_with('{'));
    assert_eq!(sent(), ["a: 2\nb: x\n"]);
    // Text that does not convert: no value, nothing more sent.
    replace(state, "{\"a\": ");
    assert_eq!(handle.value(), None);
    assert_eq!(sent(), Vec::<String>::new());
}

#[cfg(all(feature = "json", feature = "diff"))]
#[test]
fn unified_diff_compares_the_baseline_with_the_host_document() {
    let (_owner, state, handle) = editor("a: 1\nb: x\n", Some(Language::JSON));
    assert_eq!(handle.unified_diff(3).as_deref(), Some(""));
    let shown = handle.text().unwrap();
    replace(state, &shown.replacen('1', "2", 1));
    let diff = handle.unified_diff(0).unwrap();
    assert!(diff.contains("-a: 1\n+a: 2\n"), "{diff}");
    assert!(!diff.contains("b: x"), "{diff}");
    // A new baseline is the one compared with.
    state.editing().baseline.set("a: 2\nb: x\n".to_string());
    assert_eq!(handle.unified_diff(3).as_deref(), Some(""));
    replace(state, "{");
    assert_eq!(handle.unified_diff(3), None);
}

#[test]
fn a_detached_handle_has_no_value() {
    let handle = EditorHandle::new();
    assert_eq!(handle.value(), None);
    #[cfg(feature = "diff")]
    assert_eq!(handle.unified_diff(3), None);
}
