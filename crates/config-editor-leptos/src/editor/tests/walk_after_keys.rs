//! F8 walks the problems the editor lists, where it lists them, right
//! after keys that moved them: a switch waiting for the text to convert
//! says why on the line the reason is on now, not on the line it was on
//! before a line was put above it.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;

#[test]
fn a_waiting_switch_is_placed_on_the_text_f8_walks() {
    let text = "listen: 7835\npublic_address: tunnel.example.com\n''";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    switch_to(state, Language::HCL);
    assert!(state.sync().pending_switch.get_untracked().is_some());
    // A line put above the line that cannot convert (line 3, then 4).
    let at = text.rfind("''").expect("the line");
    let after = format!("{}\n{}", &text[..at], &text[at..]);
    state.commit(Edit::new(text.to_string(), (at, at), after, at + 1));
    state.check_text_now();
    let listed = state.problems().own.get_untracked();
    assert!(
        !listed.is_empty() && listed.iter().all(|problem| problem.line == Some(4)),
        "{listed:?}"
    );
}
