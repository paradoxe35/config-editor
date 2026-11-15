//! P2/P4: an edit in an Edit-as view of a large host holding secret
//! placeholders that changes no protected value reaches the host at once:
//! nothing is held, the change event and the save carry it, and the host
//! text changes on that value's line only.

use leptos::prelude::*;

use crate::editor::Language;
use crate::editor::diagnostics::build_schema;
use crate::editor::edit::Edit;
use crate::editor::host_editing::SaveRequest;
use crate::editor::sync::convert_now;
use crate::editor::test_state::TestEditor;

/// The demo's large sample in `language`.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => include_str!("../../../../tests/fixtures/samples/tunnel.large.yaml"),
        "JSON" => include_str!("../../../../tests/fixtures/samples/tunnel.large.json"),
        "TOML" => include_str!("../../../../tests/fixtures/samples/tunnel.large.toml"),
        "HCL" => include_str!("../../../../tests/fixtures/samples/tunnel.large.hcl"),
        _ => include_str!("../../../../tests/fixtures/samples/tunnel.large.xml"),
    }
}

#[test]
fn one_value_changed_in_a_view_of_a_large_host_with_secrets_saves_one_line() {
    for (stored, view) in [
        (Language::HCL, Language::YAML),
        (Language::YAML, Language::JSON),
        (Language::TOML, Language::HCL),
        (Language::JSON, Language::TOML),
        (Language::XML, Language::YAML),
    ] {
        let host_text = sample(stored);
        assert!(
            host_text.contains("«secret "),
            "{stored}: the sample has placeholders"
        );
        let changes = RwSignal::new(Vec::<String>::new());
        let saves = RwSignal::new(Vec::<String>::new());
        let mut editor = TestEditor::new(stored, host_text);
        editor.edit_as = Some(view);
        // The demo's schema (the tunnel's).
        editor.schema = Some(std::sync::Arc::new(
            build_schema(include_str!(
                "../../../../tests/fixtures/tunnel.schema.json"
            ))
            .expect("the tunnel schema"),
        ));
        editor.on_change = Some(Callback::new(move |text| {
            changes.update(|all| all.push(text));
        }));
        editor.on_save = Some(Callback::new(move |request: SaveRequest| {
            saves.update(|all| all.push(request.text));
        }));
        let (_owner, state) = editor.mount();
        let shown = state.doc().text.get_untracked();
        let at = shown.find("7835").expect("the port on screen");
        let after = shown.replacen("7835", "7999", 1);
        let units = shown[..at].encode_utf16().count();
        assert!(
            state.commit(Edit::input(
                shown,
                (units, units + 4),
                after.clone(),
                (units + 4, units + 4),
                "insertFromPaste".into(),
                0.0,
            )),
            "{stored} as {view}: the edit is taken"
        );
        convert_now(state, after, view);
        assert!(
            !state.changes_held(),
            "{stored} as {view}: held ({:?})",
            state.save_waits()
        );
        state.save();
        let saved = saves.get_untracked().last().cloned().unwrap_or_else(|| {
            panic!(
                "{stored} as {view}: nothing saved ({:?})",
                state.save_waits()
            )
        });
        assert_eq!(
            changes.get_untracked().last(),
            Some(&saved),
            "{stored} as {view}: the change event carries the save"
        );
        let (before, after): (Vec<&str>, Vec<&str>) =
            (host_text.lines().collect(), saved.lines().collect());
        assert_eq!(before.len(), after.len(), "{stored} as {view}");
        let changed: Vec<_> = before
            .iter()
            .zip(&after)
            .filter(|(old, new)| old != new)
            .collect();
        assert_eq!(changed.len(), 1, "{stored} as {view}: {changed:?}");
        assert!(changed[0].1.contains("7999"), "{stored} as {view}");
    }
}
