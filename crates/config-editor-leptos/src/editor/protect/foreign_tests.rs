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
use super::secret_tests::{SECRET, documents};

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
