//! Byte for byte: editing in the host's language, `on_change` hands back
//! the host's text with exactly the edit made, placeholders and all: its
//! line endings, byte order mark, final newline (or none), tabs and quoting
//! as they were. A text that did not change (on load, the host sending the
//! same text again, an edit undone by the next) is not reported as a new
//! one. In every language, for two unrelated placeholder patterns.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::save_matrix::{ALL, report};
use super::super::state::normalize_newlines;
use super::super::sync::take_host_value;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, Tokens, has_comments, sample, utf16};

/// The host's text in its forms: as written, with CRLF endings, with a
/// byte order mark, without a final newline, with tabs, with the other
/// quotes where the language has them.
fn forms(language: Language, tokens: &Tokens) -> Vec<(&'static str, String)> {
    let text = sample(language, tokens);
    let mut forms = vec![
        ("as written", text.clone()),
        ("CRLF", text.replace('\n', "\r\n")),
        ("BOM", format!("\u{feff}{text}")),
        ("no final newline", text.trim_end_matches('\n').to_string()),
    ];
    let tabbed = if has_comments(language) {
        text.replacen(" note ", "\tnote\t", 1)
    } else {
        text.replace("\n  ", "\n\t")
    };
    forms.push(("tabs", tabbed));
    if matches!(language.label(), "YAML" | "TOML") {
        let quoted = sample(
            language,
            &Tokens {
                quoted_in_yaml: true,
                ..*tokens
            },
        );
        forms.push(("single quotes", quoted.replace('"', "'")));
    }
    forms.push((
        "everything",
        format!(
            "\u{feff}{}",
            text.replace('\n', "\r\n").trim_end_matches("\r\n")
        ),
    ));
    forms
}

/// The host's `value` set to `text`, as the component takes it.
fn host_sends(state: super::super::state::EditorState, text: &str) {
    state.sync().line_endings.note(text);
    take_host_value(state, normalize_newlines(text.to_string()));
}

/// Byte offset of the name's value (`a`) in `text`, in every form.
fn name_value(text: &str) -> usize {
    let name = text.find("name").expect("the name") + "name".len();
    name + text[name..].find('a').expect("its value")
}

#[test]
fn on_change_hands_back_the_hosts_text_with_exactly_the_edit() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            for (form, text) in forms(language, tokens) {
                let case = format!("{language}, {form} ({})", tokens.pattern);
                if let Err(error) = language.parse(&text) {
                    failures.push(format!(
                        "{case}: the sample does not parse: {}",
                        error.message
                    ));
                    continue;
                }
                let heard = RwSignal::new(Vec::<String>::new());
                let mut editor = TestEditor::new(language, &text);
                editor.secrets = Some(tokens.options());
                editor.on_change = Some(Callback::new(move |text| {
                    heard.update(|heard| heard.push(text));
                }));
                let (_owner, state) = editor.mount();
                state.recheck_tokens();
                // The host sending its text again changes nothing.
                host_sends(state, &text);
                if !heard.get_untracked().is_empty() {
                    failures.push(format!(
                        "{case}: heard {:?} before any edit",
                        heard.get_untracked()
                    ));
                }
                // `b` typed after the name's value.
                let before = state.doc().text.get_untracked();
                let at = utf16(&before, name_value(&before) + 1);
                let after = {
                    let byte = name_value(&before) + 1;
                    format!("{}b{}", &before[..byte], &before[byte..])
                };
                state.commit(Edit::new(before.clone(), (at, at), after.clone(), at + 1));
                let host_at = name_value(&text) + 1;
                let expected = format!("{}b{}", &text[..host_at], &text[host_at..]);
                if heard.get_untracked() != vec![expected.clone()] {
                    failures.push(format!(
                        "{case}: heard {:?}, expected {expected:?}",
                        heard.get_untracked()
                    ));
                }
                // And deleted again: the host's own text, byte for byte.
                state.commit(Edit::new(after, (at, at + 1), before, at));
                if heard.get_untracked().last() != Some(&text) {
                    failures.push(format!(
                        "{case}: back to {:?}, expected {text:?}",
                        heard.get_untracked().last()
                    ));
                }
                let heard_now = heard.get_untracked().len();
                // Its echo changes nothing either.
                host_sends(state, &text);
                state.recheck_tokens();
                if heard.get_untracked().len() != heard_now {
                    failures.push(format!("{case}: the echo was reported"));
                }
            }
        }
    }
    report(failures);
}
