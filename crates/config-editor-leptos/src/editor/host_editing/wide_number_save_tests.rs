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
                "{\"a\": \"x 99999999999999999999\", \"b\": 99999999999999999999}\n",
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
                "a: 99999999999999999999 # 99999999999999999999\nb: \"1 99999999999999999999\"\n",
            ],
            vec!["id: 123456789012345678901234\nb: [\n"],
        )
    } else if language == Language::HCL {
        (
            vec![
                "id = 123456789012345678901234\n",
                "a = 18446744073709551616\nb = [1, 99999999999999999999]\n",
                "a = 99999999999999999999 # 1e400\nb = \"x 1e400\"\n",
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

/// A text of `filler` plain entries and `wide` numbers past what the editor
/// holds (integers past 64 bits, both signs, and floats past the largest),
/// written in `language`.
fn many_wide(language: Language, filler: usize, wide: usize) -> String {
    let numbers = [
        "123456789012345678901234567",
        "-99999999999999999999",
        "1e400",
    ];
    let entries: Vec<String> = (0..filler)
        .map(|index| (format!("f{index}"), index.to_string()))
        .chain((0..wide).map(|index| (format!("w{index}"), numbers[index % 3].to_string())))
        .map(|(key, value)| {
            if language == Language::JSON {
                format!("\"{key}\": {value}")
            } else if language == Language::YAML {
                format!("{key}: {value}")
            } else {
                format!("{key} = {value}")
            }
        })
        .collect();
    if language == Language::JSON {
        format!("{{\n{}\n}}\n", entries.join(",\n"))
    } else {
        format!("{}\n", entries.join("\n"))
    }
}

/// Whether a text with many numbers past what the editor holds saves is
/// told from a few reads of it, however many there are (each read in
/// turn with the ones before stood in for: 8 s for 700 in 5,000 lines).
#[test]
fn many_numbers_past_what_the_editor_holds_are_judged_in_a_few_reads() {
    use super::super::work_count::{Work, counted};
    let mut failures = Vec::new();
    for language in [Language::JSON, Language::YAML, Language::HCL] {
        for filler in [200, 2000] {
            for wide in [0, 30, 300] {
                let text = many_wide(language, filler, wide);
                let (saves, parsed) = counted(Work::ParsedBytes, || language.saves(&text));
                if !saves {
                    failures.push(format!(
                        "{language}: {filler} entries, {wide} wide: refused"
                    ));
                }
                // None: one read; any number of them: a few.
                if parsed
                    > if wide == 0 {
                        text.len()
                    } else {
                        3 * text.len()
                    }
                {
                    failures.push(format!(
                        "{language}: {filler} entries, {wide} wide: {parsed} bytes parsed of {}",
                        text.len()
                    ));
                }
                // A syntax error after them all: refused, as soon.
                let broken = format!(
                    "{text}{}",
                    if language == Language::JSON {
                        "]"
                    } else {
                        "x = [\n"
                    }
                );
                let (saves, parsed) = counted(Work::ParsedBytes, || language.saves(&broken));
                if saves || parsed > 3 * broken.len() {
                    failures.push(format!(
                        "{language}: {filler} entries, {wide} wide, broken: saves {saves}, {parsed} bytes parsed of {}",
                        broken.len()
                    ));
                }
            }
        }
    }
    // TOML holds its integers to 64 bits: there they still wait.
    let filler: String = (0..50).map(|index| format!("a{index} = 1\n")).collect();
    let toml = format!("{filler}id = 9223372036854775808\n");
    assert!(Language::TOML.saves(&filler));
    assert!(!Language::TOML.saves(&toml));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
