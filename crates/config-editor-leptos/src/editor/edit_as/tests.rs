//! When the Edit-as notice shows, per pair of languages, and that what it
//! says is what a save does.

use super::super::bridge::Bridge;
use super::super::languages::Language;
use super::notice::holds_comments;
use super::{EditAsProblem, EditAsText, NoticeReasons, SaveError};
use crate::EditorStrings;

/// Every built-in language compiled in.
fn languages() -> Vec<Language> {
    Language::ALL.to_vec()
}

/// The same small document in `language`: without comments, with one, and
/// with a comment as a user would type it in the edited view.
fn samples(language: Language) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match language.label() {
        "YAML" => (
            "listen: 1\nname: a\n",
            Some("# the port\nlisten: 1\nname: a\n"),
            Some("listen: 1 # typed note\nname: a\n"),
        ),
        "JSON" => ("{\n  \"listen\": 1,\n  \"name\": \"a\"\n}\n", None, None),
        "TOML" => (
            "listen = 1\nname = \"a\"\n",
            Some("# the port\nlisten = 1\nname = \"a\"\n"),
            Some("listen = 1 # typed note\nname = \"a\"\n"),
        ),
        "HCL" => (
            "listen = 1\nname = \"a\"\n",
            Some("/* the port */\nlisten = 1\nname = \"a\"\n"),
            Some("listen = 1 // typed note\nname = \"a\"\n"),
        ),
        "XML" => (
            "<listen>1</listen>\n<name>a</name>\n",
            Some("<!-- the port -->\n<listen>1</listen>\n<name>a</name>\n"),
            Some("<listen>1</listen> <!-- typed note -->\n<name>a</name>\n"),
        ),
        other => panic!("no sample for {other}"),
    }
}

/// Text that looks like a comment but is not one, per language.
fn look_alikes(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &[
            "s: |\n  # not a comment\nt: 1\n",
            "s: \"a # b\"\n",
            "color: a#b\n",
        ],
        "JSON" => &["{\"a\": \"# b // c\"}\n"],
        "TOML" => &["s = \"\"\"\n# not a comment\n\"\"\"\n", "s = \"a # b\"\n"],
        "HCL" => &[
            "url = \"http://x/*y*/\"\n",
            "s = <<EOT\n# not a comment\nEOT\n",
        ],
        "XML" => &["<s>a # b</s>\n"],
        _ => &[],
    }
}

#[test]
fn comments_are_found_and_look_alikes_are_not() {
    let mut failures = Vec::new();
    for language in languages() {
        let behavior = language.behavior();
        let (plain, commented, typed) = samples(language);
        if behavior.has_comments(plain) {
            failures.push(format!("{language}: comment found in {plain:?}"));
        }
        for text in [commented, typed].into_iter().flatten() {
            if !behavior.has_comments(text) {
                failures.push(format!("{language}: no comment found in {text:?}"));
            }
        }
        for text in look_alikes(language) {
            if behavior.has_comments(text) {
                failures.push(format!("{language}: comment found in {text:?}"));
            }
        }
        let holds = holds_comments(behavior);
        if holds != commented.is_some() {
            failures.push(format!("{language}: holds comments {holds}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The matrix: for every host and edited language, whether the notice
/// shows for the host's comments and for comments typed in the view.
#[test]
fn the_notice_shows_exactly_when_comments_are_out_of_view_or_not_saved() {
    let mut failures = Vec::new();
    for host in languages() {
        for edit in languages() {
            let (host_plain, host_commented, _) = samples(host);
            let (edit_plain, _, edit_typed) = samples(edit);
            let mut cases = vec![(host_plain, edit_plain, false, false)];
            if let Some(commented) = host_commented {
                cases.push((commented, edit_plain, true, false));
            }
            if let Some(typed) = edit_typed {
                cases.push((host_plain, typed, false, true));
                if let Some(commented) = host_commented {
                    cases.push((commented, typed, true, true));
                }
            }
            for (host_text, edit_text, host_comments, typed_comments) in cases {
                let expected = if host == edit {
                    NoticeReasons::default()
                } else {
                    NoticeReasons {
                        host_comments,
                        typed_comments,
                    }
                };
                let found = NoticeReasons::of(host, edit, host_text, edit_text);
                if found != expected {
                    failures.push(format!(
                        "{host} as {edit}: {found:?}, wanted {expected:?} for {host_text:?} / {edit_text:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// What the notice says is what happens: the host's comments are not in
/// the converted view and survive a save of an edited value; a comment
/// typed in the view never reaches the host's text. Every pair.
#[test]
fn a_save_keeps_the_hosts_comments_and_drops_typed_ones() {
    let mut failures = Vec::new();
    for host in languages() {
        for edit in languages() {
            if host == edit {
                continue;
            }
            let Some(bridge) = Bridge::new(host, edit, None) else {
                failures.push(format!("{host} as {edit}: no bridge"));
                continue;
            };
            let (plain, commented, _) = samples(host);
            if let Some(commented) = commented {
                let shown = match bridge.to_edit(commented) {
                    Ok(shown) => shown.text,
                    Err(error) => {
                        failures.push(format!("{host} as {edit}: {error}"));
                        continue;
                    }
                };
                if shown.contains("the port") {
                    failures.push(format!("{host} as {edit}: comment shown in {shown:?}"));
                }
                match bridge.to_host(&shown.replacen('1', "2", 1), commented) {
                    Ok(saved) if saved.text.contains("the port") && saved.text.contains('2') => {}
                    other => failures.push(format!(
                        "{host} as {edit}: the host's comment was not kept: {other:?}"
                    )),
                }
            }
            let shown = bridge.to_edit(plain).map(|shown| shown.text);
            let Ok(shown) = shown else {
                failures.push(format!("{host} as {edit}: {shown:?}"));
                continue;
            };
            let typed = match edit.label() {
                "XML" => shown.replacen("<listen>", "<!-- typed note -->\n<listen>", 1),
                "JSON" => continue,
                _ => format!("# typed note\n{shown}"),
            };
            if !edit.behavior().has_comments(&typed) {
                failures.push(format!("{host} as {edit}: no comment in {typed:?}"));
            }
            match bridge.to_host(&typed, plain) {
                Ok(saved) if !saved.text.contains("typed note") => {}
                other => failures.push(format!(
                    "{host} as {edit}: the typed comment was saved: {other:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_notice_names_both_languages_and_whether_the_host_has_comments() {
    let strings = EditorStrings::default();
    let both = NoticeReasons {
        host_comments: true,
        typed_comments: true,
    };
    let yaml_as_json = both.messages(&strings, Language::YAML, Language::TOML);
    assert_eq!(
        yaml_as_json,
        [
            "The YAML file's comments and formatting are not shown while editing as TOML; saving keeps them.",
            "Comments typed in TOML are not saved: the YAML file keeps only its own comments.",
        ]
    );
    let typed = NoticeReasons {
        host_comments: false,
        typed_comments: true,
    };
    assert_eq!(
        typed.messages(&strings, Language::JSON, Language::YAML),
        ["Comments typed in YAML are not saved: JSON has no comments."]
    );
    let custom = EditorStrings {
        edit_as_typed_comments_none: "{host}: keine Kommentare ({edit})".into(),
        ..EditorStrings::default()
    };
    assert_eq!(
        typed.messages(&custom, Language::JSON, Language::YAML),
        ["JSON: keine Kommentare (YAML)"]
    );
    assert!(
        NoticeReasons::default()
            .messages(&strings, Language::YAML, Language::JSON)
            .is_empty()
    );
}

#[test]
fn a_dismissed_reason_stays_hidden_and_a_new_one_shows() {
    let host_only = NoticeReasons {
        host_comments: true,
        typed_comments: false,
    };
    let both = NoticeReasons {
        host_comments: true,
        typed_comments: true,
    };
    assert!(!host_only.without(host_only).any());
    let shown = both.without(host_only);
    assert!(shown.typed_comments && !shown.host_comments);
}

/// Edit-as problems are worded when shown: other texts reword them.
#[test]
fn edit_as_problems_follow_the_strings_setting() {
    let problem = EditAsProblem {
        at: Default::default(),
        text: EditAsText::NotSaved {
            host: Language::XML,
            error: SaveError::NoNull {
                pointer: "/a".into(),
            },
        },
    };
    let english = problem.worded(&EditorStrings::default());
    assert_eq!(
        english.message,
        "Not saved as XML: XML has no null value (at /a)"
    );
    let custom = EditorStrings {
        not_saved: "Nicht als {host} gespeichert: {error}".into(),
        no_null: "{host} kennt kein null ({pointer})".into(),
        ..EditorStrings::default()
    };
    assert_eq!(
        problem.worded(&custom).message,
        "Nicht als XML gespeichert: XML kennt kein null (/a)"
    );
}

/// On a mounted editor: the notice shows for the host's comments, a
/// dismissal hides it, and a comment typed afterwards (seen after the
/// typing pause) shows it again for that reason alone.
#[test]
fn a_mounted_editor_shows_dismisses_and_reshows_the_notice() {
    use super::super::test_state::TestEditor;
    use leptos::prelude::Set;
    let (_owner, state) = TestEditor {
        edit_as: Some(Language::TOML),
        ..TestEditor::new(Language::YAML, "# the port\nlisten: 1\n")
    }
    .mount();
    let shown = state.edit_as_notice();
    assert!(shown.host_comments && !shown.typed_comments, "{shown:?}");
    state.dismiss_edit_as_notice();
    assert!(!state.edit_as_notice().any());
    state
        .problems()
        .diagnosed_text
        .set("# typed\nlisten = 1\n".to_string());
    let shown = state.edit_as_notice();
    assert!(!shown.host_comments && shown.typed_comments, "{shown:?}");
    // In the host's own language there is nothing to say.
    let (_owner, plain) = TestEditor::new(Language::YAML, "# the port\nlisten: 1\n").mount();
    assert!(!plain.edit_as_notice().any());
}
