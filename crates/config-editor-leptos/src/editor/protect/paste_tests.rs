//! A placeholder pasted whole stays whole, in every language: over another
//! placeholder (the two share their first characters), into an emptied
//! value, and over a selection cutting into a placeholder (which goes
//! whole). The same whether the browser said what the paste replaced (a
//! `beforeinput`) or not (an `insertFromPaste` input alone, as synthetic
//! events and `execCommand` send): then it is read off the caret the paste
//! left, which ends the pasted text.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::input::UNKNOWN_SELECTION;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::secret_tests::{PIN, SECRET, documents};

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Pastes `with` over `range` (bytes) of the shown text, the replaced
/// selection told (`known`) or not; the caret ends the pasted text.
fn paste(state: EditorState, range: std::ops::Range<usize>, with: &str, known: bool) {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let selection = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = selection.0 + with.encode_utf16().count();
    let told = if known { selection } else { UNKNOWN_SELECTION };
    state.commit(Edit::input(
        before,
        told,
        after,
        (caret, caret),
        "insertFromPaste".to_string(),
        0.0,
    ));
}

#[test]
fn a_placeholder_pasted_whole_stays_whole_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let pin = text.find(PIN).expect("the pin's token");
        let pin_end = pin + PIN.len();
        for known in [true, false] {
            let how = if known { "with" } else { "without" };
            let case = |what: &str| format!("{language} {how} beforeinput, {what}");
            let mut check = |what: &str, state: EditorState, expected: String| {
                let shown = state.doc().text.get_untracked();
                if shown != expected {
                    failures.push(format!("{}: {shown:?}, expected {expected:?}", case(what)));
                }
            };

            // Over the pin's token, selected whole.
            let (_owner, state) = TestEditor::new(language, &text).mount();
            paste(state, pin..pin_end, SECRET, known);
            let over = format!("{}{SECRET}{}", &text[..pin], &text[pin_end..]);
            check("over a placeholder", state, over);

            // Into the value emptied first.
            let (_owner, state) = TestEditor::new(language, &text).mount();
            paste(state, pin..pin_end, "", true);
            paste(state, pin..pin, SECRET, known);
            let into = format!("{}{SECRET}{}", &text[..pin], &text[pin_end..]);
            check("into an empty value", state, into);

            // Over a selection from before the token into its middle: the
            // token goes whole, the paste stays whole.
            let (_owner, state) = TestEditor::new(language, &text).mount();
            let cut_from = pin - 1;
            let cut_to = pin + "«secret 0".len();
            paste(state, cut_from..cut_to, SECRET, known);
            let cutting = format!("{}{SECRET}{}", &text[..cut_from], &text[pin_end..]);
            check(
                "over a selection cutting into a placeholder",
                state,
                cutting,
            );

            // Not a paste: Backspace at the token's end (the browser tells
            // the collapsed caret before it) still takes it whole.
            let (_owner, state) = TestEditor::new(language, &text).mount();
            let end = utf16(&text, pin_end);
            let after = format!("{}{}", &text[..pin_end - "»".len()], &text[pin_end..]);
            let told = if known { (end, end) } else { UNKNOWN_SELECTION };
            state.commit(Edit::input(
                text.clone(),
                told,
                after,
                (end - 1, end - 1),
                "deleteContentBackward".to_string(),
                0.0,
            ));
            let removed = format!("{}{}", &text[..pin], &text[pin_end..]);
            check("Backspace at the token's end", state, removed);
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
