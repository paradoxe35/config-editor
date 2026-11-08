//! A placeholder pattern the editor cannot use (it does not compile, or
//! matches empty text) is reported as a document problem, worded as the
//! npm package throws it, and no token is taken for a placeholder.

use leptos::prelude::*;

use super::super::diagnostics::Severity;
use super::super::languages::Language;
use super::super::secret_options::SecretOptions;
use super::super::test_state::TestEditor;

const HOST: &str = "name: a\nsecret: «secret 1»\n";

#[test]
fn an_unusable_placeholder_pattern_is_a_document_problem_and_matches_nothing() {
    let mut failures = Vec::new();
    for (pattern, says) in [
        ("(", "\"(\" is not a regular expression"),
        (".*", "\".*\" matches empty text"),
        ("x?", "\"x?\" matches empty text"),
    ] {
        let mut editor = TestEditor::new(Language::YAML, HOST);
        editor.secrets = Some(SecretOptions::default().with_placeholder(pattern));
        let (_owner, state) = editor.mount();
        let problems = state.problems().diagnostics.get_untracked();
        let reported = problems.iter().any(|problem| {
            problem.line.is_none()
                && problem.severity == Severity::Error
                && problem
                    .message
                    .starts_with(&format!("secrets.placeholder: {says}"))
        });
        if !reported {
            failures.push(format!("{pattern}: not reported in {problems:?}"));
        }
        let tokens = state.protect().secrets.tokens.get_untracked();
        let known = state.protect().secrets.known.get_untracked();
        if !tokens.is_empty() || !known.is_empty() {
            failures.push(format!("{pattern}: tokens {tokens:?}, known {known:?}"));
        }
    }
    // The default pattern and a valid one report nothing.
    for options in [
        SecretOptions::default(),
        SecretOptions::default().with_placeholder("«secret [0-9]+»"),
    ] {
        let mut editor = TestEditor::new(Language::YAML, HOST);
        editor.secrets = Some(options);
        let (_owner, state) = editor.mount();
        let problems = state.problems().diagnostics.get_untracked();
        if !problems.is_empty() || state.protect().secrets.known.get_untracked().is_empty() {
            failures.push(format!("a valid pattern: {problems:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
