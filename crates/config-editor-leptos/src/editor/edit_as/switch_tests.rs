//! An Edit-as switch as an undo step: undo goes back to the language and
//! text it left, redo returns, the caret stays on its field, and the host
//! hears the language on screen each time. Every pair of languages.

use leptos::prelude::*;

use super::super::edit::After;
use super::super::language::{ConfigLanguage, LanguageInfo, ParseError};
use super::super::languages::Language;
use super::super::paths::TextPosition;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;

/// The same document in `language`, the caret's field `/web/http` in it.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "listen: 1\nweb:\n  http: 80\n",
        "JSON" => "{\n  \"listen\": 1,\n  \"web\": {\n    \"http\": 80\n  }\n}\n",
        "TOML" => "listen = 1\n\n[web]\nhttp = 80\n",
        "HCL" => "listen = 1\n\nweb {\n  http = 80\n}\n",
        "XML" => "<listen>1</listen>\n<web>\n  <http>80</http>\n</web>\n",
        other => panic!("no sample for {other}"),
    }
}

#[test]
fn a_switch_is_undone_and_redone_with_the_caret_on_its_field() {
    let mut failures = Vec::new();
    for &host in Language::ALL {
        for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
            let text = sample(host);
            let (_owner, state) = TestEditor::new(host, text).mount();
            let heard = RwSignal::new(Vec::<Language>::new());
            state
                .sync()
                .on_edit_as_change
                .set_value(Some(Callback::new(move |language| {
                    heard.update(|heard| heard.push(language));
                })));
            let field = host
                .range_of(text, "/web/http")
                .expect("the field")
                .start
                .to_offset(text);
            state.doc().caret.set(field);
            switch_to(state, edit);
            let case = format!("{host} host, switched to {edit}");
            let shown = state.doc().text.get_untracked();
            let caret = state.doc().caret.get_untracked();
            let on_field = edit
                .path_at(&shown, TextPosition::from_offset(&shown, caret))
                .map(|path| path.to_pointer());
            if state.sync().edit_language.get_untracked() != edit {
                failures.push(format!("{case}: not switched"));
                continue;
            }
            if on_field.as_deref() != Some("/web/http") {
                failures.push(format!("{case}: the caret is on {on_field:?}"));
            }
            // Undo: the host's language and text again.
            let undo = state.history_edit(false).expect("the switch is a step");
            state.commit(undo);
            if state.sync().edit_language.get_untracked() != host
                || state.doc().text.get_untracked() != text
            {
                failures.push(format!(
                    "{case}: undo shows {} {:?}",
                    state.sync().edit_language.get_untracked(),
                    state.doc().text.get_untracked()
                ));
            }
            // Redo: the switch again.
            let redo = state.history_edit(true).expect("the switch to redo");
            state.commit(redo);
            if state.sync().edit_language.get_untracked() != edit
                || state.doc().text.get_untracked() != shown
            {
                failures.push(format!("{case}: redo does not switch again"));
            }
            if heard.get_untracked() != vec![edit, host, edit] {
                failures.push(format!(
                    "{case}: the host heard {:?}",
                    heard.get_untracked()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The caret inside the key `http` (after `ht`) in `text`.
fn in_key(text: &str) -> usize {
    let at = text.find("http").expect("the key");
    text[..at].encode_utf16().count() + 2
}

/// Undoing a switch leaves a caret where it was, nothing selected (round 16
/// selected the whole document, so the next key replaced it); the switch
/// and its redo keep the caret's column in the key.
#[test]
fn undoing_a_switch_restores_the_caret_and_redo_keeps_its_column() {
    let mut failures = Vec::new();
    for &host in Language::ALL {
        for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
            let text = sample(host);
            let (_owner, state) = TestEditor::new(host, text).mount();
            let caret = in_key(text);
            state.doc().caret.set(caret);
            switch_to(state, edit);
            let case = format!("{host} host, switched to {edit}");
            let shown = state.doc().text.get_untracked();
            if state.doc().caret.get_untracked() != in_key(&shown) {
                failures.push(format!(
                    "{case}: the caret is at {} of {shown:?}",
                    state.doc().caret.get_untracked()
                ));
            }
            let undo = state.history_edit(false).expect("the switch is a step");
            match undo.after {
                After::Select {
                    anchor, caret: at, ..
                } if anchor == caret && at == caret => {}
                other => failures.push(format!("{case}: undo leaves {other:?}, not {caret}")),
            }
            state.commit(undo);
            let redo = state.history_edit(true).expect("the switch to redo");
            let column = in_key(&shown);
            match redo.after {
                After::Select {
                    anchor, caret: at, ..
                } if anchor == column && at == column => {}
                other => failures.push(format!("{case}: redo leaves {other:?}, not {column}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A language registered with `parse` alone: it reads documents but cannot
/// write them.
struct ParseOnly;

impl ConfigLanguage for ParseOnly {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "ParseOnly",
            icon: "P",
            extensions: &[],
        }
    }

    fn parse(&self, _source: &str) -> Result<serde_json::Value, ParseError> {
        Ok(serde_json::json!({"listen": 1}))
    }
}

static PARSE_ONLY: ParseOnly = ParseOnly;

#[test]
fn a_switch_from_or_into_a_language_that_cannot_write_is_refused_at_once() {
    let parse_only = Language::new(&PARSE_ONLY);
    let strings = crate::EditorStrings::default();
    let mut failures = Vec::new();
    for (host, edit, text) in [
        (parse_only, Language::YAML, "anything"),
        (Language::YAML, parse_only, sample(Language::YAML)),
    ] {
        let (_owner, state) = TestEditor::new(host, text).mount();
        switch_to(state, edit);
        let case = format!("{host} host, switched to {edit}");
        if state.sync().edit_language.get_untracked() != host
            || state.doc().text.get_untracked() != text
        {
            failures.push(format!("{case}: switched"));
        }
        if state.sync().pending_switch.get_untracked().is_some() {
            failures.push(format!("{case}: left pending"));
        }
        let said: Vec<String> = state
            .sync()
            .load_problems
            .get_untracked()
            .iter()
            .map(|problem| problem.worded(&strings).message)
            .collect();
        let expected = format!("Cannot edit as {edit}: ParseOnly documents cannot be written");
        if said != [expected] {
            failures.push(format!("{case}: says {said:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Undoing a switch back to the host's own text tells the host nothing
/// (round 26 sent the unchanged value again); after an edit the host
/// heard, undoing back to its first text does tell it. Every pair.
#[test]
fn undoing_a_switch_back_to_the_hosts_text_sends_nothing() {
    let mut failures = Vec::new();
    for &host in Language::ALL {
        for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
            let text = sample(host);
            let heard = RwSignal::new(Vec::<String>::new());
            let mut editor = TestEditor::new(host, text);
            editor.on_change = Some(Callback::new(move |text| {
                heard.update(|heard| heard.push(text));
            }));
            let (_owner, state) = editor.mount();
            switch_to(state, edit);
            let undo = state.history_edit(false).expect("the switch is a step");
            state.commit(undo);
            let redo = state.history_edit(true).expect("the switch to redo");
            state.commit(redo);
            let undo = state.history_edit(false).expect("the switch again");
            state.commit(undo);
            if !heard.get_untracked().is_empty() {
                failures.push(format!(
                    "{host} host, switched to {edit}: heard {:?}",
                    heard.get_untracked()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What an undo back across a switch brings that the host does not have
/// (an edit it heard, undone with the switch before the pause): it hears
/// its first text again.
#[test]
fn undoing_a_heard_edit_and_the_switch_sends_the_hosts_first_text() {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::YAML, "a: 1\n");
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (_owner, state) = editor.mount();
    switch_to(state, Language::TOML);
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    state.commit(super::super::edit::Edit::new(
        before,
        (0, end),
        "a = 2\n".to_string(),
        6,
    ));
    super::super::sync::convert_now(state, "a = 2\n".to_string(), Language::TOML);
    for _ in 0..2 {
        let undo = state.history_edit(false).expect("a step");
        state.commit(undo);
    }
    assert_eq!(state.sync().edit_language.get_untracked(), Language::YAML);
    assert_eq!(heard.get_untracked(), vec!["a: 2\n", "a: 1\n"]);
}
