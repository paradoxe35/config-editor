//! A secret field's element copied in an XML view (`Shift+Alt+Down`, or
//! the element typed again below it): the field becomes a list of copies
//! of the placeholder. The copy is flagged as a copy, as a copied line is
//! in every language (QA 36: the host's own line was flagged as standing
//! for another field's secret), and the save waits, since the host would
//! read a list of copies as plain text and the secret would be gone
//! (round 29 saved).

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::edit_as::EditAsText;
use super::super::save_matrix::report;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, problems, utf16};
use super::texts;
use crate::strings::default_strings;

/// A host in `language` whose `/db/password` holds `token`.
fn host(language: Language, token: &str) -> String {
    match language.label() {
        "YAML" => format!("db:\n  host: h\n  password: \"{token}\"\n  name: app\n"),
        "JSON" => format!(
            "{{\n  \"db\": {{\n    \"host\": \"h\",\n    \"password\": \"{token}\",\n    \"name\": \"app\"\n  }}\n}}\n"
        ),
        "TOML" => format!("[db]\nhost = \"h\"\npassword = \"{token}\"\nname = \"app\"\n"),
        "HCL" => format!("db {{\n  host = \"h\"\n  password = \"{token}\"\n  name = \"app\"\n}}\n"),
        other => panic!("no host in {other}"),
    }
}

#[test]
fn a_secret_element_copied_in_an_xml_view_is_flagged_as_a_copy_and_waits() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            let case = format!("{language} host ({})", tokens.pattern);
            let mut editor = TestEditor::new(language, &host(language, tokens.same));
            editor.edit_as = Some(Language::XML);
            editor.secrets = Some(tokens.options());
            let heard = RwSignal::new(Vec::<String>::new());
            editor.on_change = Some(Callback::new(move |text| {
                heard.update(|heard| heard.push(text));
            }));
            let (_owner, state) = editor.mount();
            // The password's line, copied below it.
            let before = state.doc().text.get_untracked();
            let Some(start) = before.find("<password>") else {
                failures.push(format!("{case}: no password in {before:?}"));
                continue;
            };
            let line_start = before[..start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = start + before[start..].find('\n').unwrap_or(0) + 1;
            let line = &before[line_start..line_end];
            let after = format!("{}{line}{}", &before[..line_end], &before[line_end..]);
            let at = utf16(&before, line_end);
            state.commit(Edit::new_units(before, (at, at), after.clone(), at));
            state.recheck_tokens();
            convert_now(state, after.clone(), Language::XML);
            let flagged: Vec<String> = problems(state, Language::XML, None)
                .into_iter()
                .filter(|problem| problem.contains("/db/password"))
                .collect();
            // The copy, on the line below the host's: flagged as a copy,
            // as in the other languages; the host's line is not.
            let extra = texts::extra_placeholder(default_strings(), "/db/password");
            let copy_line = after[..after.rfind("<password>").unwrap_or(0)]
                .matches('\n')
                .count()
                + 1;
            if flagged.len() != 1
                || !flagged[0].starts_with(&format!("{copy_line}:"))
                || !flagged[0].ends_with(&extra)
            {
                failures.push(format!("{case}: not the copy alone flagged: {flagged:?}"));
            }
            let waits = state
                .sync()
                .conversion_problems
                .get_untracked()
                .iter()
                .any(|problem| matches!(&problem.text, EditAsText::NotSaved { .. }));
            if !waits {
                failures.push(format!(
                    "{case}: the save does not wait; the host heard {:?}",
                    heard.get_untracked()
                ));
            }
        }
    }
    report(failures);
}

/// A copy of the placeholder in another field, its own field keeping it,
/// is flagged and saved as before: the secret is still where it was.
#[test]
fn a_placeholder_copied_to_another_field_in_an_xml_view_still_saves() {
    let mut failures = Vec::new();
    let tokens = &HOSTS[0];
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
    ] {
        let mut editor = TestEditor::new(language, &host(language, tokens.same));
        editor.edit_as = Some(Language::XML);
        editor.secrets = Some(tokens.options());
        let (_owner, state) = editor.mount();
        let before = state.doc().text.get_untracked();
        let after = before.replacen(
            "<name>app</name>",
            &format!("<name>{}</name>", tokens.same),
            1,
        );
        let at = utf16(&after, after.len());
        state.commit(Edit::new_units(before, (0, 0), after.clone(), at));
        state.recheck_tokens();
        convert_now(state, after, Language::XML);
        if !state.sync().conversion_problems.get_untracked().is_empty() {
            failures.push(format!(
                "{language}: waits: {:?}",
                state.sync().conversion_problems.get_untracked()
            ));
        }
    }
    report(failures);
}
