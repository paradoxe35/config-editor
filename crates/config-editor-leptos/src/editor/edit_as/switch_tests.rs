//! An Edit-as switch as an undo step: undo goes back to the language and
//! text it left, redo returns, the caret stays on its field, and the host
//! hears the language on screen each time. Every pair of languages.

use leptos::prelude::*;

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
                .sync
                .on_edit_as_change
                .set_value(Some(Callback::new(move |language| {
                    heard.update(|heard| heard.push(language));
                })));
            let field = host
                .range_of(text, "/web/http")
                .expect("the field")
                .start
                .to_offset(text);
            state.doc.caret.set(field);
            switch_to(state, edit);
            let case = format!("{host} host, switched to {edit}");
            let shown = state.doc.text.get_untracked();
            let caret = state.doc.caret.get_untracked();
            let on_field = edit
                .path_at(&shown, TextPosition::from_offset(&shown, caret))
                .map(|path| path.to_pointer());
            if state.sync.edit_language.get_untracked() != edit {
                failures.push(format!("{case}: not switched"));
                continue;
            }
            if on_field.as_deref() != Some("/web/http") {
                failures.push(format!("{case}: the caret is on {on_field:?}"));
            }
            // Undo: the host's language and text again.
            let undo = state.history_edit(false).expect("the switch is a step");
            state.commit(undo);
            if state.sync.edit_language.get_untracked() != host
                || state.doc.text.get_untracked() != text
            {
                failures.push(format!(
                    "{case}: undo shows {} {:?}",
                    state.sync.edit_language.get_untracked(),
                    state.doc.text.get_untracked()
                ));
            }
            // Redo: the switch again.
            let redo = state.history_edit(true).expect("the switch to redo");
            state.commit(redo);
            if state.sync.edit_language.get_untracked() != edit
                || state.doc.text.get_untracked() != shown
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
