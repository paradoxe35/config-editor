//! P4: while a change is held for a placeholder the parse reads with text
//! beside it (see `whole_guard::Held`), the problems say so ("Not saved",
//! at the placeholder), as for every other held change, not only when the
//! user saves.

use leptos::prelude::*;

use super::super::Language;
use super::super::test_state::TestEditor;
use super::guarantee_tests::replace;
use super::secret_tests::SECRET;
use super::whole_guard::Held;

/// A key typed a character at a time before a placeholder's entry
/// (`passwordx: password: '…'`, see `whole_guard::Held`): the hold is
/// listed at the placeholder while the text does not read it as the host's
/// text does, and goes when it does again.
#[test]
fn a_held_placeholder_is_listed_as_not_saved_while_it_is_held() {
    let host = format!("name: a\npassword: '{SECRET}'\n");
    let (_owner, state) = TestEditor::new(Language::YAML, &host).mount();
    let listed = |text: &str| {
        state.problems().diagnosed_text.set(text.to_string());
        state.problems().own.get_untracked()
    };
    let at = host.find("password").expect("the key");
    replace(state, at..at, "x: ");
    let now = state.doc().text.get_untracked();
    state.protect().secrets.held.set_value(Some(Held {
        field: "/password".into(),
        tokens: vec![SECRET.into()],
    }));
    assert!(state.changes_held(), "held in {now:?}");
    let line = now[..now.find(SECRET).expect("it")].matches('\n').count() + 1;
    let problems = listed(&now);
    assert!(
        problems.iter().any(
            |problem| problem.message.starts_with("Not saved: /password")
                && problem.line == Some(line)
        ),
        "{problems:#?}"
    );
    // The text reading the placeholder as the host's again: nothing listed.
    replace(state, at..at + "x: ".len(), "");
    let now = state.doc().text.get_untracked();
    assert!(!state.changes_held(), "released in {now:?}");
    let problems = listed(&now);
    assert!(
        problems
            .iter()
            .all(|problem| !problem.message.starts_with("Not saved")),
        "{problems:#?}"
    );
}
