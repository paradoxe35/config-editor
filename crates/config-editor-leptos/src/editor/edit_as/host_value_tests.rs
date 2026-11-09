//! A new host text the language on screen cannot hold (TOML has no `null`,
//! XML no `x/y` element, any text that does not parse): it is shown in the
//! host's language and read as it, the host hears that language, edits
//! reach `on_change`, and the switch waits until the document converts.
//! Every pair of languages.

use leptos::prelude::*;

use super::super::bridge::Bridge;
use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{switch_to, take_host_value};
use super::super::test_state::TestEditor;
use super::problems::EditAsText;

/// A document every language holds, in `language`.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "a: 1\n",
        "JSON" => "{\n  \"a\": 1\n}\n",
        "TOML" => "a = 1\n",
        "HCL" => "a = 1\n",
        "XML" => "<a>1</a>\n",
        other => panic!("no sample for {other}"),
    }
}

/// Host texts some other language cannot hold: one that does not parse
/// (no language holds it), then values only some languages lack.
fn unconvertible(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &["a: [\n", "a: 1\nport: ~\n", "a: 1\n\"x/y\": 1\n"],
        "JSON" => &[
            "{\n  \"a\": \n",
            "{\n  \"a\": 1,\n  \"port\": null\n}\n",
            "{\n  \"x/y\": 1\n}\n",
        ],
        "TOML" => &["a = \n", "\"x/y\" = 1\n"],
        "HCL" => &["a = \n", "a = 1\nport = null\n"],
        "XML" => &["<a>\n", "<a>1<b>2</b></a>\n"],
        other => panic!("no texts for {other}"),
    }
}

/// `host` edited as `edit`, with what `on_change` and `on_edit_as_change`
/// heard.
fn mounted(
    host: Language,
    edit: Language,
) -> (
    Owner,
    EditorState,
    RwSignal<Vec<String>>,
    RwSignal<Vec<Language>>,
) {
    let changes = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, sample(host));
    editor.edit_as = Some(edit);
    editor.on_change = Some(Callback::new(move |text| {
        changes.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    let languages = RwSignal::new(Vec::<Language>::new());
    state
        .sync()
        .on_edit_as_change
        .set_value(Some(Callback::new(move |language| {
            languages.update(|heard| heard.push(language));
        })));
    (owner, state, changes, languages)
}

#[test]
fn a_host_text_the_view_cannot_hold_is_edited_in_the_hosts_language_until_it_converts() {
    let mut failures = Vec::new();
    let mut cases = 0;
    for &host in Language::ALL {
        for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
            let bridge = Bridge::new(host, edit, None).expect("two languages");
            for &text in unconvertible(host) {
                if bridge.to_edit(text).is_ok() {
                    continue;
                }
                cases += 1;
                let case = format!("{host} host as {edit}, the host sends {text:?}");
                let (_owner, state, changes, languages) = mounted(host, edit);
                if state.sync().edit_language.get_untracked() != edit {
                    failures.push(format!("{case}: the sample does not convert"));
                    continue;
                }
                take_host_value(state, text.to_string());
                let on_screen = state.sync().edit_language.get_untracked();
                if on_screen != host || state.doc().text.get_untracked() != text {
                    failures.push(format!(
                        "{case}: {on_screen} on screen, {:?}",
                        state.doc().text.get_untracked()
                    ));
                    continue;
                }
                if state.sync().pending_switch.get_untracked() != Some(edit) {
                    failures.push(format!("{case}: no switch waits"));
                }
                if languages.get_untracked() != vec![host] {
                    failures.push(format!(
                        "{case}: the host heard {:?}",
                        languages.get_untracked()
                    ));
                }
                let waits = state.sync().load_problems.with_untracked(|problems| {
                    matches!(
                        problems.as_slice(),
                        [problem] if matches!(problem.text, EditAsText::CannotSwitch { edit: to, .. } if to == edit)
                    )
                });
                if !waits {
                    failures.push(format!(
                        "{case}: problems {:?}",
                        state.sync().load_problems.get_untracked()
                    ));
                }
                if !state.sync().in_sync.get_untracked()
                    || !state
                        .doc()
                        .history
                        .with_untracked(|history| !history.can_undo())
                {
                    failures.push(format!(
                        "{case}: out of sync, or steps of the other language kept"
                    ));
                }
                // Fixed by typing: the host hears the edit in its language.
                let before = state.doc().text.get_untracked();
                let end = before.encode_utf16().count();
                let fixed = sample(host);
                state.commit(Edit::new(before, (0, end), fixed.to_string(), fixed.len()));
                if changes.get_untracked().last().map(String::as_str) != Some(fixed) {
                    failures.push(format!(
                        "{case}: on_change heard {:?}",
                        changes.get_untracked()
                    ));
                }
                // The switch waiting is made (as the editor retries it after
                // the edit).
                switch_to(state, edit);
                if state.sync().edit_language.get_untracked() != edit
                    || state.sync().pending_switch.get_untracked().is_some()
                    || languages.get_untracked() != vec![host, edit]
                {
                    failures.push(format!(
                        "{case}: after the fix {} on screen, heard {:?}",
                        state.sync().edit_language.get_untracked(),
                        languages.get_untracked()
                    ));
                }
            }
        }
    }
    let pairs = Language::ALL.len() * (Language::ALL.len() - 1);
    assert!(cases > pairs, "{cases} cases for {pairs} pairs");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Asking for the host's language while such a text waits ends the wait
/// (round 26 answered "Cannot switch to YAML yet"); a host text that
/// converts again makes the switch.
#[test]
fn a_waiting_switch_ends_on_asking_for_the_hosts_language_or_goes_on_a_text_that_converts() {
    let (_owner, state, _, languages) = mounted(Language::YAML, Language::TOML);
    take_host_value(state, "a: ~\n".to_string());
    state.ask_edit_as(Language::YAML);
    assert_eq!(state.sync().pending_switch.get_untracked(), None);
    assert!(state.sync().load_problems.get_untracked().is_empty());
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);

    let (_owner, state, _, languages_too) = mounted(Language::YAML, Language::TOML);
    take_host_value(state, "a: ~\n".to_string());
    take_host_value(state, "a: 2\n".to_string());
    // Still in YAML, read as YAML: the retry after the pause switches.
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    assert_eq!(state.doc().text.get_untracked(), "a: 2\n");
    switch_to(state, Language::TOML);
    assert_eq!(state.doc().text.get_untracked(), "a = 2\n");
    assert_eq!(languages.get_untracked(), vec![Language::YAML]);
    assert_eq!(
        languages_too.get_untracked(),
        vec![Language::YAML, Language::TOML]
    );
}
