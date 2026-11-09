//! A placeholder is bound to its place: real only in the field the host's
//! text had it in, and one the host wrote in a comment only in that text's
//! comments. Moved or pasted anywhere else it is plain text and reported
//! where it is written (never accepted silently); a host problem about
//! the misplaced token, by path or by line and column, shows at that spot.
//! In every language, for two unrelated patterns, in the host's language
//! and while editing as another.

use leptos::prelude::*;

use super::super::Language;
use super::super::diagnostics::Severity;
use super::super::host_diagnostics::{HostDiagnostic, RangeText, place as place_host};
use super::super::paths::{TextPosition, TextRange};
use super::super::save_matrix::{ALL, report};
use super::super::test_state::TestEditor;
use super::guarantee_tests::{
    HOSTS, has_comments, mount, nth, place, problems, real_texts, replace, sample,
};

/// The problems of the shown text, each `place message`.
fn flagged(state: super::super::state::EditorState, language: Language) -> Vec<String> {
    problems(state, language, None)
}

#[test]
fn a_fields_token_pasted_into_another_field_is_reported_there() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let case = format!("{language} ({})", tokens.pattern);
            let text = sample(language, tokens);
            let (_owner, state) = mount(language, &text, tokens);
            // The token of `a` and `b` pasted over the one of `c`.
            let c = nth(&text, tokens.other, 0);
            replace(state, c.clone(), tokens.same);
            let shown = state.doc().text.get_untracked();
            let want = vec![format!(
                "{} /c: this placeholder stands for the secret of /a, not this field's; here it is plain text",
                place(&shown, c.start, tokens.same)
            )];
            if flagged(state, language) != want {
                failures.push(format!("{case}: {:?}", flagged(state, language)));
            }
            // The two in `a` and `b` stay real; the pasted one is not.
            if real_texts(state)
                .iter()
                .filter(|token| *token == tokens.same)
                .count()
                != 2
            {
                failures.push(format!("{case}: real {:?}", real_texts(state)));
            }
        }
    }
    report(failures);
}

/// The host's problem about the field, or about the line and column the
/// pasted token is at, marks the token there.
#[test]
fn a_host_problem_at_a_misplaced_token_shows_at_that_spot() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let case = format!("{language} ({})", tokens.pattern);
            let text = sample(language, tokens).replacen(tokens.other, tokens.same, 1);
            let at = nth(&text, tokens.same, 2);
            let line = text[..at.start].matches('\n').count() + 1;
            let line_start = text[..at.start].rfind('\n').map_or(0, |at| at + 1);
            let column = text[line_start..at.start].chars().count() + 1;
            let end = column + tokens.same.chars().count();
            let problems = [
                HostDiagnostic::at_path("/c", Severity::Error, "wrong secret"),
                HostDiagnostic::at_range(
                    TextRange::new(
                        TextPosition::new(line, column),
                        TextPosition::new(line, end),
                    ),
                    Severity::Error,
                    "wrong secret here",
                ),
            ];
            let placed = place_host(
                language.behavior(),
                &text,
                &problems,
                RangeText::OnScreen,
                &[],
                crate::strings::default_strings(),
            );
            for problem in &placed {
                // A path marks the value (with its quotes where written).
                let on_token = problem.line == Some(line)
                    && problem
                        .column
                        .is_some_and(|start| start + 1 >= column && start <= column)
                    && problem
                        .end_column
                        .is_some_and(|stop| stop >= end && stop <= end + 1);
                if !on_token {
                    failures.push(format!(
                        "{case}: {} at {:?}:{:?}-{:?}, the token at {line}:{column}-{end}",
                        problem.message, problem.line, problem.column, problem.end_column
                    ));
                }
            }
        }
    }
    report(failures);
}

#[test]
fn a_comments_token_is_real_in_the_comments_only() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL.into_iter().filter(|language| has_comments(*language)) {
            let case = format!("{language} ({})", tokens.pattern);
            let text = sample(language, tokens);
            // Real (atomic) in its comment.
            let (_owner, state) = mount(language, &text, tokens);
            if !real_texts(state).iter().any(|token| token == tokens.note) {
                failures.push(format!("{case}: not real in its comment"));
            }
            // An edit cutting into it takes it whole.
            let note = nth(&text, tokens.note, 0);
            let last = tokens.note.chars().last().unwrap().len_utf8();
            replace(state, note.end - last..note.end, "");
            let cut = format!("{}{}", &text[..note.start], &text[note.end..]);
            if state.doc().text.get_untracked() != cut {
                failures.push(format!(
                    "{case}: Backspace at its end left\n{}",
                    state.doc().text.get_untracked()
                ));
            }

            // Pasted over the token of `c`: plain text there, and said so.
            let (_owner, state) = mount(language, &text, tokens);
            let c = nth(&text, tokens.other, 0);
            replace(state, c.clone(), tokens.note);
            let shown = state.doc().text.get_untracked();
            let want = vec![format!(
                "{} /c: this placeholder stands for a secret in a comment, not this field's; here it is plain text",
                place(&shown, c.start, tokens.note)
            )];
            if flagged(state, language) != want {
                failures.push(format!("{case}: in /c {:?}", flagged(state, language)));
            }
            if real_texts(state)
                .iter()
                .filter(|token| *token == tokens.note)
                .count()
                != 1
            {
                failures.push(format!("{case}: in /c real {:?}", real_texts(state)));
            }

            // A field's token pasted into the comment: plain text, said so.
            let (_owner, state) = mount(language, &text, tokens);
            replace(state, note.clone(), tokens.other);
            let shown = state.doc().text.get_untracked();
            let want = vec![format!(
                "{} This placeholder stands for the secret of /c; in a comment it is plain text",
                place(&shown, note.start, tokens.other)
            )];
            if flagged(state, language) != want {
                failures.push(format!(
                    "{case}: in the comment {:?}",
                    flagged(state, language)
                ));
            }
            if real_texts(state)
                .iter()
                .filter(|token| *token == tokens.other)
                .count()
                != 1
            {
                failures.push(format!(
                    "{case}: in the comment real {:?}",
                    real_texts(state)
                ));
            }

            // Moved to another comment of the text: still real.
            let (_owner, state) = mount(language, &text, tokens);
            let comment_line = text[..note.end].rfind('\n').map_or(0, |at| at + 1);
            let line_end = text[note.end..].find('\n').unwrap() + note.end + 1;
            let line = text[comment_line..line_end].to_string();
            let end = text.len();
            replace(state, end..end, &line);
            replace(state, comment_line..line_end, "");
            if !real_texts(state).iter().any(|token| token == tokens.note)
                || !flagged(state, language).is_empty()
            {
                failures.push(format!(
                    "{case}: moved to the last line, real {:?} problems {:?}\n{}",
                    real_texts(state),
                    flagged(state, language),
                    state.doc().text.get_untracked()
                ));
            }
        }
    }
    report(failures);
}

/// While editing as another language, the same: a token moved to another
/// field of the view is plain text and reported there.
#[test]
fn a_token_moved_in_an_edit_as_view_is_reported_there() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            for view in ALL.into_iter().filter(|view| *view != host) {
                let case = format!("{host} host via {view} ({})", tokens.pattern);
                let text = sample(host, tokens);
                let mut editor = TestEditor::new(host, &text);
                editor.edit_as = Some(view);
                editor.secrets = Some(tokens.options());
                let (_owner, state) = editor.mount();
                let shown = state.doc().text.get_untracked();
                let Some(c) = shown.find(tokens.other) else {
                    failures.push(format!("{case}: no token of c in\n{shown}"));
                    continue;
                };
                replace(state, c..c + tokens.other.len(), tokens.same);
                let shown = state.doc().text.get_untracked();
                let found = flagged(state, view);
                let want = format!(
                    "{} /c: this placeholder stands for the secret of /a",
                    place(&shown, c, tokens.same)
                );
                if found.len() != 1 || !found[0].starts_with(&want) {
                    failures.push(format!("{case}: {found:?}"));
                }
            }
        }
    }
    report(failures);
}
