//! P1 across the documents of a documents view: one document's secret
//! placeholder pasted into another (glued into a value) stands for no
//! secret there, and a problem says so where it is written, in every
//! language. The view gives each editor every editor's placeholders; an
//! editor's own are real only in its own fields.

use std::sync::Arc;

use leptos::prelude::*;

use super::super::Language;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{problems, replace};
use super::secret_tests::{ID, PIN, SECRET, documents};

/// Another document's placeholder.
const OTHER: &str = "«secret 9b1c»";

#[test]
fn another_documents_placeholder_pasted_in_is_flagged_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let (_other_owner, other) =
            TestEditor::new(Language::YAML, &format!("key: {OTHER}\n")).mount();
        let shared = Signal::derive(move || {
            let theirs = other.protect().own_placeholders("tls.yaml".to_string());
            Arc::new(theirs.into_iter().collect::<Vec<_>>())
        });
        let mut editor = TestEditor::new(language, &text);
        editor.foreign = Some(shared);
        let (_owner, state) = editor.mount();
        // Glued after the `name` value's `a`.
        let name = text.find("name").expect("a name field") + 4;
        let at = name + text[name..].find('a').expect("its value") + 1;
        replace(state, at..at, OTHER);
        let found = problems(state, language, None);
        let line = text[..at].matches('\n').count() + 1;
        if !found
            .iter()
            .any(|problem| problem.starts_with(&format!("{line}:")) && problem.contains("tls.yaml"))
        {
            failures.push(format!("{language}: glued on line {line}: {found:?}"));
        }
        // This document's own placeholder, in its own field, is not flagged.
        if found.iter().any(|problem| problem.contains(SECRET)) {
            failures.push(format!("{language}: its own flagged: {found:?}"));
        }
        // The problems the editor lists (`on_problems`) hold it too.
        state
            .problems()
            .diagnosed_text
            .set(state.doc().text.get_untracked());
        let listed = state.problems().own.get_untracked();
        if !listed
            .iter()
            .any(|problem| problem.line == Some(line) && problem.message.contains("tls.yaml"))
        {
            failures.push(format!("{language}: not listed: {listed:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// P1: another document's placeholder pasted in is as solid here as a
/// placeholder of this document's own: an edit cutting it (Backspace on
/// its last character, part of it selected and deleted) takes it whole,
/// a drag of part of it is refused, and Replace all leaves a match inside
/// it, in every language.
#[test]
fn another_documents_placeholder_pasted_in_is_never_cut() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let name = text.find("name").expect("a name field") + 4;
        let at = name + text[name..].find('a').expect("its value") + 1;
        let end = at + OTHER.len();
        // An editor of `text` with the other document's token pasted at
        // `at` (the owners of both editors kept with it).
        let pasted = || {
            let (theirs_owner, theirs) =
                TestEditor::new(Language::YAML, &format!("key: {OTHER}\n")).mount();
            let shared = Signal::derive(move || {
                let tokens = theirs.protect().own_placeholders("tls.yaml".to_string());
                Arc::new(tokens.into_iter().collect::<Vec<_>>())
            });
            let mut editor = TestEditor::new(language, &text);
            editor.foreign = Some(shared);
            let (owner, state) = editor.mount();
            replace(state, at..at, OTHER);
            (theirs_owner, owner, state)
        };
        let (_theirs, _owner, state) = pasted();
        let shown = state.doc().text.get_untracked();
        let units = |byte: usize| shown[..byte].encode_utf16().count();
        if !state.cuts_a_token(units(at + '«'.len_utf8()), units(end)) {
            failures.push(format!("{language}: a drag of part of it taken"));
        }
        if !state
            .protected_bytes(&shown)
            .iter()
            .any(|range| range.start <= at && end <= range.end)
        {
            failures.push(format!("{language}: Replace all may cut it"));
        }
        for (what, cut) in [
            ("Backspace on its last character", end - '»'.len_utf8()..end),
            ("part of it deleted", at + '«'.len_utf8()..at + 9),
        ] {
            let (_theirs, _owner, state) = pasted();
            replace(state, cut, "");
            let now = state.doc().text.get_untracked();
            if now.contains("9b1c") || now.matches('«').count() != text.matches('«').count() {
                failures.push(format!("{language}: {what}: {now:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// P1: with a copy of this document's own placeholder pasted over the
/// `name` value (plain text there), an undo and a redo leave the real one
/// in its field real: a cut into it, or a key typed into it, takes it
/// whole, in every language. (Found again by their text after the undo,
/// the copy, written first, was taken for the real one.)
#[test]
fn the_real_placeholder_stays_real_past_an_undo_of_a_copy_written_before_it() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let name = text.find("name").expect("a name field") + 4;
        let at = name + text[name..].find('a').expect("its value");
        let pasted = format!("{}{SECRET}{}", &text[..at], &text[at + 1..]);
        let real = pasted.rfind(SECRET).expect("the real one") + SECRET.len();
        for (what, cut, with) in [
            ("the real one cut into", real - 5..real - 2, ""),
            ("a key typed into the real one", real - 5..real - 5, "c"),
        ] {
            let (_owner, state) = TestEditor::new(language, &text).mount();
            replace(state, at..at + 1, SECRET);
            if state.doc().text.get_untracked() != pasted {
                failures.push(format!("{language}: the copy not pasted"));
                continue;
            }
            // Undone and redone: the tokens found again by their text.
            for redo in [false, true] {
                if let Some(step) = state.history_edit(redo) {
                    state.commit(step);
                }
            }
            replace(state, cut, with);
            let now = state.doc().text.get_untracked();
            // Every `«` starts a whole token the host gave.
            let whole = now.match_indices('«').all(|(start, _)| {
                [SECRET, PIN, ID]
                    .iter()
                    .any(|token| now[start..].starts_with(token))
            });
            if !whole {
                failures.push(format!("{language}: {what}: {now:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
