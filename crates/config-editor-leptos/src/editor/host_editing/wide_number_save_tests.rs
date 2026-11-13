//! A number the editor cannot hold exactly (an integer past 64 bits, a
//! float past the largest) in a language whose specification allows it
//! (JSON, YAML, HCL): the text saves in its own language, its problem
//! still shown, so a file holding one can be saved again. TOML limits its
//! integers to 64 bits: there it is invalid and waits. A view of another
//! language never writes such a number rounded: it waits.

#![cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]

use super::super::languages::Language;
use leptos::prelude::*;

use super::save_tests::{mount, replace_all};

/// Texts holding numbers past what the editor holds, with a syntax error
/// besides or not.
fn wide(language: Language) -> (Vec<&'static str>, Vec<&'static str>) {
    if language == Language::JSON {
        (
            vec![
                "{\"id\": 123456789012345678901234}\n",
                "{\"id\": -9223372036854775809}\n",
                "{\"n\": 1e400}\n",
                "{\"a\": 18446744073709551616, \"b\": [1e400, -99999999999999999999]}\n",
            ],
            vec![
                "{\"id\": 123456789012345678901234, }\n",
                "{\"n\": 1e400 \"m\": 1}\n",
            ],
        )
    } else if language == Language::YAML {
        (
            vec![
                "id: 123456789012345678901234\n",
                "id: -9223372036854775809\n",
                "a: 18446744073709551616\nb:\n  - 1\n  - -99999999999999999999\n",
            ],
            vec!["id: 123456789012345678901234\nb: [\n"],
        )
    } else if language == Language::HCL {
        (
            vec![
                "id = 123456789012345678901234\n",
                "a = 18446744073709551616\nb = [1, 99999999999999999999]\n",
            ],
            vec!["id = 123456789012345678901234\nb = [\n"],
        )
    } else if language == Language::TOML {
        (
            Vec::new(),
            vec![
                "id = 123456789012345678901234\n",
                "id = 9223372036854775808\n",
            ],
        )
    } else {
        (Vec::new(), Vec::new())
    }
}

#[test]
fn a_number_past_what_the_editor_holds_saves_where_the_language_allows_it() {
    let mut failures = Vec::new();
    for language in [
        Language::JSON,
        Language::YAML,
        Language::HCL,
        Language::TOML,
    ] {
        let (saved, waits) = wide(language);
        for text in saved {
            let problems = crate::check_syntax(language, text);
            if problems.is_empty() {
                failures.push(format!("{language}: {text:?} shows no problem"));
            }
            // Loaded so, and typed so.
            let (_owner, state, saves) = mount(language, language, text);
            if !state.save() || saves.get_untracked() != vec![text.to_string()] {
                failures.push(format!(
                    "{language}: loaded {text:?} saved {:?}",
                    saves.get_untracked()
                ));
            }
            let (_owner, state, saves) = mount(language, language, "\n");
            replace_all(state, text);
            if !state.save() || saves.get_untracked() != vec![text.to_string()] {
                failures.push(format!(
                    "{language}: typed {text:?} saved {:?}",
                    saves.get_untracked()
                ));
            }
        }
        for text in waits {
            let (_owner, state, saves) = mount(language, language, "\n");
            replace_all(state, text);
            if state.save() || !saves.get_untracked().is_empty() {
                failures.push(format!(
                    "{language}: {text:?} saved {:?}",
                    saves.get_untracked()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_view_never_saves_a_number_past_what_the_editor_holds() {
    let mut failures = Vec::new();
    for edit in [Language::JSON, Language::YAML, Language::HCL] {
        for host in [Language::YAML, Language::JSON, Language::TOML] {
            if host == edit {
                continue;
            }
            let valid = if host == Language::JSON {
                "{\"a\": 1}\n"
            } else if host == Language::TOML {
                "a = 1\n"
            } else {
                "a: 1\n"
            };
            for text in wide(edit).0 {
                let (_owner, state, saves) = mount(host, edit, valid);
                replace_all(state, text);
                if state.save() || !saves.get_untracked().is_empty() {
                    failures.push(format!(
                        "{host} as {edit}: {text:?} saved {:?}",
                        saves.get_untracked()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
