//! F8 walks the problems the editor lists, where it lists them, right
//! after keys that moved them: a switch waiting for the text to convert
//! says why on the line the reason is on now, not on the line it was on
//! before a line was put above it.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
#[cfg(feature = "protect")]
use super::super::keys::check_before_walk;
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

/// F8 checks the text before it walks, with the secret placeholders read
/// afresh first as the typing pause reads them, so it walks the problems
/// the editor then lists (a placeholder pasted into a key, or a key
/// renamed over one, was still taken as real where it had been).
#[cfg(feature = "protect")]
#[test]
fn f8_walks_the_problems_of_the_placeholders_as_they_are_now() {
    use super::super::secret_options::SecretOptions;
    let token = "\u{ab}secret 3f2a91c0\u{bb}";
    let text = format!("clients:\n  acme:\n    secret: {token}\nother: {token}\n");
    let pasted = format!("{token} acme");
    let edits = [
        ("acme", pasted.as_str()),
        ("other", "renamed"),
        ("  acme:\n", "  acme:\n  x: 1\n"),
    ];
    let mut failures = Vec::new();
    for (find, with) in edits {
        let mut editor = TestEditor::new(Language::YAML, &text);
        editor.secrets = Some(SecretOptions::default().with_placeholder(r"«secret [0-9a-f]{8}»"));
        let (_owner, state) = editor.mount();
        let before = state.doc().text.get_untracked();
        let at = before.find(find).expect("the text to edit");
        let after = format!("{}{with}{}", &before[..at], &before[at + find.len()..]);
        let start = before[..at].encode_utf16().count();
        let end = start + find.encode_utf16().count();
        let caret = start + with.encode_utf16().count();
        state.commit(Edit::new(before, (start, end), after, caret));
        check_before_walk(state);
        let walked = state.problems().own.get_untracked();
        // What the typing pause lists for the same text.
        state.recheck_tokens();
        let listed = state.problems().own.get_untracked();
        if walked != listed {
            failures.push(format!(
                "{find} -> {with}: walked {walked:?}\nlisted {listed:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
