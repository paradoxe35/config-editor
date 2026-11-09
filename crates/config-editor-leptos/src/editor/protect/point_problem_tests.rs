//! A host problem at a position with no end, on a placeholder the host's
//! pattern matches, marks the whole token in the live editor: in every
//! language, for two unrelated patterns, from any column inside it.

use leptos::prelude::*;

use super::super::diagnostics::{DiagnosticOrigin, Severity};
use super::super::host_diagnostics::HostDiagnostic;
use super::super::paths::TextPosition;
use super::super::save_matrix::{ALL, report};
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, nth, place, sample};

#[test]
fn a_host_point_on_a_placeholder_marks_the_whole_token_in_every_language() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let text = sample(language, tokens);
            let at = nth(&text, tokens.other, 0);
            let line = text[..at.start].matches('\n').count() + 1;
            let line_start = text[..at.start].rfind('\n').map_or(0, |at| at + 1);
            let column = text[line_start..at.start].chars().count() + 1;
            let mut editor = TestEditor::new(language, &text);
            editor.secrets = Some(tokens.options());
            let inside = column + 3;
            editor.host_diagnostics = Signal::stored(vec![HostDiagnostic::at_position(
                TextPosition::new(line, inside),
                Severity::Error,
                "the secret was rotated",
            )]);
            let (_owner, state) = editor.mount();
            let marked: Vec<String> = state
                .problems()
                .diagnostics
                .get_untracked()
                .into_iter()
                .filter(|problem| problem.origin == DiagnosticOrigin::Host)
                .map(|problem| {
                    format!(
                        "{}:{}-{}",
                        problem.line.unwrap_or(0),
                        problem.column.unwrap_or(0),
                        problem.end_column.unwrap_or(0)
                    )
                })
                .collect();
            let want = vec![place(&text, at.start, tokens.other)];
            if marked != want {
                failures.push(format!(
                    "{language} ({}): marked {marked:?}, the token at {want:?}",
                    tokens.pattern
                ));
            }
        }
    }
    report(failures);
}
