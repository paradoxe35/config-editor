//! A save never sends text that does not parse, in any language: typed
//! invalid text and a host text its reader refuses wait, in the host's own
//! view and in an Edit-as view (round 19's `D19.docs-1`).

#![cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::SaveRequest;

/// A valid text in each language, and texts in it its reader refuses: a
/// typed half-written bracket, and for XML what XML 1.0 cannot hold (a
/// control character, raw or as a reference, in text, an attribute or
/// CDATA, references to U+FFFE, NUL or a surrogate, and a namespace prefix
/// no declaration binds).
fn cases() -> Vec<(Language, &'static str, Vec<&'static str>)> {
    vec![
        (Language::YAML, "a: 1\n", vec!["a: [\n"]),
        (Language::JSON, "{\"a\": 1}\n", vec!["{\"a\": [}\n"]),
        (Language::TOML, "a = 1\n", vec!["a = [\n"]),
        (Language::HCL, "a = 1\n", vec!["a = [\n"]),
        (
            Language::XML,
            "<r><s>x</s><f>1</f></r>\n",
            vec![
                "<r><s><</s><f>1</f></r>\n",
                "<r><s>\u{1}</s><f>1</f></r>\n",
                "<r><s>&#1;</s><f>1</f></r>\n",
                "<r><s>&#x1;</s><f>2</f></r>\n",
                "<r><s>&#0;</s><f>1</f></r>\n",
                "<r><s>&#xFFFE;</s><f>1</f></r>\n",
                "<r><s>&#xD800;</s><f>1</f></r>\n",
                "<r><s>&#x110000;</s><f>1</f></r>\n",
                "<r><s>\u{fffe}</s><f>1</f></r>\n",
                "<r><s k=\"&#1;\">x</s><f>1</f></r>\n",
                "<r><s k=\"&#0;\">x</s><f>1</f></r>\n",
                "<r><s><![CDATA[\u{1}]]></s><f>1</f></r>\n",
                // A prefix no declaration binds, on an element or an
                // attribute, or out of its declaration's scope.
                "<r><p:s>x</p:s><f>1</f></r>\n",
                "<r><s p:k=\"1\">x</s><f>1</f></r>\n",
                "<r><s xmlns:p=\"urn:p\"/><p:s>x</p:s><f>1</f></r>\n",
            ],
        ),
    ]
}

/// Mounts `text` in `host` (shown as `edit`) and records what `on_save`
/// gets.
pub(super) fn mount(
    host: Language,
    edit: Language,
    text: &str,
) -> (
    Owner,
    super::super::state::EditorState,
    RwSignal<Vec<String>>,
) {
    let saves = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, text);
    editor.edit_as = (edit != host).then_some(edit);
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        saves.update(|all| all.push(request.text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, saves)
}

pub(super) fn replace_all(state: super::super::state::EditorState, text: &str) {
    let before = state.doc().text.get_untracked();
    let end = before.len();
    state.commit(Edit::new(before, (0, end), text.to_string(), text.len()));
}

#[test]
fn typed_invalid_text_is_never_saved_in_any_language() {
    let mut failures = Vec::new();
    for (host, valid, invalid) in cases() {
        for bad in invalid {
            let (_owner, state, saves) = mount(host, host, valid);
            replace_all(state, bad);
            if state.save() || !saves.get_untracked().is_empty() {
                failures.push(format!(
                    "{host}: typed {bad:?} saved {:?}",
                    saves.get_untracked()
                ));
            }
            // Repaired, it saves.
            replace_all(state, valid);
            if !state.save() || saves.get_untracked() != vec![valid.to_string()] {
                failures.push(format!(
                    "{host}: repaired {valid:?} saved {:?}",
                    saves.get_untracked()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_host_text_that_does_not_parse_is_not_saved_after_an_unrelated_edit() {
    let mut failures = Vec::new();
    for (host, _, invalid) in cases() {
        for bad in invalid {
            let (_owner, state, saves) = mount(host, host, bad);
            // An edit away from the problem: a line break added at the end.
            let before = state.doc().text.get_untracked();
            let end = before.len();
            state.commit(Edit::new(
                before.clone(),
                (end, end),
                format!("{before}\n"),
                end + 1,
            ));
            if state.save() || !saves.get_untracked().is_empty() {
                failures.push(format!("{host}: {bad:?} saved {:?}", saves.get_untracked()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn invalid_text_typed_in_an_edit_as_view_is_never_saved() {
    let mut failures = Vec::new();
    for (host, valid, _) in cases() {
        for (edit, _, invalid) in cases() {
            if edit == host {
                continue;
            }
            for bad in invalid {
                let (_owner, state, saves) = mount(host, edit, valid);
                replace_all(state, bad);
                if state.save() || !saves.get_untracked().is_empty() {
                    failures.push(format!(
                        "{host} as {edit}: typed {bad:?} saved {:?}",
                        saves.get_untracked()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
