//! A secret placeholder is real only in the field the host gave it in:
//! pasted into another field, or left under a renamed key, it is plain text
//! and flagged (it must never stand for another field's secret); cut and
//! pasted back into its own field, it is real again.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::problems::unknown_placeholders;
use super::secret_tests::{PIN, SECRET, documents, schema};
use super::tokens::SecretMatcher;

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Replaces `range` (bytes) of the shown text with `with`, as typing or a
/// paste would.
fn replace(state: EditorState, range: std::ops::Range<usize>, with: &str) -> bool {
    let before = state.doc.text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::new(before, (start, end), after, caret))
}

/// Whether the token written at byte `at` of the shown text is real.
fn real_at(state: EditorState, at: usize) -> bool {
    let text = state.doc.text.get_untracked();
    let at = utf16(&text, at);
    state
        .protect
        .secrets
        .tokens
        .with_untracked(|tokens| tokens.iter().any(|token| token.start == at))
}

/// The warnings on placeholders of the shown text.
fn flagged(state: EditorState, language: Language) -> Vec<String> {
    let text = state.doc.text.get_untracked();
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let schema = schema();
    state.protect.secrets.known.with_untracked(|known| {
        state.protect.secrets.tokens.with_untracked(|tokens| {
            unknown_placeholders(
                language.behavior(),
                &text,
                &matcher,
                known,
                tokens,
                Some(&schema),
                crate::strings::default_strings(),
            )
            .into_iter()
            .map(|problem| problem.message)
            .collect()
        })
    })
}

#[test]
fn a_placeholder_is_real_only_in_its_own_field_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        // Pasted over the pin's token: plain there, and flagged.
        let (_owner, state) = TestEditor::new(language, &text).mount();
        let pin = text.find(PIN).unwrap();
        // Pasted over the pin's token selected whole: the two tokens share
        // their first characters, and the paste still replaces it whole.
        replace(state, pin..pin + PIN.len(), SECRET);
        let pasted = pin;
        if !state.doc.text.get_untracked()[pin..].starts_with(SECRET) {
            failures.push(format!(
                "{language}: the paste over the pin's token left {:?}",
                state.doc.text.get_untracked()
            ));
        }
        if real_at(state, pasted) {
            failures.push(format!("{language}: the secret's token is real in /pin"));
        }
        if !flagged(state, language)
            .iter()
            .any(|message| message.starts_with("/pin") && message.contains("/secret"))
        {
            failures.push(format!(
                "{language}: not flagged in /pin: {:?}",
                flagged(state, language)
            ));
        }

        // Cut whole and pasted back into its own field: real again.
        let (_owner, state) = TestEditor::new(language, &text).mount();
        let at = text.find(SECRET).unwrap();
        replace(state, at..at + SECRET.len(), "");
        replace(state, at..at, SECRET);
        if !real_at(state, at) || !flagged(state, language).is_empty() {
            failures.push(format!(
                "{language}: pasted back, real {} flagged {:?}",
                real_at(state, at),
                flagged(state, language)
            ));
        }

        // Its key renamed: the token no longer stands in its field.
        let (_owner, state) = TestEditor::new(language, &text).mount();
        let key = text.find("secret").unwrap();
        replace(state, key..key + "secret".len(), "secrex");
        let shown = state.doc.text.get_untracked();
        let moved = shown.find(SECRET).unwrap();
        if real_at(state, moved) {
            failures.push(format!("{language}: real under a renamed key:\n{shown}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
